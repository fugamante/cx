use crate::llm::{
    HttpRequestOptions, LlmRunError, http_body_opts, http_plain_opts, http_raw_opts,
    run_llama_cpp_plain, run_mlx_plain, run_ollama_plain, run_primary_jsonl, run_primary_plain,
    wrap_agent_text_as_jsonl,
};
use crate::process::run_command_output_with_timeout;
use crate::runtime::{
    llm_backend, llm_model, resolve_llama_cpp_model_for_run, resolve_mlx_model_for_run,
    resolve_ollama_model_for_run,
};
use base64::Engine;
use serde_json::{Value, json};
use std::env;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderStatus {
    Stable,
    Experimental,
    StubUnimplemented,
}

impl ProviderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Experimental => "experimental",
            Self::StubUnimplemented => "stub_unimplemented",
        }
    }

    pub fn to_log_field(self) -> Option<&'static str> {
        match self {
            Self::Stable => None,
            _ => Some(self.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderCapabilities {
    pub jsonl_native: bool,
    pub schema_strict: bool,
    pub transport: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendExperimentCapabilities {
    pub turboquant_runtime_support: &'static str,
    pub turboquant_backend_role: &'static str,
    pub turboquant_metric_kind: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendRuntimeCapabilities {
    pub model_registry: Option<bool>,
    pub model_aliases: Option<bool>,
    pub local_model_path: Option<bool>,
    pub resident_server: Option<bool>,
    pub openai_compatible: Option<bool>,
    pub anthropic_compatible: Option<bool>,
    pub supports_batching: Option<bool>,
    pub supports_tool_calling: Option<bool>,
    pub supports_vlm: Option<bool>,
    pub supports_embeddings: Option<bool>,
    pub supports_reranking: Option<bool>,
    pub cache_metric_kind: Option<&'static str>,
    pub supports_persisted_kv_restore: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpTlsPosture {
    pub https_required: bool,
    pub local_http_exception: bool,
    pub allowlist_active: bool,
    pub allowed_hosts: Vec<String>,
    pub pinned_pubkey: bool,
    pub ca_bundle: bool,
    pub client_cert: bool,
    pub client_key: bool,
    pub min_tls_version: &'static str,
    pub follow_redirects: bool,
    pub max_redirects: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpSecretSource {
    Env,
    File,
}

impl HttpSecretSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::File => "file",
        }
    }
}

fn normalized_backend_name(raw: &str) -> &'static str {
    match raw.to_ascii_lowercase().as_str() {
        "primary" => "primary",
        "ollama" => "ollama",
        "llamacpp" | "llama.cpp" | "llama_cpp" => "llamacpp",
        "mlx" => "mlx",
        _ => "primary",
    }
}

fn adapter_override() -> Option<String> {
    env::var("CX_PROVIDER_ADAPTER")
        .ok()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_lowercase())
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn secret_perm_ok(path: &std::path::Path) -> Result<(), LlmRunError> {
    #[cfg(unix)]
    {
        let mode = fs::metadata(path)
            .map_err(|e| {
                LlmRunError::message(format!(
                    "http-curl adapter could not stat secret file {}: {e}",
                    path.display()
                ))
            })?
            .permissions()
            .mode()
            & 0o777;
        if mode & 0o077 != 0 {
            return Err(LlmRunError::message(format!(
                "http-curl adapter secret file {} must not be group/world readable or writable",
                path.display()
            )));
        }
    }
    Ok(())
}

fn read_secret_src(name: &str) -> Result<Option<(String, HttpSecretSource)>, LlmRunError> {
    let env_name = name.to_string();
    let file_name = format!("{name}_FILE");
    let direct = env_nonempty(&env_name);
    let file = env_nonempty(&file_name);
    if direct.is_some() && file.is_some() {
        return Err(LlmRunError::message(format!(
            "http-curl adapter secret source is ambiguous; set only one of {env_name} or {file_name}"
        )));
    }
    if let Some(value) = direct {
        return Ok(Some((value, HttpSecretSource::Env)));
    }
    let Some(path) = file else {
        return Ok(None);
    };
    let path_ref = std::path::Path::new(&path);
    secret_perm_ok(path_ref)?;
    let value = fs::read_to_string(path_ref).map_err(|e| {
        LlmRunError::message(format!(
            "http-curl adapter could not read secret file {}: {e}",
            path_ref.display()
        ))
    })?;
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LlmRunError::message(format!(
            "http-curl adapter secret file {} was empty",
            path_ref.display()
        )));
    }
    Ok(Some((value, HttpSecretSource::File)))
}

fn is_local_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.starts_with("http://localhost:")
        || u == "http://localhost"
        || u.starts_with("http://localhost/")
        || u.starts_with("http://127.0.0.1:")
        || u == "http://127.0.0.1"
        || u.starts_with("http://127.0.0.1/")
        || u.starts_with("http://[::1]:")
        || u == "http://[::1]"
        || u.starts_with("http://[::1]/")
}

fn url_host(url: &str) -> Option<String> {
    let lower = url.trim().to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(without_bracket) = host_port.strip_prefix('[') {
        let host = without_bracket.split(']').next().unwrap_or("").trim();
        return (!host.is_empty()).then(|| host.to_string());
    }
    let host = host_port.split(':').next().unwrap_or("").trim();
    (!host.is_empty()).then(|| host.to_string())
}

fn parse_http_hosts() -> Option<Vec<String>> {
    let raw = env::var("CX_HTTP_ALLOWED_HOSTS").ok()?;
    let mut hosts: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    hosts.sort();
    hosts.dedup();
    (!hosts.is_empty()).then_some(hosts)
}

fn validate_host_allowlist(url: &str) -> Result<(), LlmRunError> {
    let Some(allowed) = parse_http_hosts() else {
        return Ok(());
    };
    let Some(host) = url_host(url) else {
        return Err(LlmRunError::message(
            "http-curl adapter [http_url_host_invalid] unable to parse provider host from CX_HTTP_PROVIDER_URL".to_string(),
        ));
    };
    if allowed.iter().any(|h| h == &host) {
        return Ok(());
    }
    Err(LlmRunError::message(format!(
        "http-curl adapter [http_host_not_allowed] host '{host}' is not in CX_HTTP_ALLOWED_HOSTS"
    )))
}

fn validate_http_url(url: &str) -> Result<(), LlmRunError> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(LlmRunError::message(
            "http-curl adapter [http_url_missing] provider URL is empty".to_string(),
        ));
    }
    if trimmed.starts_with("https://") {
        return validate_host_allowlist(trimmed);
    }
    if !trimmed.starts_with("http://") {
        return Err(LlmRunError::message(
            "http-curl adapter [http_url_scheme_invalid] CX_HTTP_PROVIDER_URL must use http:// or https://".to_string(),
        ));
    }
    let require_https = env_bool("CX_HTTP_REQUIRE_HTTPS", true);
    if !require_https {
        return validate_host_allowlist(trimmed);
    }
    let allow_local_http = env_bool("CX_HTTP_ALLOW_LOCAL_HTTP", true);
    if allow_local_http && is_local_url(trimmed) {
        return validate_host_allowlist(trimmed);
    }
    validate_host_allowlist(trimmed)?;
    Err(LlmRunError::message(
        "http-curl adapter [http_url_insecure] HTTPS is required for non-local endpoints; set CX_HTTP_PROVIDER_URL to https://..., or explicitly set CX_HTTP_REQUIRE_HTTPS=0 for local testing".to_string(),
    ))
}

pub fn selected_adapter_name() -> &'static str {
    if let Some(v) = adapter_override() {
        if v == "mock" {
            return "mock";
        }
        if v == "http-stub" {
            return "http-stub";
        }
        if v == "http" || v == "http-curl" {
            return "http-curl";
        }
    }
    match normalized_backend_name(&llm_backend()) {
        "ollama" => "ollama-cli",
        "llamacpp" => "llama.cpp-cli",
        "mlx" => "mlx-python",
        _ => "primary-cli",
    }
}

pub fn selected_provider_transport() -> &'static str {
    provider_transport_for_adapter(selected_adapter_name())
}

pub fn selected_http_provider_format() -> &'static str {
    match env::var("CX_HTTP_PROVIDER_FORMAT")
        .ok()
        .map(|v| v.trim().to_lowercase())
        .as_deref()
    {
        Some("jsonl") => "jsonl",
        Some("json") => "json",
        _ => match http_profile() {
            "openai_json" => "json",
            _ => "text",
        },
    }
}

pub fn selected_http_provider_format_opt() -> Option<&'static str> {
    if selected_provider_transport() != "http" {
        return None;
    }
    Some(selected_http_provider_format())
}

pub fn selected_http_parser_mode_opt() -> Option<&'static str> {
    let format = selected_http_provider_format_opt()?;
    match format {
        "jsonl" => Some("jsonl_passthrough"),
        "json" => match http_profile_opt() {
            Some("openai_json") => Some("openai_chat_completion"),
            _ => Some("json_payload"),
        },
        _ => Some("envelope"),
    }
}

pub fn http_profile_opt() -> Option<&'static str> {
    if selected_provider_transport() != "http" {
        return None;
    }
    Some(http_profile())
}

pub fn http_profile() -> &'static str {
    match env::var("CX_HTTP_REQUEST_PROFILE")
        .ok()
        .map(|v| v.trim().to_lowercase())
        .as_deref()
    {
        Some("openai") | Some("openai-json") | Some("openai_json") => "openai_json",
        _ => "plain_text",
    }
}

pub fn http_auth_mode() -> &'static str {
    match env::var("CX_HTTP_AUTH_PROFILE")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("basic") => "basic",
        Some("header") | Some("custom_header") | Some("custom-header") => "header",
        _ => "bearer",
    }
}

pub fn http_auth_head() -> Option<String> {
    if http_auth_mode() != "header" {
        return None;
    }
    env_nonempty("CX_HTTP_AUTH_HEADER")
}

pub fn http_auth_src() -> Option<&'static str> {
    auth_secret_src().map(HttpSecretSource::as_str)
}

fn auth_secret_src() -> Option<HttpSecretSource> {
    match http_auth_mode() {
        "basic" => read_secret_src("CX_HTTP_AUTH_PASSWORD")
            .ok()
            .flatten()
            .map(|(_, source)| source),
        "header" => read_secret_src("CX_HTTP_AUTH_VALUE")
            .ok()
            .flatten()
            .or_else(|| read_secret_src("CX_HTTP_PROVIDER_TOKEN").ok().flatten())
            .map(|(_, source)| source),
        _ => read_secret_src("CX_HTTP_PROVIDER_TOKEN")
            .ok()
            .flatten()
            .map(|(_, source)| source),
    }
}

fn http_auth_pair() -> Result<Option<(String, String)>, LlmRunError> {
    match http_auth_mode() {
        "basic" => {
            let user = env_nonempty("CX_HTTP_AUTH_USERNAME").ok_or_else(|| {
                LlmRunError::message(
                    "http-curl adapter requires CX_HTTP_AUTH_USERNAME when CX_HTTP_AUTH_PROFILE=basic"
                        .to_string(),
                )
            })?;
            let pass = read_secret_src("CX_HTTP_AUTH_PASSWORD")?
                .map(|(value, _)| value)
                .ok_or_else(|| {
                LlmRunError::message(
                    "http-curl adapter requires CX_HTTP_AUTH_PASSWORD when CX_HTTP_AUTH_PROFILE=basic"
                        .to_string(),
                )
            })?;
            let creds = format!("{user}:{pass}");
            let enc = base64::engine::general_purpose::STANDARD.encode(creds);
            Ok(Some(("Authorization".to_string(), format!("Basic {enc}"))))
        }
        "header" => {
            let name = http_auth_head().ok_or_else(|| {
                LlmRunError::message(
                    "http-curl adapter requires CX_HTTP_AUTH_HEADER when CX_HTTP_AUTH_PROFILE=header"
                        .to_string(),
                )
            })?;
            let value = read_secret_src("CX_HTTP_AUTH_VALUE")?
                .or_else(|| read_secret_src("CX_HTTP_PROVIDER_TOKEN").ok().flatten())
                .map(|(value, _)| value)
                .ok_or_else(|| {
                    LlmRunError::message(
                        "http-curl adapter requires CX_HTTP_AUTH_VALUE or CX_HTTP_PROVIDER_TOKEN when CX_HTTP_AUTH_PROFILE=header"
                            .to_string(),
                    )
                })?;
            Ok(Some((name, value)))
        }
        _ => Ok(read_secret_src("CX_HTTP_PROVIDER_TOKEN")?
            .map(|(token, _)| ("Authorization".to_string(), format!("Bearer {token}")))),
    }
}

pub fn http_tlsver() -> &'static str {
    match env::var("CX_HTTP_TLS_MIN_VERSION")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("1.3") | Some("tls1.3") | Some("tlsv1.3") => "1.3",
        Some("default") | Some("system") | Some("system_default") => "default",
        _ => "1.2",
    }
}

pub fn http_follow_redirects() -> bool {
    env_bool("CX_HTTP_FOLLOW_REDIRECTS", false)
}

pub fn http_max_redirects() -> u32 {
    if !http_follow_redirects() {
        return 0;
    }
    env::var("CX_HTTP_MAX_REDIRECTS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(3)
}

pub fn tls_posture_opt() -> Option<HttpTlsPosture> {
    if selected_provider_transport() != "http" {
        return None;
    }
    let allowed_hosts = parse_http_hosts().unwrap_or_default();
    Some(HttpTlsPosture {
        https_required: env_bool("CX_HTTP_REQUIRE_HTTPS", true),
        local_http_exception: env_bool("CX_HTTP_ALLOW_LOCAL_HTTP", true),
        allowlist_active: !allowed_hosts.is_empty(),
        allowed_hosts,
        pinned_pubkey: env_nonempty("CX_HTTP_TLS_PINNEDPUBKEY").is_some(),
        ca_bundle: env_nonempty("CX_HTTP_CA_BUNDLE").is_some(),
        client_cert: env_nonempty("CX_HTTP_CLIENT_CERT").is_some(),
        client_key: env_nonempty("CX_HTTP_CLIENT_KEY").is_some(),
        min_tls_version: http_tlsver(),
        follow_redirects: http_follow_redirects(),
        max_redirects: http_max_redirects(),
    })
}

pub fn tls_posture_json() -> Option<Value> {
    let posture = tls_posture_opt()?;
    Some(json!({
        "https_required": posture.https_required,
        "local_http_exception": posture.local_http_exception,
        "allowlist_active": posture.allowlist_active,
        "allowed_hosts": posture.allowed_hosts,
        "pinned_pubkey": if posture.pinned_pubkey { "set" } else { "off" },
        "ca_bundle": if posture.ca_bundle { "set" } else { "off" },
        "client_cert": if posture.client_cert { "set" } else { "off" },
        "client_key": if posture.client_key { "set" } else { "off" },
        "min_tls_version": posture.min_tls_version,
        "follow_redirects": posture.follow_redirects,
        "max_redirects": posture.max_redirects,
    }))
}

pub fn selected_provider_status() -> Option<&'static str> {
    selected_provider_status_kind().to_log_field()
}

pub fn selected_provider_status_kind() -> ProviderStatus {
    provider_status_for_adapter(selected_adapter_name())
}

pub fn adapter_policy_value() -> Value {
    json!({
        "default_transport": "process",
        "http_transport_opt_in": true,
        "explicit_override_required_for_http": true,
        "selected_adapter": selected_adapter_name(),
        "selected_transport": selected_provider_transport(),
        "selected_status": selected_provider_status_kind().as_str(),
        "http_request_profile": http_profile_opt(),
        "http_auth_mode": if selected_provider_transport() == "http" { Some(http_auth_mode()) } else { None },
        "http_auth_header": http_auth_head(),
        "http_auth_secret_source": if selected_provider_transport() == "http" { http_auth_src() } else { None },
        "http_tls_posture": tls_posture_json(),
        "explicit_override_set": adapter_override().is_some(),
        "default_switch_guard": "two_green_ci_windows",
        "rollback_rule": "revert to process default in same release window on schema failures or transport errors"
    })
}

pub fn normalize_provider_status(raw: Option<&str>) -> ProviderStatus {
    match raw.map(str::trim).map(str::to_lowercase).as_deref() {
        Some("experimental") => ProviderStatus::Experimental,
        Some("stub_unimplemented") => ProviderStatus::StubUnimplemented,
        _ => ProviderStatus::Stable,
    }
}

fn provider_transport_for_adapter(adapter_name: &str) -> &'static str {
    match adapter_name {
        "mock" => "mock",
        "http-stub" | "http-curl" => "http",
        _ => "process",
    }
}

fn provider_status_for_adapter(adapter_name: &str) -> ProviderStatus {
    match adapter_name {
        "http-stub" => ProviderStatus::StubUnimplemented,
        "http-curl" => ProviderStatus::Experimental,
        _ => ProviderStatus::Stable,
    }
}

pub fn capabilities_for_adapter(adapter_name: &str) -> ProviderCapabilities {
    match adapter_name {
        "primary-cli" => ProviderCapabilities {
            jsonl_native: true,
            schema_strict: true,
            transport: "process",
        },
        "ollama-cli" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "process",
        },
        "llama.cpp-cli" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "process",
        },
        "mlx-python" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "process",
        },
        "mock" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "mock",
        },
        "http-stub" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "http",
        },
        "http-curl" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "http",
        },
        _ => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "process",
        },
    }
}

pub fn selected_provider_capabilities() -> ProviderCapabilities {
    capabilities_for_adapter(selected_adapter_name())
}

pub fn backend_tq_caps(raw_backend: &str) -> BackendExperimentCapabilities {
    match raw_backend.trim().to_ascii_lowercase().as_str() {
        "mlx" => BackendExperimentCapabilities {
            turboquant_runtime_support: "comparative_only",
            turboquant_backend_role: "comparative_backend",
            turboquant_metric_kind: Some("cache_nbytes"),
        },
        "llama.cpp" | "llamacpp" | "llama_cpp" => BackendExperimentCapabilities {
            turboquant_runtime_support: "reference_only",
            turboquant_backend_role: "codec_reference_backend",
            turboquant_metric_kind: Some("raw_ratio"),
        },
        _ => BackendExperimentCapabilities {
            turboquant_runtime_support: "none",
            turboquant_backend_role: "standard_provider",
            turboquant_metric_kind: None,
        },
    }
}

pub fn selected_tq_caps() -> BackendExperimentCapabilities {
    backend_tq_caps(&llm_backend())
}

pub fn backend_runtime_caps_for(
    backend_raw: &str,
    adapter_name: &str,
    http_profile_name: Option<&str>,
    provider_url: Option<&str>,
) -> BackendRuntimeCapabilities {
    let backend = normalized_backend_name(backend_raw);
    let local_backend = matches!(backend, "ollama" | "llamacpp" | "mlx");
    let is_http = provider_transport_for_adapter(adapter_name) == "http";
    let openai_profile = http_profile_name == Some("openai_json");
    let resident_server =
        resident_capability_for(backend_raw, adapter_name, http_profile_name, provider_url);
    BackendRuntimeCapabilities {
        model_registry: Some(local_backend),
        model_aliases: Some(local_backend),
        local_model_path: Some(local_backend),
        resident_server: Some(resident_server),
        openai_compatible: if is_http {
            Some(openai_profile)
        } else {
            Some(false)
        },
        anthropic_compatible: None,
        supports_batching: None,
        supports_tool_calling: None,
        supports_vlm: None,
        supports_embeddings: None,
        supports_reranking: None,
        cache_metric_kind: backend_tq_caps(backend).turboquant_metric_kind,
        supports_persisted_kv_restore: Some(false),
    }
}

pub fn selected_runtime_caps() -> BackendRuntimeCapabilities {
    let provider_url = env_nonempty("CX_HTTP_PROVIDER_URL");
    backend_runtime_caps_for(
        &llm_backend(),
        selected_adapter_name(),
        http_profile_opt(),
        provider_url.as_deref(),
    )
}

pub fn runtime_caps_json(caps: BackendRuntimeCapabilities) -> Value {
    json!({
        "model_registry": caps.model_registry,
        "model_aliases": caps.model_aliases,
        "local_model_path": caps.local_model_path,
        "resident_server": caps.resident_server,
        "openai_compatible": caps.openai_compatible,
        "anthropic_compatible": caps.anthropic_compatible,
        "supports_batching": caps.supports_batching,
        "supports_tool_calling": caps.supports_tool_calling,
        "supports_vlm": caps.supports_vlm,
        "supports_embeddings": caps.supports_embeddings,
        "supports_reranking": caps.supports_reranking,
        "cache_metric_kind": caps.cache_metric_kind,
        "supports_persisted_kv_restore": caps.supports_persisted_kv_restore
    })
}

fn models_probe_url(url: &str) -> Result<String, LlmRunError> {
    let trimmed = url.trim();
    let (scheme, rest) = if let Some(v) = trimmed.strip_prefix("https://") {
        ("https://", v)
    } else if let Some(v) = trimmed.strip_prefix("http://") {
        ("http://", v)
    } else {
        return Err(LlmRunError::message(
            "http-curl adapter [http_url_scheme_invalid] CX_HTTP_PROVIDER_URL must use http:// or https://".to_string(),
        ));
    };
    let authority = rest.split('/').next().unwrap_or(rest).trim();
    if authority.is_empty() {
        return Err(LlmRunError::message(
            "http-curl adapter [http_url_host_invalid] unable to parse provider host from CX_HTTP_PROVIDER_URL".to_string(),
        ));
    }
    Ok(format!("{scheme}{authority}/v1/models"))
}

pub fn resident_boundary_reason(
    backend_raw: &str,
    adapter_name: &str,
    http_profile_name: Option<&str>,
    provider_url: Option<&str>,
) -> &'static str {
    if provider_transport_for_adapter(adapter_name) != "http" {
        return "transport_not_http";
    }
    if normalized_backend_name(backend_raw) != "mlx" {
        return "backend_not_mlx";
    }
    if http_profile_name != Some("openai_json") {
        return "http_profile_not_openai_json";
    }
    let Some(url) = provider_url.map(str::trim).filter(|v| !v.is_empty()) else {
        return "provider_url_missing";
    };
    if !is_local_url(url) {
        return "provider_url_not_local";
    }
    "eligible"
}

fn resident_capability_for(
    backend_raw: &str,
    adapter_name: &str,
    http_profile_name: Option<&str>,
    provider_url: Option<&str>,
) -> bool {
    resident_boundary_reason(backend_raw, adapter_name, http_profile_name, provider_url)
        == "eligible"
}

pub fn selected_resident_json() -> Value {
    let backend = llm_backend();
    let adapter = selected_adapter_name();
    let profile = http_profile_opt();
    let provider_url = env_nonempty("CX_HTTP_PROVIDER_URL");
    let local_endpoint = provider_url.as_deref().map(is_local_url);
    let reason = resident_boundary_reason(&backend, adapter, profile, provider_url.as_deref());
    json!({
        "required_backend": "mlx",
        "required_transport": "http",
        "required_http_profile": "openai_json",
        "required_local_endpoint": true,
        "selected_backend": normalized_backend_name(&backend),
        "selected_adapter": adapter,
        "selected_transport": selected_provider_transport(),
        "selected_http_profile": profile,
        "provider_url_local": local_endpoint,
        "eligible": reason == "eligible",
        "reason": reason
    })
}

pub fn probe_http_models_v1() -> Result<Value, LlmRunError> {
    let backend = llm_backend();
    let adapter = selected_adapter_name();
    let profile = http_profile_opt();
    let provider_url = env_nonempty("CX_HTTP_PROVIDER_URL");
    let boundary_reason =
        resident_boundary_reason(&backend, adapter, profile, provider_url.as_deref());
    if boundary_reason != "eligible" {
        return Err(LlmRunError::message(format!(
            "http models probe requires the local MLX resident boundary (reason: {boundary_reason})"
        )));
    }
    if selected_provider_transport() != "http" {
        return Err(LlmRunError::message(
            "http models probe requires CX_PROVIDER_ADAPTER=http-curl|http-stub".to_string(),
        ));
    }
    if http_profile() != "openai_json" {
        return Err(LlmRunError::message(
            "http models probe requires CX_HTTP_REQUEST_PROFILE=openai_json".to_string(),
        ));
    }
    let url = provider_url.ok_or_else(|| {
        LlmRunError::message("http models probe requires CX_HTTP_PROVIDER_URL".to_string())
    })?;
    validate_http_url(&url)?;
    let probe_url = models_probe_url(&url)?;
    let auth = http_auth_pair()?;

    let mut cmd = std::process::Command::new("curl");
    cmd.args([
        "-sS",
        "-f",
        "-X",
        "GET",
        &probe_url,
        "-H",
        "Accept: application/json",
    ]);
    if let Some((name, value)) = auth {
        cmd.args(["-H", &format!("{name}: {value}")]);
    }
    if let Some(pinned) = env_nonempty("CX_HTTP_TLS_PINNEDPUBKEY") {
        cmd.args(["--pinnedpubkey", &pinned]);
    }
    if let Some(ca_bundle) = env_nonempty("CX_HTTP_CA_BUNDLE") {
        cmd.args(["--cacert", &ca_bundle]);
    }
    if let Some(client_cert) = env_nonempty("CX_HTTP_CLIENT_CERT") {
        cmd.args(["--cert", &client_cert]);
    }
    if let Some(client_key) = env_nonempty("CX_HTTP_CLIENT_KEY") {
        cmd.args(["--key", &client_key]);
    }
    match http_tlsver() {
        "1.3" => {
            cmd.arg("--tlsv1.3");
        }
        "1.2" => {
            cmd.arg("--tlsv1.2");
        }
        _ => {}
    }
    if http_follow_redirects() {
        cmd.arg("-L");
        cmd.arg("--max-redirs");
        cmd.arg(http_max_redirects().to_string());
    }

    let out = run_command_output_with_timeout(cmd, "http provider models probe")
        .map_err(LlmRunError::message)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(LlmRunError::message(if stderr.is_empty() {
            format!("http models probe exited with status {}", out.status)
        } else {
            format!(
                "http models probe exited with status {}: {}",
                out.status, stderr
            )
        }));
    }
    let parsed = serde_json::from_slice::<Value>(&out.stdout).map_err(|e| {
        LlmRunError::message(format!(
            "http models probe expected JSON payload from {}: {e}",
            probe_url
        ))
    })?;
    let model_ids: Vec<String> = parsed
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .map(ToOwned::to_owned)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    Ok(json!({
        "probe_url": probe_url,
        "model_count": model_ids.len(),
        "model_ids": model_ids
    }))
}

pub fn current_provider_capabilities() -> Result<ProviderCapabilities, LlmRunError> {
    let adapter = resolve_provider_adapter()?;
    Ok(adapter.capabilities())
}

fn plain_text_to_jsonl(text: &str) -> Result<String, LlmRunError> {
    wrap_agent_text_as_jsonl(text).map_err(LlmRunError::message)
}

pub trait ProviderAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError>;
    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError>;
    fn capabilities(&self) -> ProviderCapabilities;
}

pub struct PrimaryProcessAdapter;

impl ProviderAdapter for PrimaryProcessAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_primary_plain(prompt)
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_primary_jsonl(prompt)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("primary-cli")
    }
}

pub struct OllamaCliAdapter {
    model: String,
}

pub struct LlamaCppCliAdapter {
    model: String,
    bin: String,
}

pub struct MlxPythonAdapter {
    model: String,
    python: String,
    preferred_args: Option<String>,
}

impl MlxPythonAdapter {
    fn new() -> Result<Self, LlmRunError> {
        let env_model_selector = env::var("CX_MLX_MODEL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let model_selector = env_model_selector.clone().unwrap_or_else(llm_model);
        let model = resolve_mlx_model_for_run().map_err(LlmRunError::message)?;
        let python = env::var("CX_MLX_PYTHON")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "python3".to_string());
        let preferred_args = crate::local_models::selector_preferred_args("mlx", &model_selector)
            .map_err(LlmRunError::message)?
            .or_else(|| {
                if env_model_selector.is_some() {
                    None
                } else {
                    crate::local_models::find_record_for_backend(&model, "mlx")
                        .ok()
                        .flatten()
                        .and_then(|record| record.preferred_args)
                }
            });
        Ok(Self {
            model,
            python,
            preferred_args,
        })
    }
}

impl ProviderAdapter for MlxPythonAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_mlx_plain(
            prompt,
            &self.model,
            &self.python,
            self.preferred_args.as_deref(),
        )
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        let text = self.run_plain(prompt)?;
        plain_text_to_jsonl(&text)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("mlx-python")
    }
}

impl LlamaCppCliAdapter {
    fn new() -> Result<Self, LlmRunError> {
        let model = resolve_llama_cpp_model_for_run().map_err(LlmRunError::message)?;
        let bin = env::var("CX_LLAMA_CPP_BIN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "llama-cli".to_string());
        Ok(Self { model, bin })
    }
}

impl ProviderAdapter for LlamaCppCliAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_llama_cpp_plain(prompt, &self.model, &self.bin)
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        let text = self.run_plain(prompt)?;
        plain_text_to_jsonl(&text)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("llama.cpp-cli")
    }
}

impl OllamaCliAdapter {
    fn new() -> Result<Self, LlmRunError> {
        let model = resolve_ollama_model_for_run().map_err(LlmRunError::message)?;
        Ok(Self { model })
    }
}

impl ProviderAdapter for OllamaCliAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_ollama_plain(prompt, &self.model)
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        let text = self.run_plain(prompt)?;
        plain_text_to_jsonl(&text)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("ollama-cli")
    }
}

pub struct MockAdapter {
    plain_response: String,
    jsonl_response: Option<String>,
    error_message: Option<String>,
}

impl MockAdapter {
    fn new_from_env() -> Self {
        let plain_response = env::var("CX_MOCK_PLAIN_RESPONSE")
            .unwrap_or_else(|_| "{\"commands\":[\"echo mock\"]}".to_string());
        let jsonl_response = env::var("CX_MOCK_JSONL_RESPONSE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let error_message = env::var("CX_MOCK_ERROR")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Self {
            plain_response,
            jsonl_response,
            error_message,
        }
    }
}

impl ProviderAdapter for MockAdapter {
    fn run_plain(&self, _prompt: &str) -> Result<String, LlmRunError> {
        if let Some(err) = &self.error_message {
            return Err(LlmRunError::message(err.clone()));
        }
        Ok(self.plain_response.clone())
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        if let Some(err) = &self.error_message {
            return Err(LlmRunError::message(err.clone()));
        }
        if let Some(jsonl) = &self.jsonl_response {
            return Ok(jsonl.clone());
        }
        let plain = self.run_plain(prompt)?;
        plain_text_to_jsonl(&plain)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("mock")
    }
}

pub struct HttpStubAdapter;

impl ProviderAdapter for HttpStubAdapter {
    fn run_plain(&self, _prompt: &str) -> Result<String, LlmRunError> {
        Err(LlmRunError::message(
            "http-stub adapter selected; HTTP provider transport is not implemented yet"
                .to_string(),
        ))
    }

    fn run_jsonl(&self, _prompt: &str) -> Result<String, LlmRunError> {
        self.run_plain("")
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("http-stub")
    }
}

pub struct HttpCurlAdapter {
    url: String,
    format: HttpProviderFormat,
    request_profile: HttpRequestProfile,
    model: Option<String>,
    http_options: HttpRequestOptions,
}

#[derive(Clone, Copy)]
enum HttpProviderFormat {
    Text,
    Json,
    Jsonl,
}

#[derive(Clone, Copy)]
enum HttpRequestProfile {
    PlainText,
    OpenAiJson,
}

impl HttpCurlAdapter {
    fn parse_format_from_env() -> HttpProviderFormat {
        match selected_http_provider_format() {
            "jsonl" => HttpProviderFormat::Jsonl,
            "json" => HttpProviderFormat::Json,
            _ => HttpProviderFormat::Text,
        }
    }

    fn parse_http_profile() -> HttpRequestProfile {
        match http_profile() {
            "openai_json" => HttpRequestProfile::OpenAiJson,
            _ => HttpRequestProfile::PlainText,
        }
    }

    fn http_model_env() -> Option<String> {
        env::var("CX_HTTP_PROVIDER_MODEL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| {
                let current = crate::runtime::llm_model();
                (!current.trim().is_empty()).then_some(current)
            })
    }

    fn extract_json_payload(raw: &str) -> Result<String, LlmRunError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LlmRunError::message(
                "http-curl adapter [http_json_empty] returned empty JSON payload".to_string(),
            ));
        }
        let parsed = serde_json::from_str::<serde_json::Value>(trimmed).map_err(|e| {
            LlmRunError::message(format!(
                "http-curl adapter [http_json_invalid] expected JSON payload: {e}"
            ))
        })?;

        match parsed {
            serde_json::Value::String(s) => Ok(s),
            serde_json::Value::Object(obj) => {
                if let Some(payload) = Self::extract_openai_text(&obj)? {
                    return Ok(payload);
                }
                if let Some(s) = obj.get("text").and_then(serde_json::Value::as_str) {
                    return Ok(s.to_string());
                }
                if let Some(s) = obj.get("response").and_then(serde_json::Value::as_str) {
                    return Ok(s.to_string());
                }
                if let Some(s) = obj.get("output").and_then(serde_json::Value::as_str) {
                    return Ok(s.to_string());
                }
                if let Some(arr) = obj.get("content").and_then(serde_json::Value::as_array) {
                    let mut joined = Vec::new();
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            joined.push(s.to_string());
                            continue;
                        }
                        if let Some(s) = item.get("text").and_then(serde_json::Value::as_str) {
                            joined.push(s.to_string());
                            continue;
                        }
                        return Err(LlmRunError::message(
                            "http-curl adapter [http_json_content_invalid] unsupported content item shape"
                                .to_string(),
                        ));
                    }
                    if joined.is_empty() {
                        return Err(LlmRunError::message(
                            "http-curl adapter [http_json_content_empty] content array had no usable text"
                                .to_string(),
                        ));
                    }
                    return Ok(joined.join("\n"));
                }
                Ok(serde_json::Value::Object(obj).to_string())
            }
            serde_json::Value::Array(_) => Ok(parsed.to_string()),
            serde_json::Value::Bool(_) | serde_json::Value::Number(_) | serde_json::Value::Null => {
                Err(LlmRunError::message(
                    "http-curl adapter [http_json_type_unsupported] expected string/object/array payload"
                        .to_string(),
                ))
            }
        }
    }

    fn extract_openai_text(
        obj: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Option<String>, LlmRunError> {
        let Some(choices) = obj.get("choices").and_then(serde_json::Value::as_array) else {
            return Ok(None);
        };
        let Some(message) = choices
            .first()
            .and_then(serde_json::Value::as_object)
            .and_then(|choice| choice.get("message"))
            .and_then(serde_json::Value::as_object)
        else {
            return Ok(None);
        };
        let Some(content) = message.get("content") else {
            return Ok(None);
        };
        match content {
            serde_json::Value::String(s) => Ok(Some(s.to_string())),
            serde_json::Value::Array(items) => {
                let mut joined = Vec::new();
                for item in items {
                    if let Some(text) = item
                        .as_object()
                        .and_then(|v| v.get("text"))
                        .and_then(serde_json::Value::as_str)
                    {
                        joined.push(text.to_string());
                        continue;
                    }
                    if let Some(text) = item.as_str() {
                        joined.push(text.to_string());
                        continue;
                    }
                    return Err(LlmRunError::message(
                        "http-curl adapter [http_openai_content_invalid] unsupported OpenAI content item shape".to_string(),
                    ));
                }
                if joined.is_empty() {
                    return Err(LlmRunError::message(
                        "http-curl adapter [http_openai_content_empty] OpenAI content array had no usable text".to_string(),
                    ));
                }
                Ok(Some(joined.join("\n")))
            }
            _ => Err(LlmRunError::message(
                "http-curl adapter [http_openai_content_type_unsupported] expected OpenAI message.content to be string or array".to_string(),
            )),
        }
    }

    fn run_raw(&self, prompt: &str) -> Result<String, LlmRunError> {
        match self.request_profile {
            HttpRequestProfile::PlainText => http_raw_opts(prompt, &self.url, &self.http_options),
            HttpRequestProfile::OpenAiJson => {
                let model = self
                    .model
                    .clone()
                    .unwrap_or_else(|| "xshelf-http".to_string());
                let body = json!({
                    "model": model,
                    "messages": [
                        {
                            "role": "user",
                            "content": prompt
                        }
                    ],
                    "stream": false
                })
                .to_string();
                http_body_opts(
                    &body,
                    &self.url,
                    "Content-Type: application/json",
                    &self.http_options,
                )
            }
        }
    }

    fn validate_jsonl_payload(raw: &str) -> Result<String, LlmRunError> {
        let mut saw_item = false;
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            let parsed = serde_json::from_str::<serde_json::Value>(line).map_err(|e| {
                LlmRunError::message(format!("http-curl adapter expected JSONL lines: {e}"))
            })?;
            if parsed.get("type").and_then(serde_json::Value::as_str) == Some("item.completed") {
                saw_item = true;
            }
        }
        if !saw_item {
            return Err(LlmRunError::message(
                "http-curl adapter jsonl payload missing item.completed entry".to_string(),
            ));
        }
        Ok(raw.to_string())
    }

    fn new_from_env() -> Result<Self, LlmRunError> {
        let url = env::var("CX_HTTP_PROVIDER_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                LlmRunError::message(
                    "http-curl adapter requires CX_HTTP_PROVIDER_URL to be set".to_string(),
                )
            })?;
        validate_http_url(&url)?;
        let auth = http_auth_pair()?;
        let tls_pinned_pubkey = env_nonempty("CX_HTTP_TLS_PINNEDPUBKEY");
        let tls_ca_bundle = env_nonempty("CX_HTTP_CA_BUNDLE");
        let tls_client_cert = env_nonempty("CX_HTTP_CLIENT_CERT");
        let tls_client_key = env_nonempty("CX_HTTP_CLIENT_KEY");
        let format = Self::parse_format_from_env();
        let request_profile = Self::parse_http_profile();
        let model = Self::http_model_env();
        Ok(Self {
            url,
            format,
            request_profile,
            model,
            http_options: HttpRequestOptions {
                auth_hdr: auth.as_ref().map(|(name, _)| name.clone()),
                auth_val: auth.as_ref().map(|(_, value)| value.clone()),
                tls_pinned_pubkey,
                tls_ca_bundle,
                tls_client_cert,
                tls_client_key,
                tls_min_version: Some(http_tlsver().to_string()),
                follow_redirects: http_follow_redirects(),
                max_redirects: http_max_redirects(),
            },
        })
    }
}

impl ProviderAdapter for HttpCurlAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        match self.request_profile {
            HttpRequestProfile::PlainText => http_plain_opts(prompt, &self.url, &self.http_options),
            HttpRequestProfile::OpenAiJson => {
                let raw = self.run_raw(prompt)?;
                Self::extract_json_payload(&raw)
            }
        }
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        match self.format {
            HttpProviderFormat::Text => {
                let text = self.run_plain(prompt)?;
                plain_text_to_jsonl(&text)
            }
            HttpProviderFormat::Json => {
                let raw = self.run_raw(prompt)?;
                let payload = Self::extract_json_payload(&raw)?;
                plain_text_to_jsonl(&payload)
            }
            HttpProviderFormat::Jsonl => {
                let jsonl = self.run_raw(prompt)?;
                Self::validate_jsonl_payload(&jsonl)
            }
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("http-curl")
    }
}

pub fn resolve_provider_adapter() -> Result<Box<dyn ProviderAdapter>, LlmRunError> {
    if let Some(v) = adapter_override() {
        if v == "mock" {
            return Ok(Box::new(MockAdapter::new_from_env()));
        }
        if v == "http-stub" {
            return Ok(Box::new(HttpStubAdapter));
        }
        if v == "http" || v == "http-curl" {
            return Ok(Box::new(HttpCurlAdapter::new_from_env()?));
        }
    }
    match normalized_backend_name(&llm_backend()) {
        "ollama" => return Ok(Box::new(OllamaCliAdapter::new()?)),
        "llamacpp" => return Ok(Box::new(LlamaCppCliAdapter::new()?)),
        "mlx" => return Ok(Box::new(MlxPythonAdapter::new()?)),
        _ => {}
    }
    Ok(Box::new(PrimaryProcessAdapter))
}

pub fn run_jsonl_with_current_adapter(prompt: &str) -> Result<String, LlmRunError> {
    let adapter = resolve_provider_adapter()?;
    adapter.run_jsonl(prompt)
}

#[cfg(test)]
mod tests {
    use super::{
        HttpCurlAdapter, HttpProviderFormat, HttpRequestProfile, ProviderAdapter, ProviderStatus,
        backend_tq_caps, http_profile, is_local_url, normalize_provider_status,
        normalized_backend_name, parse_http_hosts, plain_text_to_jsonl, url_host,
        validate_http_url,
    };
    use serde_json::Value;
    use std::env;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn env_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env test lock")
    }

    #[test]
    fn backend_normalization_defaults_to_codex() {
        assert_eq!(normalized_backend_name("primary"), "primary");
        assert_eq!(normalized_backend_name("CoDeX"), "primary");
        assert_eq!(normalized_backend_name("unknown"), "primary");
    }

    #[test]
    fn backend_normalization_accepts_ollama_case() {
        assert_eq!(normalized_backend_name("ollama"), "ollama");
        assert_eq!(normalized_backend_name("OLLAMA"), "ollama");
    }

    #[test]
    fn backend_normalization_accepts_llama_cpp_aliases() {
        assert_eq!(normalized_backend_name("llamacpp"), "llamacpp");
        assert_eq!(normalized_backend_name("llama.cpp"), "llamacpp");
        assert_eq!(normalized_backend_name("llama_cpp"), "llamacpp");
    }

    #[test]
    fn backend_normalization_accepts_mlx() {
        assert_eq!(normalized_backend_name("mlx"), "mlx");
        assert_eq!(normalized_backend_name("MLX"), "mlx");
    }

    #[test]
    fn plain_text_output_wrapped_as_jsonl_agent() {
        let raw = "line1\nline2 with \"quotes\"";
        let jsonl = plain_text_to_jsonl(raw).expect("wrap jsonl");
        let parsed: Value = serde_json::from_str(&jsonl).expect("parse wrapped json");
        assert_eq!(
            parsed.get("type").and_then(Value::as_str),
            Some("item.completed")
        );
        let item = parsed.get("item").expect("item");
        assert_eq!(
            item.get("type").and_then(Value::as_str),
            Some("agent_message")
        );
        assert_eq!(item.get("text").and_then(Value::as_str), Some(raw));
    }

    #[test]
    fn selected_adapter_name_follows_backend_normalization() {
        assert_eq!(normalized_backend_name("ollama"), "ollama");
        assert_eq!(normalized_backend_name("primary"), "primary");
    }

    #[test]
    fn provider_transport_mapping_covers_mock_and_process() {
        assert_eq!(super::provider_transport_for_adapter("mock"), "mock");
        assert_eq!(
            super::provider_transport_for_adapter("primary-cli"),
            "process"
        );
        assert_eq!(
            super::provider_transport_for_adapter("ollama-cli"),
            "process"
        );
    }

    #[test]
    fn capabilities_mapping_is_deterministic() {
        let primary = super::capabilities_for_adapter("primary-cli");
        assert!(primary.jsonl_native);
        assert!(primary.schema_strict);
        assert_eq!(primary.transport, "process");

        let ollama = super::capabilities_for_adapter("ollama-cli");
        assert!(!ollama.jsonl_native);
        assert!(ollama.schema_strict);
        assert_eq!(ollama.transport, "process");

        let mock = super::capabilities_for_adapter("mock");
        assert!(!mock.jsonl_native);
        assert!(mock.schema_strict);
        assert_eq!(mock.transport, "mock");

        let http = super::capabilities_for_adapter("http-stub");
        assert!(!http.jsonl_native);
        assert!(http.schema_strict);
        assert_eq!(http.transport, "http");

        let http_curl = super::capabilities_for_adapter("http-curl");
        assert!(!http_curl.jsonl_native);
        assert!(http_curl.schema_strict);
        assert_eq!(http_curl.transport, "http");
    }

    #[test]
    fn tq_caps_typed() {
        let standard = backend_tq_caps("primary");
        assert_eq!(standard.turboquant_runtime_support, "none");
        assert_eq!(standard.turboquant_backend_role, "standard_provider");
        assert_eq!(standard.turboquant_metric_kind, None);

        let mlx = backend_tq_caps("mlx");
        assert_eq!(mlx.turboquant_runtime_support, "comparative_only");
        assert_eq!(mlx.turboquant_backend_role, "comparative_backend");
        assert_eq!(mlx.turboquant_metric_kind, Some("cache_nbytes"));

        let llama = backend_tq_caps("llama.cpp");
        assert_eq!(llama.turboquant_runtime_support, "reference_only");
        assert_eq!(llama.turboquant_backend_role, "codec_reference_backend");
        assert_eq!(llama.turboquant_metric_kind, Some("raw_ratio"));
    }

    #[test]
    fn runtime_caps_typed_for_local_and_http_profile() {
        let mlx_process = super::backend_runtime_caps_for("mlx", "mlx-python", None, None);
        assert_eq!(mlx_process.model_registry, Some(true));
        assert_eq!(mlx_process.model_aliases, Some(true));
        assert_eq!(mlx_process.local_model_path, Some(true));
        assert_eq!(mlx_process.resident_server, Some(false));
        assert_eq!(mlx_process.openai_compatible, Some(false));
        assert_eq!(mlx_process.cache_metric_kind, Some("cache_nbytes"));
        assert_eq!(mlx_process.supports_persisted_kv_restore, Some(false));

        let llama_process =
            super::backend_runtime_caps_for("llamacpp", "llama.cpp-cli", None, None);
        assert_eq!(llama_process.cache_metric_kind, Some("raw_ratio"));
        assert_eq!(llama_process.model_registry, Some(true));

        let ollama_process = super::backend_runtime_caps_for("ollama", "ollama-cli", None, None);
        assert_eq!(ollama_process.model_registry, Some(true));
        assert_eq!(ollama_process.cache_metric_kind, None);

        let http_plain =
            super::backend_runtime_caps_for("primary", "http-curl", Some("plain_text"), None);
        assert_eq!(http_plain.model_registry, Some(false));
        assert_eq!(http_plain.model_aliases, Some(false));
        assert_eq!(http_plain.local_model_path, Some(false));
        assert_eq!(http_plain.resident_server, Some(false));
        assert_eq!(http_plain.openai_compatible, Some(false));

        let http_openai = super::backend_runtime_caps_for(
            "primary",
            "http-curl",
            Some("openai_json"),
            Some("http://127.0.0.1:11434/v1/chat/completions"),
        );
        assert_eq!(http_openai.openai_compatible, Some(true));
        assert_eq!(http_openai.resident_server, Some(false));
        assert_eq!(http_openai.supports_batching, None);
        assert_eq!(http_openai.supports_tool_calling, None);
        assert_eq!(http_openai.supports_embeddings, None);
        assert_eq!(http_openai.supports_reranking, None);

        let mlx_http_remote = super::backend_runtime_caps_for(
            "mlx",
            "http-curl",
            Some("openai_json"),
            Some("https://example.com/v1/chat/completions"),
        );
        assert_eq!(mlx_http_remote.openai_compatible, Some(true));
        assert_eq!(mlx_http_remote.resident_server, Some(false));

        let mlx_http_local = super::backend_runtime_caps_for(
            "mlx",
            "http-curl",
            Some("openai_json"),
            Some("http://127.0.0.1:11434/v1/chat/completions"),
        );
        assert_eq!(mlx_http_local.openai_compatible, Some(true));
        assert_eq!(mlx_http_local.resident_server, Some(true));
    }

    #[test]
    fn adapter_trait_capabilities_match_mapping() {
        let primary = super::PrimaryProcessAdapter;
        let caps = primary.capabilities();
        assert!(caps.jsonl_native);
        assert_eq!(caps.transport, "process");
    }

    #[test]
    fn adapter_override_http_stub_sets_transport_and_status() {
        assert_eq!(super::provider_transport_for_adapter("http-stub"), "http");
        assert_eq!(
            super::provider_status_for_adapter("http-stub"),
            ProviderStatus::StubUnimplemented
        );
        assert_eq!(super::provider_transport_for_adapter("http-curl"), "http");
        assert_eq!(
            super::provider_status_for_adapter("http-curl"),
            ProviderStatus::Experimental
        );
        assert_eq!(
            super::provider_status_for_adapter("primary-cli"),
            ProviderStatus::Stable
        );
    }

    #[test]
    fn normalize_provider_status_maps_unknown_to_stable() {
        assert_eq!(
            normalize_provider_status(Some("experimental")),
            ProviderStatus::Experimental
        );
        assert_eq!(
            normalize_provider_status(Some("stub_unimplemented")),
            ProviderStatus::StubUnimplemented
        );
        assert_eq!(
            normalize_provider_status(Some("totally_unknown")),
            ProviderStatus::Stable
        );
        assert_eq!(normalize_provider_status(None), ProviderStatus::Stable);
    }

    #[test]
    fn local_url_strict() {
        assert!(is_local_url("http://localhost:8080/v1"));
        assert!(is_local_url("http://127.0.0.1"));
        assert!(is_local_url("http://[::1]/health"));
        assert!(!is_local_url("http://example.com"));
        assert!(!is_local_url("https://localhost:8080"));
    }

    #[test]
    fn models_probe_url_rewrites_to_v1_models() {
        assert_eq!(
            super::models_probe_url("http://127.0.0.1:11434/v1/chat/completions")
                .expect("probe url"),
            "http://127.0.0.1:11434/v1/models"
        );
        assert_eq!(
            super::models_probe_url("https://api.example.local/anything").expect("probe url"),
            "https://api.example.local/v1/models"
        );
    }

    #[test]
    fn models_probe_url_rejects_invalid_shape() {
        let err = super::models_probe_url("api.example.local/v1").expect_err("invalid url");
        assert!(
            err.message.contains("http_url_scheme_invalid"),
            "{}",
            err.message
        );
    }

    #[test]
    fn https_block_default() {
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_ALLOWED_HOSTS");
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        let err = validate_http_url("http://example.com/v1").expect_err("expected block");
        assert!(err.message.contains("http_url_insecure"), "{}", err.message);
    }

    #[test]
    fn https_allow_local() {
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_ALLOWED_HOSTS");
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        validate_http_url("http://127.0.0.1:8080/v1").expect("local http should pass");
    }

    #[test]
    fn https_allow_override() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_REQUIRE_HTTPS", "0");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
            env::remove_var("CX_HTTP_ALLOWED_HOSTS");
        }
        validate_http_url("http://example.com/v1").expect("https override should allow");
        unsafe {
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOWED_HOSTS");
        };
    }

    #[test]
    fn url_host_shapes() {
        assert_eq!(
            url_host("https://example.com/v1").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            url_host("http://127.0.0.1:8080").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(
            url_host("https://user:pw@api.example.com/x").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(url_host("https://[::1]:9443").as_deref(), Some("::1"));
    }

    #[test]
    fn parse_hosts_norm() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var(
                "CX_HTTP_ALLOWED_HOSTS",
                "example.com, EXAMPLE.com,api.local",
            )
        };
        let parsed = parse_http_hosts().expect("hosts");
        assert_eq!(
            parsed,
            vec!["api.local".to_string(), "example.com".to_string()]
        );
        unsafe { env::remove_var("CX_HTTP_ALLOWED_HOSTS") };
    }

    #[test]
    fn allowlist_blocks_unknown() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_ALLOWED_HOSTS", "allowed.example");
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        let err = validate_http_url("https://blocked.example/v1").expect_err("must block");
        assert!(
            err.message.contains("http_host_not_allowed"),
            "{}",
            err.message
        );
        unsafe { env::remove_var("CX_HTTP_ALLOWED_HOSTS") };
    }

    #[test]
    fn allowlist_allows_configured() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_ALLOWED_HOSTS", "allowed.example");
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        validate_http_url("https://allowed.example/v1").expect("must allow configured host");
        unsafe { env::remove_var("CX_HTTP_ALLOWED_HOSTS") };
    }

    #[test]
    fn rollout_policy_ok() {
        let value = super::adapter_policy_value();
        assert_eq!(
            value.get("default_transport").and_then(Value::as_str),
            Some("process")
        );
        assert_eq!(
            value.get("http_transport_opt_in").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            value.get("default_switch_guard").and_then(Value::as_str),
            Some("two_green_ci_windows")
        );
    }

    #[test]
    fn http_profile_defaults() {
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_REQUEST_PROFILE");
        }
        assert_eq!(http_profile(), "plain_text");
    }

    #[test]
    fn http_profile_aliases() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_REQUEST_PROFILE", "openai");
        }
        assert_eq!(http_profile(), "openai_json");
        unsafe {
            env::set_var("CX_HTTP_REQUEST_PROFILE", "openai-json");
        }
        assert_eq!(http_profile(), "openai_json");
        unsafe {
            env::remove_var("CX_HTTP_REQUEST_PROFILE");
        }
    }

    #[test]
    fn tlsver_default_cov() {
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_TLS_MIN_VERSION");
        }
        assert_eq!(super::http_tlsver(), "1.2");
    }

    #[test]
    fn tlsver_alias_cov() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_TLS_MIN_VERSION", "tls1.3");
        }
        assert_eq!(super::http_tlsver(), "1.3");
        unsafe {
            env::set_var("CX_HTTP_TLS_MIN_VERSION", "default");
        }
        assert_eq!(super::http_tlsver(), "default");
        unsafe {
            env::remove_var("CX_HTTP_TLS_MIN_VERSION");
        }
    }

    #[test]
    fn http_redirect_defaults() {
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_FOLLOW_REDIRECTS");
            env::remove_var("CX_HTTP_MAX_REDIRECTS");
        }
        assert!(!super::http_follow_redirects());
        assert_eq!(super::http_max_redirects(), 0);
    }

    #[test]
    fn redirect_env_cov() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_FOLLOW_REDIRECTS", "1");
            env::set_var("CX_HTTP_MAX_REDIRECTS", "5");
        }
        assert!(super::http_follow_redirects());
        assert_eq!(super::http_max_redirects(), 5);
        unsafe {
            env::remove_var("CX_HTTP_FOLLOW_REDIRECTS");
            env::remove_var("CX_HTTP_MAX_REDIRECTS");
        }
    }

    #[test]
    fn openai_json_extracts() {
        let raw = r#"{"choices":[{"message":{"content":"openai ok"}}]}"#;
        assert_eq!(
            HttpCurlAdapter::extract_json_payload(raw).expect("payload"),
            "openai ok"
        );
    }

    #[test]
    fn openai_json_defaults() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_REQUEST_PROFILE", "openai_json");
            env::remove_var("CX_HTTP_PROVIDER_FORMAT");
        }
        let adapter = HttpCurlAdapter {
            url: "http://127.0.0.1:9999/infer".to_string(),
            format: HttpCurlAdapter::parse_format_from_env(),
            request_profile: HttpRequestProfile::OpenAiJson,
            model: Some("gpt-test".to_string()),
            http_options: Default::default(),
        };
        assert!(matches!(adapter.format, HttpProviderFormat::Json));
        unsafe {
            env::remove_var("CX_HTTP_REQUEST_PROFILE");
        }
    }
}

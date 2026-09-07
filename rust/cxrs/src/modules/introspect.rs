use std::env;

use serde_json::json;

use crate::capture::budget_config_from_env;
use crate::config::app_config;
use crate::execmeta::toolchain_version_string;
use crate::paths::{resolve_log_file, resolve_quarantine_dir, resolve_state_file};
use crate::provider_adapter::{
    current_provider_capabilities, http_auth_head, http_auth_mode, http_auth_src, http_profile,
    runtime_caps_json, selected_adapter_name, selected_http_provider_format,
    selected_provider_status_kind, selected_runtime_caps, selected_tq_caps, tls_posture_json,
    tls_posture_opt,
};
use crate::runtime::{llm_backend, llm_model, logging_enabled};
use crate::state::{read_state_value, value_at_path};

fn value_to_display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

fn state_pref(path: &str) -> String {
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, path))
        .map(value_to_display)
        .unwrap_or_else(|| "n/a".to_string())
}

fn print_version_header(
    app_name: &str,
    app_version: &str,
    cwd: &str,
    execution_path: &str,
    source: &str,
) {
    println!("name: {app_name}");
    println!("version: {}", toolchain_version_string(app_version));
    println!("cwd: {cwd}");
    println!("execution_path: {execution_path}");
    println!("source: {source}");
}

fn print_version_paths(log_file: &str, state_file: &str, quarantine_dir: &str) {
    println!("log_file: {log_file}");
    println!("state_file: {state_file}");
    println!("quarantine_dir: {quarantine_dir}");
}

fn print_version_runtime(mode: &str, backend: &str, active_model: &str, schema_relaxed: &str) {
    let adapter_name = selected_adapter_name();
    let provider_status = selected_provider_status_kind().as_str();
    let caps = current_provider_capabilities()
        .unwrap_or_else(|_| crate::provider_adapter::selected_provider_capabilities());
    println!("mode: {mode}");
    println!("llm_backend: {backend}");
    println!("provider_adapter: {adapter_name}");
    println!("provider_transport: {}", caps.transport);
    println!("provider_status: {provider_status}");
    if caps.transport == "http" {
        println!("http_provider_format: {}", selected_http_provider_format());
        println!("http_auth_mode: {}", http_auth_mode());
        println!(
            "http_auth_header: {}",
            http_auth_head().unwrap_or_else(|| "n/a".to_string())
        );
        println!(
            "http_auth_secret_source: {}",
            http_auth_src().unwrap_or("off")
        );
        let posture = tls_posture_opt().expect("http tls posture");
        let allowed_hosts = if posture.allowlist_active {
            posture.allowed_hosts.join(",")
        } else {
            "<any>".to_string()
        };
        println!("http_require_https: {}", posture.https_required);
        println!("http_allow_local_http: {}", posture.local_http_exception);
        println!("http_allowed_hosts: {allowed_hosts}");
        println!(
            "http_tls_pinning: {}",
            if posture.pinned_pubkey { "set" } else { "off" }
        );
        println!(
            "http_tls_ca_bundle: {}",
            if posture.ca_bundle { "set" } else { "off" }
        );
        println!(
            "http_tls_client_cert: {}",
            if posture.client_cert { "set" } else { "off" }
        );
        println!(
            "http_tls_client_key: {}",
            if posture.client_key { "set" } else { "off" }
        );
        println!("http_tls_min_version: {}", posture.min_tls_version);
        println!("http_follow_redirects: {}", posture.follow_redirects);
        println!("http_max_redirects: {}", posture.max_redirects);
        println!(
            "http_tls_posture: https_required={} local_http_exception={} allowlist_active={} min_tls_version={} pinned_pubkey={} ca_bundle={} client_cert={} client_key={} follow_redirects={} max_redirects={}",
            posture.https_required,
            posture.local_http_exception,
            posture.allowlist_active,
            posture.min_tls_version,
            if posture.pinned_pubkey { "set" } else { "off" },
            if posture.ca_bundle { "set" } else { "off" },
            if posture.client_cert { "set" } else { "off" },
            if posture.client_key { "set" } else { "off" },
            posture.follow_redirects,
            posture.max_redirects
        );
    }
    println!("provider_jsonl_native: {}", caps.jsonl_native);
    println!("provider_schema_strict: {}", caps.schema_strict);
    println!("llm_model: {active_model}");
    println!("backend_resolution: backend={backend} model={active_model}");
    println!("schema_relaxed: {schema_relaxed}");
}

fn print_version_capture(capture_provider: &str, native_reduce: &str, prefer_native: &str) {
    println!("capture_provider: {capture_provider}");
    println!("native_reduce: {native_reduce}");
    println!("capture_prefer_native: {prefer_native}");
    println!("capture_external_dependencies: none");
}

fn core_payload(app_version: &str) -> serde_json::Value {
    let runtime_cfg = app_config();
    let mode = runtime_cfg.cx_mode.clone();
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() {
        "<unset>".to_string()
    } else {
        model
    };
    let capture_provider = runtime_cfg.capture_provider.clone();
    let adapter_name = selected_adapter_name();
    let provider_status = selected_provider_status_kind().as_str();
    let caps = current_provider_capabilities()
        .unwrap_or_else(|_| crate::provider_adapter::selected_provider_capabilities());
    let experiment_caps = selected_tq_caps();
    let runtime_caps = selected_runtime_caps();
    let capture_prefer_native = env::var("CX_CAPTURE_PREFER_NATIVE")
        .ok()
        .map(|v| v.trim() != "0")
        .unwrap_or(true);
    let budget_cfg = budget_config_from_env();
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let bash_fallback = execution_path.contains("bash");

    let mut provider = json!({
        "adapter": adapter_name,
        "transport": caps.transport,
        "status": provider_status,
        "jsonl_native": caps.jsonl_native,
        "schema_strict": caps.schema_strict,
    });
    if caps.transport == "http" {
        let posture = tls_posture_opt().expect("http tls posture");
        let allowed_hosts = if posture.allowlist_active {
            posture.allowed_hosts.join(",")
        } else {
            "<any>".to_string()
        };
        provider["http_provider_format"] = json!(selected_http_provider_format());
        provider["http_auth_mode"] = json!(http_auth_mode());
        provider["http_auth_header"] = json!(http_auth_head());
        provider["http_auth_secret_source"] = json!(http_auth_src());
        provider["http_request_profile"] = json!(http_profile());
        provider["http_require_https"] = json!(posture.https_required);
        provider["http_allow_local_http"] = json!(posture.local_http_exception);
        provider["http_allowed_hosts"] = json!(allowed_hosts);
        provider["http_tls_pinning"] = json!(if posture.pinned_pubkey { "set" } else { "off" });
        provider["http_tls_ca_bundle"] = json!(if posture.ca_bundle { "set" } else { "off" });
        provider["http_tls_client_cert"] = json!(if posture.client_cert { "set" } else { "off" });
        provider["http_tls_client_key"] = json!(if posture.client_key { "set" } else { "off" });
        provider["http_tls_min_version"] = json!(posture.min_tls_version);
        provider["http_follow_redirects"] = json!(posture.follow_redirects);
        provider["http_max_redirects"] = json!(posture.max_redirects);
        provider["http_tls_posture"] = tls_posture_json().expect("http tls posture json");
    }

    json!({
        "contract_version": "core.v1",
        "version": toolchain_version_string(app_version),
        "execution": {
            "path": execution_path,
            "bash_fallback_used": bash_fallback,
            "mode": mode,
        },
        "backend": {
            "name": backend,
            "active_model": active_model,
        },
        "provider": provider,
        "backend_capabilities": {
            "turboquant": {
                "cx_runtime_support": experiment_caps.turboquant_runtime_support,
                "selected_backend_role": experiment_caps.turboquant_backend_role,
                "memory_metric_kind": experiment_caps.turboquant_metric_kind,
            },
            "runtime": runtime_caps_json(runtime_caps)
        },
        "capture": {
            "provider": capture_provider,
            "prefer_native": capture_prefer_native,
            "external_dependencies": "none",
        },
        "budget": {
            "chars": budget_cfg.budget_chars,
            "lines": budget_cfg.budget_lines,
            "clip_mode": budget_cfg.clip_mode,
            "clip_footer": budget_cfg.clip_footer,
        },
        "schema_enforcement": true,
        "logging_enabled": logging_enabled(),
        "log_file": log_file,
    })
}

fn print_version_preferences() {
    println!(
        "state.preferences.conventional_commits: {}",
        state_pref("preferences.conventional_commits")
    );
    println!(
        "state.preferences.pr_summary_format: {}",
        state_pref("preferences.pr_summary_format")
    );
}

fn version_payload(app_name: &str, app_version: &str) -> serde_json::Value {
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let cfg = app_config();
    let source = env::var("CX_SOURCE_LOCATION").unwrap_or_else(|_| "standalone:cxrs".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state_file = resolve_state_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let quarantine_dir = resolve_quarantine_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() {
        "<unset>".to_string()
    } else {
        model
    };
    let caps = current_provider_capabilities()
        .unwrap_or_else(|_| crate::provider_adapter::selected_provider_capabilities());
    let provider_status = selected_provider_status_kind().as_str();
    let experiment_caps = selected_tq_caps();
    let runtime_caps = selected_runtime_caps();
    let native_reduce = env::var("CX_NATIVE_REDUCE").unwrap_or_else(|_| "1".to_string());
    let prefer_native = env::var("CX_CAPTURE_PREFER_NATIVE").unwrap_or_else(|_| "1".to_string());
    let mut provider = json!({
        "adapter": selected_adapter_name(),
        "transport": caps.transport,
        "status": provider_status,
        "jsonl_native": caps.jsonl_native,
        "schema_strict": caps.schema_strict,
    });
    if caps.transport == "http" {
        let posture = tls_posture_opt().expect("http tls posture");
        let allowed_hosts = if posture.allowlist_active {
            posture.allowed_hosts.join(",")
        } else {
            "<any>".to_string()
        };
        provider["http_provider_format"] = json!(selected_http_provider_format());
        provider["http_auth_mode"] = json!(http_auth_mode());
        provider["http_auth_header"] = json!(http_auth_head());
        provider["http_auth_secret_source"] = json!(http_auth_src());
        provider["http_request_profile"] = json!(http_profile());
        provider["http_require_https"] = json!(posture.https_required);
        provider["http_allow_local_http"] = json!(posture.local_http_exception);
        provider["http_allowed_hosts"] = json!(allowed_hosts);
        provider["http_tls_pinning"] = json!(if posture.pinned_pubkey { "set" } else { "off" });
        provider["http_tls_ca_bundle"] = json!(if posture.ca_bundle { "set" } else { "off" });
        provider["http_tls_client_cert"] = json!(if posture.client_cert { "set" } else { "off" });
        provider["http_tls_client_key"] = json!(if posture.client_key { "set" } else { "off" });
        provider["http_tls_min_version"] = json!(posture.min_tls_version);
        provider["http_follow_redirects"] = json!(posture.follow_redirects);
        provider["http_max_redirects"] = json!(posture.max_redirects);
        provider["http_tls_posture"] = tls_posture_json().expect("http tls posture json");
    }

    json!({
        "contract_version": "version.v1",
        "name": app_name,
        "version": toolchain_version_string(app_version),
        "cwd": cwd,
        "execution_path": execution_path,
        "source": source,
        "paths": {
            "log_file": log_file,
            "state_file": state_file,
            "quarantine_dir": quarantine_dir,
        },
        "runtime": {
            "mode": cfg.cx_mode,
            "backend": backend,
            "active_model": active_model,
            "schema_relaxed": cfg.schema_relaxed,
        },
        "provider": provider,
        "backend_capabilities": {
            "turboquant": {
                "cx_runtime_support": experiment_caps.turboquant_runtime_support,
                "selected_backend_role": experiment_caps.turboquant_backend_role,
                "memory_metric_kind": experiment_caps.turboquant_metric_kind,
            },
            "runtime": runtime_caps_json(runtime_caps)
        },
        "capture": {
            "provider": cfg.capture_provider,
            "native_reduce": native_reduce,
            "prefer_native": prefer_native,
            "external_dependencies": "none",
        },
        "budget": {
            "chars": cfg.budget_chars,
            "lines": cfg.budget_lines,
            "clip_mode": cfg.clip_mode,
        },
        "cmd_timeout_secs": cfg.cmd_timeout_secs,
        "state_preferences": {
            "conventional_commits": state_pref("preferences.conventional_commits"),
            "pr_summary_format": state_pref("preferences.pr_summary_format"),
        },
    })
}

pub fn print_version(app_name: &str, app_version: &str, args: &[String]) {
    let json_out = args.iter().any(|a| a == "--json");
    if json_out {
        let payload = version_payload(app_name, app_version);
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
        );
        return;
    }

    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let cfg = app_config();
    let source = env::var("CX_SOURCE_LOCATION").unwrap_or_else(|_| "standalone:cxrs".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state_file = resolve_state_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let quarantine_dir = resolve_quarantine_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };

    print_version_header(app_name, app_version, &cwd, &execution_path, &source);
    print_version_paths(&log_file, &state_file, &quarantine_dir);
    print_version_runtime(
        &cfg.cx_mode,
        &backend,
        active_model,
        if cfg.schema_relaxed { "1" } else { "0" },
    );

    let native_reduce = env::var("CX_NATIVE_REDUCE").unwrap_or_else(|_| "1".to_string());
    let prefer_native = env::var("CX_CAPTURE_PREFER_NATIVE").unwrap_or_else(|_| "1".to_string());
    print_version_capture(&cfg.capture_provider, &native_reduce, &prefer_native);

    let experiment_caps = selected_tq_caps();
    let runtime_caps = selected_runtime_caps();
    println!(
        "backend_capability.turboquant_runtime_support: {}",
        experiment_caps.turboquant_runtime_support
    );
    println!(
        "backend_capability.turboquant_backend_role: {}",
        experiment_caps.turboquant_backend_role
    );
    println!(
        "backend_capability.turboquant_metric_kind: {}",
        experiment_caps.turboquant_metric_kind.unwrap_or("n/a")
    );
    println!(
        "backend_capability.runtime.model_registry: {}",
        runtime_caps
            .model_registry
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.model_aliases: {}",
        runtime_caps
            .model_aliases
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.local_model_path: {}",
        runtime_caps
            .local_model_path
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.resident_server: {}",
        runtime_caps
            .resident_server
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.openai_compatible: {}",
        runtime_caps
            .openai_compatible
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.supports_persisted_kv_restore: {}",
        runtime_caps
            .supports_persisted_kv_restore
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.cache_metric_kind: {}",
        runtime_caps.cache_metric_kind.unwrap_or("n/a")
    );
    println!("budget_chars: {}", cfg.budget_chars);
    println!("budget_lines: {}", cfg.budget_lines);
    println!("cmd_timeout_secs: {}", cfg.cmd_timeout_secs);
    println!("clip_mode: {}", cfg.clip_mode);
    print_version_preferences();
}

pub fn cmd_core(app_version: &str, args: &[String]) -> i32 {
    let json_out = args.iter().any(|a| a == "--json");
    let payload = core_payload(app_version);
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
        );
        return 0;
    }

    let runtime_cfg = app_config();
    let mode = runtime_cfg.cx_mode.clone();
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    let capture_provider = runtime_cfg.capture_provider.clone();
    let adapter_name = selected_adapter_name();
    let provider_status = selected_provider_status_kind().as_str();
    let caps = current_provider_capabilities()
        .unwrap_or_else(|_| crate::provider_adapter::selected_provider_capabilities());
    let experiment_caps = selected_tq_caps();
    let runtime_caps = selected_runtime_caps();
    let capture_prefer_native = env::var("CX_CAPTURE_PREFER_NATIVE")
        .ok()
        .map(|v| v.trim() != "0")
        .unwrap_or(true);
    let budget_cfg = budget_config_from_env();
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let bash_fallback = execution_path.contains("bash");

    println!("== cxcore ==");
    println!("version: {}", toolchain_version_string(app_version));
    println!("execution_path: {execution_path}");
    println!("bash_fallback_used: {bash_fallback}");
    println!("backend: {backend}");
    println!("provider_adapter: {adapter_name}");
    println!("provider_transport: {}", caps.transport);
    println!("provider_status: {provider_status}");
    if caps.transport == "http" {
        println!("http_provider_format: {}", selected_http_provider_format());
        println!("http_auth_mode: {}", http_auth_mode());
        println!(
            "http_auth_header: {}",
            http_auth_head().unwrap_or_else(|| "n/a".to_string())
        );
        println!(
            "http_auth_secret_source: {}",
            http_auth_src().unwrap_or("off")
        );
        println!("http_request_profile: {}", http_profile());
        let posture = tls_posture_opt().expect("http tls posture");
        let allowed_hosts = if posture.allowlist_active {
            posture.allowed_hosts.join(",")
        } else {
            "<any>".to_string()
        };
        println!("http_require_https: {}", posture.https_required);
        println!("http_allow_local_http: {}", posture.local_http_exception);
        println!("http_allowed_hosts: {allowed_hosts}");
        println!(
            "http_tls_pinning: {}",
            if posture.pinned_pubkey { "set" } else { "off" }
        );
        println!(
            "http_tls_ca_bundle: {}",
            if posture.ca_bundle { "set" } else { "off" }
        );
        println!(
            "http_tls_client_cert: {}",
            if posture.client_cert { "set" } else { "off" }
        );
        println!(
            "http_tls_client_key: {}",
            if posture.client_key { "set" } else { "off" }
        );
        println!("http_tls_min_version: {}", posture.min_tls_version);
        println!("http_follow_redirects: {}", posture.follow_redirects);
        println!("http_max_redirects: {}", posture.max_redirects);
        println!(
            "http_tls_posture: https_required={} local_http_exception={} allowlist_active={} min_tls_version={} pinned_pubkey={} ca_bundle={} client_cert={} client_key={} follow_redirects={} max_redirects={}",
            posture.https_required,
            posture.local_http_exception,
            posture.allowlist_active,
            posture.min_tls_version,
            if posture.pinned_pubkey { "set" } else { "off" },
            if posture.ca_bundle { "set" } else { "off" },
            if posture.client_cert { "set" } else { "off" },
            if posture.client_key { "set" } else { "off" },
            posture.follow_redirects,
            posture.max_redirects
        );
    }
    println!("provider_jsonl_native: {}", caps.jsonl_native);
    println!("provider_schema_strict: {}", caps.schema_strict);
    println!(
        "backend_capability.turboquant_runtime_support: {}",
        experiment_caps.turboquant_runtime_support
    );
    println!(
        "backend_capability.turboquant_backend_role: {}",
        experiment_caps.turboquant_backend_role
    );
    println!(
        "backend_capability.turboquant_metric_kind: {}",
        experiment_caps.turboquant_metric_kind.unwrap_or("n/a")
    );
    println!(
        "backend_capability.runtime.model_registry: {}",
        runtime_caps
            .model_registry
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.model_aliases: {}",
        runtime_caps
            .model_aliases
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.local_model_path: {}",
        runtime_caps
            .local_model_path
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.resident_server: {}",
        runtime_caps
            .resident_server
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.openai_compatible: {}",
        runtime_caps
            .openai_compatible
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.supports_persisted_kv_restore: {}",
        runtime_caps
            .supports_persisted_kv_restore
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "backend_capability.runtime.cache_metric_kind: {}",
        runtime_caps.cache_metric_kind.unwrap_or("n/a")
    );
    println!("active_model: {active_model}");
    println!("execution_mode: {mode}");
    println!("capture_provider: {capture_provider}");
    println!("capture_prefer_native: {capture_prefer_native}");
    println!("capture_external_dependencies: none");
    println!("budget_chars: {}", budget_cfg.budget_chars);
    println!("budget_lines: {}", budget_cfg.budget_lines);
    println!("cmd_timeout_secs: {}", runtime_cfg.cmd_timeout_secs);
    println!("clip_mode: {}", budget_cfg.clip_mode);
    println!("clip_footer: {}", budget_cfg.clip_footer);
    println!("schema_enforcement: true");
    println!("logging_enabled: {}", logging_enabled());
    println!("log_file: {log_file}");
    0
}

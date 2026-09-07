use serde_json::{Value, json};
use std::env;
use std::process::Command;

use crate::process::{TimeoutInfo, run_command_with_stdin_output_with_timeout_meta};
use crate::types::UsageStats;

#[derive(Clone, Debug, Default)]
pub struct HttpRequestOptions {
    pub auth_hdr: Option<String>,
    pub auth_val: Option<String>,
    pub tls_pinned_pubkey: Option<String>,
    pub tls_ca_bundle: Option<String>,
    pub tls_client_cert: Option<String>,
    pub tls_client_key: Option<String>,
    pub tls_min_version: Option<String>,
    pub follow_redirects: bool,
    pub max_redirects: u32,
}

#[derive(Clone, Debug)]
pub struct LlmRunError {
    pub message: String,
    pub timeout: Option<TimeoutInfo>,
}

impl LlmRunError {
    fn from_process(err: crate::process::ProcessError) -> Self {
        let timeout = err.timeout_info().cloned();
        Self {
            message: err.to_string(),
            timeout,
        }
    }

    pub(crate) fn message(message: String) -> Self {
        Self {
            message,
            timeout: None,
        }
    }
}

impl std::fmt::Display for LlmRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub fn usage_from_jsonl(jsonl: &str) -> UsageStats {
    let mut out = UsageStats::default();
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("turn.completed") {
            continue;
        }
        let usage = v.get("usage").cloned().unwrap_or(Value::Null);
        out.input_tokens = usage.get("input_tokens").and_then(Value::as_u64);
        out.cached_input_tokens = usage.get("cached_input_tokens").and_then(Value::as_u64);
        out.output_tokens = usage.get("output_tokens").and_then(Value::as_u64);
    }
    out
}

pub fn effective_input_tokens(input: Option<u64>, cached: Option<u64>) -> Option<u64> {
    match (input, cached) {
        (Some(i), Some(c)) => Some(i.saturating_sub(c)),
        (Some(i), None) => Some(i),
        _ => None,
    }
}

pub fn extract_agent_text(jsonl: &str) -> Option<String> {
    let mut last: Option<String> = None;
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let is_item_completed = v.get("type").and_then(Value::as_str) == Some("item.completed");
        if !is_item_completed {
            continue;
        }
        let item = v.get("item")?;
        if item.get("type").and_then(Value::as_str) != Some("agent_message") {
            continue;
        }
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            last = Some(text.to_string());
        }
    }
    last
}

pub fn run_primary_jsonl(prompt: &str) -> Result<String, LlmRunError> {
    let mut cmd = Command::new(concat!("co", "dex"));
    cmd.args(["exec", "--json", "-"]);
    let out = run_command_with_stdin_output_with_timeout_meta(cmd, prompt, "primary exec --json -")
        .map_err(LlmRunError::from_process)?;

    if !out.status.success() {
        return Err(LlmRunError::message(format!(
            "primary backend exited with status {}",
            out.status
        )));
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn run_primary_plain(prompt: &str) -> Result<String, LlmRunError> {
    let mut cmd = Command::new(concat!("co", "dex"));
    cmd.args(["exec", "-"]);
    let out = run_command_with_stdin_output_with_timeout_meta(cmd, prompt, "primary exec -")
        .map_err(LlmRunError::from_process)?;
    if !out.status.success() {
        return Err(LlmRunError::message(format!(
            "primary backend exited with status {}",
            out.status
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn run_ollama_plain(prompt: &str, model: &str) -> Result<String, LlmRunError> {
    let mut cmd = Command::new("ollama");
    cmd.args(["run", model]);
    let out = run_command_with_stdin_output_with_timeout_meta(cmd, prompt, "ollama run")
        .map_err(LlmRunError::from_process)?;
    if !out.status.success() {
        return Err(LlmRunError::message(format!(
            "ollama exited with status {}",
            out.status
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn llama_cpp_uses_hf_repo(model: &str) -> bool {
    let model = model.trim();
    if model.is_empty()
        || model.starts_with('/')
        || model.starts_with('.')
        || model.starts_with('~')
        || model.contains(".gguf")
    {
        return false;
    }
    model.contains('/')
}

pub fn run_llama_cpp_plain(prompt: &str, model: &str, bin: &str) -> Result<String, LlmRunError> {
    let mut cmd = Command::new(bin);
    let model_flag = if llama_cpp_uses_hf_repo(model) {
        "-hf"
    } else {
        "-m"
    };
    cmd.args([model_flag, model, "-p", prompt, "--no-display-prompt"]);
    if let Ok(raw_args) = env::var("CX_LLAMA_CPP_ARGS") {
        let extra = shell_words::split(&raw_args).map_err(|e| {
            LlmRunError::message(format!(
                "llama.cpp adapter could not parse CX_LLAMA_CPP_ARGS: {e}"
            ))
        })?;
        cmd.args(extra);
    }
    let out = run_command_with_stdin_output_with_timeout_meta(cmd, "", "llama.cpp llama-cli")
        .map_err(LlmRunError::from_process)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(LlmRunError::message(if stderr.is_empty() {
            format!("llama.cpp exited with status {}", out.status)
        } else {
            format!("llama.cpp exited with status {}: {}", out.status, stderr)
        }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn run_mlx_plain(
    prompt: &str,
    model: &str,
    python: &str,
    preferred_args: Option<&str>,
) -> Result<String, LlmRunError> {
    let mut cmd = Command::new(python);
    cmd.args([
        "-m", "mlx_lm", "generate", "--model", model, "--prompt", prompt,
    ]);
    if let Ok(max_tokens) = env::var("CX_MLX_MAX_TOKENS")
        && !max_tokens.trim().is_empty()
    {
        cmd.args(["--max-tokens", max_tokens.trim()]);
    }
    if let Some(raw_args) = preferred_args
        && !raw_args.trim().is_empty()
    {
        let extra = shell_words::split(raw_args).map_err(|e| {
            LlmRunError::message(format!(
                "MLX adapter could not parse registry preferred_args: {e}"
            ))
        })?;
        cmd.args(extra);
    }
    if let Ok(raw_args) = env::var("CX_MLX_ARGS") {
        let extra = shell_words::split(&raw_args).map_err(|e| {
            LlmRunError::message(format!("MLX adapter could not parse CX_MLX_ARGS: {e}"))
        })?;
        cmd.args(extra);
    }
    let out = run_command_with_stdin_output_with_timeout_meta(cmd, "", "MLX mlx_lm generate")
        .map_err(LlmRunError::from_process)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(LlmRunError::message(if stderr.is_empty() {
            format!("MLX exited with status {}", out.status)
        } else {
            format!("MLX exited with status {}: {}", out.status, stderr)
        }));
    }
    let raw = String::from_utf8_lossy(&out.stdout).to_string();
    if env_bool("CX_MLX_RAW_OUTPUT", false) {
        Ok(raw)
    } else {
        Ok(normalize_mlx_output(&raw))
    }
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

fn normalize_mlx_output(raw: &str) -> String {
    let mut saw_generation = false;
    let mut collecting = false;
    let mut out: Vec<String> = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("==========") {
            if collecting {
                break;
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Generation:") {
            saw_generation = true;
            collecting = true;
            let rest = rest.trim();
            if !rest.is_empty() {
                out.push(rest.to_string());
            }
            continue;
        }
        if trimmed.starts_with("Prompt:")
            || trimmed.starts_with("Prompt tokens:")
            || trimmed.starts_with("Generation tokens:")
            || trimmed.starts_with("Peak memory:")
        {
            if collecting {
                break;
            }
            continue;
        }
        if collecting {
            out.push(line.to_string());
        }
    }
    if saw_generation {
        return out.join("\n").trim().to_string();
    }
    raw.trim().to_string()
}

fn run_http_body(
    body: &str,
    url: &str,
    content_type: &str,
    options: &HttpRequestOptions,
) -> Result<String, LlmRunError> {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "-f",
        "-X",
        "POST",
        url,
        "-H",
        content_type,
        "--data-binary",
        "@-",
    ]);
    if let Some((name, value)) = options
        .auth_hdr
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .zip(
            options
                .auth_val
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty()),
        )
    {
        cmd.args(["-H", &format!("{name}: {value}")]);
    }
    if let Some(pinned) = options
        .tls_pinned_pubkey
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        cmd.args(["--pinnedpubkey", pinned]);
    }
    if let Some(ca_bundle) = options
        .tls_ca_bundle
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        cmd.args(["--cacert", ca_bundle]);
    }
    if let Some(client_cert) = options
        .tls_client_cert
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        cmd.args(["--cert", client_cert]);
    }
    if let Some(client_key) = options
        .tls_client_key
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        cmd.args(["--key", client_key]);
    }
    match options
        .tls_min_version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some("1.3") => {
            cmd.arg("--tlsv1.3");
        }
        Some("1.2") => {
            cmd.arg("--tlsv1.2");
        }
        _ => {}
    }
    if options.follow_redirects {
        cmd.arg("-L");
        cmd.arg("--max-redirs");
        cmd.arg(options.max_redirects.to_string());
    }
    let out = run_command_with_stdin_output_with_timeout_meta(cmd, body, "http provider curl")
        .map_err(LlmRunError::from_process)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let kind = classify_http_curl_error(&stderr);
        return Err(LlmRunError::message(if stderr.is_empty() {
            format!("http provider [{kind}] exited with status {}", out.status)
        } else {
            format!(
                "http provider [{kind}] exited with status {}: {}",
                out.status, stderr
            )
        }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn http_raw_opts(
    prompt: &str,
    url: &str,
    options: &HttpRequestOptions,
) -> Result<String, LlmRunError> {
    run_http_body(
        prompt,
        url,
        "Content-Type: text/plain; charset=utf-8",
        options,
    )
}

pub fn http_body_opts(
    body: &str,
    url: &str,
    content_type: &str,
    options: &HttpRequestOptions,
) -> Result<String, LlmRunError> {
    run_http_body(body, url, content_type, options)
}

pub fn http_plain_opts(
    prompt: &str,
    url: &str,
    options: &HttpRequestOptions,
) -> Result<String, LlmRunError> {
    let body = http_raw_opts(prompt, url, options)?;
    Ok(parse_http_provider_body(&body))
}

fn classify_http_curl_error(stderr: &str) -> &'static str {
    let s = stderr.to_ascii_lowercase();
    if s.contains("could not resolve host")
        || s.contains("failed to connect")
        || s.contains("connection refused")
        || s.contains("connection timed out")
    {
        return "transport_unreachable";
    }
    if s.contains("requested url returned error") || s.contains("http/") {
        return "http_status";
    }
    if s.trim().is_empty() {
        return "transport_error";
    }
    "provider_error"
}

fn parse_http_provider_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
        return body.to_string();
    };
    if let Some(s) = v.get("text").and_then(Value::as_str) {
        return s.to_string();
    }
    if let Some(s) = v.get("response").and_then(Value::as_str) {
        return s.to_string();
    }
    if let Some(s) = v.get("output").and_then(Value::as_str) {
        return s.to_string();
    }
    if let Some(arr) = v.get("content").and_then(Value::as_array) {
        let mut joined = Vec::new();
        for item in arr {
            if let Some(s) = item.as_str() {
                joined.push(s.to_string());
                continue;
            }
            if let Some(s) = item.get("text").and_then(Value::as_str) {
                joined.push(s.to_string());
            }
        }
        if !joined.is_empty() {
            return joined.join("\n");
        }
    }
    body.to_string()
}

pub fn wrap_agent_text_as_jsonl(text: &str) -> Result<String, String> {
    let wrapped = json!({
      "type":"item.completed",
      "item":{"type":"agent_message","text":text}
    });
    serde_json::to_string(&wrapped)
        .map_err(|e| format!("failed to serialize ollama JSONL wrapper: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{classify_http_curl_error, normalize_mlx_output, parse_http_provider_body};

    #[test]
    fn http_body_parser_prefers_text_field() {
        let parsed = parse_http_provider_body("{\"text\":\"hello\"}");
        assert_eq!(parsed, "hello");
    }

    #[test]
    fn http_body_parser_supports_content_array_objects() {
        let parsed =
            parse_http_provider_body("{\"content\":[{\"text\":\"line1\"},{\"text\":\"line2\"}]}");
        assert_eq!(parsed, "line1\nline2");
    }

    #[test]
    fn http_body_parser_falls_back_to_raw_body() {
        let raw = "plain response";
        let parsed = parse_http_provider_body(raw);
        assert_eq!(parsed, raw);
    }

    #[test]
    fn http_body_parser_unknown_envelope_falls_back() {
        let raw = r#"{"unexpected":"shape"}"#;
        let parsed = parse_http_provider_body(raw);
        assert_eq!(parsed, raw);
    }

    #[test]
    fn http_error_classifier_categorizes_curl_patterns() {
        assert_eq!(
            classify_http_curl_error("curl: (7) Failed to connect to 127.0.0.1"),
            "transport_unreachable"
        );
        assert_eq!(
            classify_http_curl_error("curl: (22) The requested URL returned error: 503"),
            "http_status"
        );
        assert_eq!(classify_http_curl_error(""), "transport_error");
    }

    #[test]
    fn mlx_output_normalizer_extracts_generation_block() {
        let raw = "==========\nPrompt: hi\nGeneration: OK\n==========\nPrompt tokens: 2\n";
        assert_eq!(normalize_mlx_output(raw), "OK");
    }

    #[test]
    fn mlx_output_normalizer_preserves_plain_output() {
        assert_eq!(normalize_mlx_output("plain answer\n"), "plain answer");
    }
}

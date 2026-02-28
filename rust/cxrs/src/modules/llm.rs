use serde_json::{Value, json};
use std::process::Command;

use crate::process::{TimeoutInfo, run_command_with_stdin_output_with_timeout_meta};
use crate::types::UsageStats;

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

pub fn run_codex_jsonl(prompt: &str) -> Result<String, LlmRunError> {
    let mut cmd = Command::new("codex");
    cmd.args(["exec", "--json", "-"]);
    let out = run_command_with_stdin_output_with_timeout_meta(cmd, prompt, "codex exec --json -")
        .map_err(LlmRunError::from_process)?;

    if !out.status.success() {
        return Err(LlmRunError::message(format!(
            "codex exited with status {}",
            out.status
        )));
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn run_codex_plain(prompt: &str) -> Result<String, LlmRunError> {
    let mut cmd = Command::new("codex");
    cmd.args(["exec", "-"]);
    let out = run_command_with_stdin_output_with_timeout_meta(cmd, prompt, "codex exec -")
        .map_err(LlmRunError::from_process)?;
    if !out.status.success() {
        return Err(LlmRunError::message(format!(
            "codex exited with status {}",
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

pub fn run_http_plain(prompt: &str, url: &str, token: Option<&str>) -> Result<String, LlmRunError> {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "-f",
        "-X",
        "POST",
        url,
        "-H",
        "Content-Type: text/plain; charset=utf-8",
        "--data-binary",
        "@-",
    ]);
    if let Some(t) = token.filter(|v| !v.trim().is_empty()) {
        cmd.args(["-H", &format!("Authorization: Bearer {t}")]);
    }
    let out = run_command_with_stdin_output_with_timeout_meta(cmd, prompt, "http provider curl")
        .map_err(LlmRunError::from_process)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(LlmRunError::message(if stderr.is_empty() {
            format!("http provider exited with status {}", out.status)
        } else {
            format!(
                "http provider exited with status {}: {}",
                out.status, stderr
            )
        }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn wrap_agent_text_as_jsonl(text: &str) -> Result<String, String> {
    let wrapped = json!({
      "type":"item.completed",
      "item":{"type":"agent_message","text":text}
    });
    serde_json::to_string(&wrapped)
        .map_err(|e| format!("failed to serialize ollama JSONL wrapper: {e}"))
}

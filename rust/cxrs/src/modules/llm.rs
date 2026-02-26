use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::types::UsageStats;

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

pub fn run_codex_jsonl(prompt: &str) -> Result<String, String> {
    let mut child = Command::new("codex")
        .args(["exec", "--json", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to start codex: {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| format!("failed writing prompt to codex stdin: {e}"))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for codex: {e}"))?;

    if !out.status.success() {
        return Err(format!("codex exited with status {}", out.status));
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn run_codex_plain(prompt: &str) -> Result<String, String> {
    let mut child = Command::new("codex")
        .args(["exec", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to start codex: {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| format!("failed writing prompt to codex stdin: {e}"))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for codex: {e}"))?;
    if !out.status.success() {
        return Err(format!("codex exited with status {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn run_ollama_plain(prompt: &str, model: &str) -> Result<String, String> {
    let mut child = Command::new("ollama")
        .args(["run", model])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to start ollama: {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| format!("failed writing prompt to ollama stdin: {e}"))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for ollama: {e}"))?;
    if !out.status.success() {
        return Err(format!("ollama exited with status {}", out.status));
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

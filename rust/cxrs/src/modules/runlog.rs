use serde_json::json;
use std::env;

use crate::config::app_config;
use crate::execmeta::{is_schema_tool, make_execution_id, prompt_preview, utc_now_iso};
use crate::llm::effective_input_tokens;
use crate::logs::{append_jsonl, validate_execution_log_row};
use crate::paths::{repo_root, resolve_log_file, resolve_schema_fail_log_file};
use crate::quarantine::quarantine_store_with_attempts;
use crate::runtime::{llm_backend, llm_model};
use crate::schema::schema_name_for_tool;
use crate::state::{current_task_id, current_task_parent_id};
use crate::types::{CaptureStats, ExecutionLog, QuarantineAttempt, UsageStats};
use crate::util::sha256_hex;

pub struct RunLogInput<'a> {
    pub tool: &'a str,
    pub prompt: &'a str,
    pub duration_ms: u64,
    pub usage: Option<&'a UsageStats>,
    pub capture: Option<&'a CaptureStats>,
    pub schema_ok: bool,
    pub schema_reason: Option<&'a str>,
    pub schema_name: Option<&'a str>,
    pub quarantine_id: Option<&'a str>,
    pub policy_blocked: Option<bool>,
    pub policy_reason: Option<&'a str>,
}

pub fn log_codex_run(input: RunLogInput<'_>) -> Result<(), String> {
    let run_log = resolve_log_file().ok_or_else(|| "unable to resolve run log file".to_string())?;
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let root = repo_root()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let scope = if root.is_empty() { "global" } else { "repo" };

    let input_tokens = input.usage.and_then(|u| u.input_tokens);
    let cached = input.usage.and_then(|u| u.cached_input_tokens);
    let output = input.usage.and_then(|u| u.output_tokens);
    let effective = effective_input_tokens(input_tokens, cached);
    let cap = input.capture.cloned().unwrap_or_default();
    let backend = llm_backend();
    let model = llm_model();
    let mode = app_config().cx_mode.clone();
    let exec_id = make_execution_id(input.tool);
    let schema_enforced = is_schema_tool(input.tool);
    let task_id = current_task_id().unwrap_or_default();
    let task_parent_id = current_task_parent_id().unwrap_or_default();

    let ts = utc_now_iso();
    let row = ExecutionLog {
        execution_id: exec_id,
        timestamp: ts.clone(),
        ts,
        command: input.tool.to_string(),
        tool: input.tool.to_string(),
        cwd,
        scope: scope.to_string(),
        repo_root: root,
        backend_used: backend.clone(),
        llm_backend: backend,
        llm_model: if model.is_empty() { None } else { Some(model) },
        capture_provider: cap.capture_provider.clone(),
        execution_mode: mode,
        duration_ms: Some(input.duration_ms),
        schema_enforced,
        schema_name: input.schema_name.map(|s| s.to_string()),
        schema_valid: input.schema_ok,
        schema_ok: input.schema_ok,
        schema_reason: input.schema_reason.map(|s| s.to_string()),
        quarantine_id: input.quarantine_id.map(|s| s.to_string()),
        task_id: if task_id.is_empty() {
            None
        } else {
            Some(task_id)
        },
        task_parent_id: if task_parent_id.is_empty() {
            None
        } else {
            Some(task_parent_id)
        },
        input_tokens,
        cached_input_tokens: cached,
        effective_input_tokens: effective,
        output_tokens: output,
        system_output_len_raw: cap.system_output_len_raw,
        system_output_len_processed: cap.system_output_len_processed,
        system_output_len_clipped: cap.system_output_len_clipped,
        system_output_lines_raw: cap.system_output_lines_raw,
        system_output_lines_processed: cap.system_output_lines_processed,
        system_output_lines_clipped: cap.system_output_lines_clipped,
        clipped: cap.clipped,
        budget_chars: cap.budget_chars,
        budget_lines: cap.budget_lines,
        clip_mode: cap.clip_mode,
        clip_footer: cap.clip_footer,
        rtk_used: cap.rtk_used,
        prompt_sha256: Some(sha256_hex(input.prompt)),
        prompt_preview: Some(prompt_preview(input.prompt, 180)),
        policy_blocked: input.policy_blocked,
        policy_reason: input.policy_reason.map(|s| s.to_string()),
    };
    validate_execution_log_row(&row)?;
    let value = serde_json::to_value(row).map_err(|e| format!("failed serialize run log: {e}"))?;
    append_jsonl(&run_log, &value)
}

pub fn log_schema_failure(
    tool: &str,
    reason: &str,
    raw: &str,
    schema: &str,
    prompt: &str,
    attempts: Vec<QuarantineAttempt>,
) -> Result<String, String> {
    let qid = quarantine_store_with_attempts(tool, reason, raw, schema, prompt, attempts)?;

    let schema_fail_log = resolve_schema_fail_log_file()
        .ok_or_else(|| "unable to resolve schema_failures log file".to_string())?;
    let failure_row = json!({
        "ts": utc_now_iso(),
        "tool": tool,
        "reason": reason,
        "quarantine_id": qid,
        "raw_sha256": sha256_hex(raw)
    });
    append_jsonl(&schema_fail_log, &failure_row)?;

    let run_log = resolve_log_file().ok_or_else(|| "unable to resolve run log file".to_string())?;
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let root = repo_root()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let scope = if root.is_empty() { "global" } else { "repo" };

    let ts = utc_now_iso();
    let task_id = current_task_id();
    let task_parent_id = current_task_parent_id();
    let backend = llm_backend();
    let run_failure = ExecutionLog {
        execution_id: make_execution_id(tool),
        timestamp: ts.clone(),
        ts,
        command: tool.to_string(),
        tool: tool.to_string(),
        cwd,
        scope: scope.to_string(),
        repo_root: root,
        backend_used: backend.clone(),
        llm_backend: backend,
        llm_model: {
            let m = llm_model();
            if m.is_empty() { None } else { Some(m) }
        },
        capture_provider: None,
        execution_mode: app_config().cx_mode.clone(),
        duration_ms: None,
        schema_enforced: true,
        schema_name: schema_name_for_tool(tool).map(|s| s.to_string()),
        schema_valid: false,
        schema_ok: false,
        schema_reason: Some(reason.to_string()),
        quarantine_id: Some(qid.clone()),
        task_id,
        task_parent_id,
        input_tokens: None,
        cached_input_tokens: None,
        effective_input_tokens: None,
        output_tokens: None,
        system_output_len_raw: None,
        system_output_len_processed: None,
        system_output_len_clipped: None,
        system_output_lines_raw: None,
        system_output_lines_processed: None,
        system_output_lines_clipped: None,
        clipped: None,
        budget_chars: None,
        budget_lines: None,
        clip_mode: None,
        clip_footer: None,
        rtk_used: None,
        prompt_sha256: None,
        prompt_preview: None,
        policy_blocked: None,
        policy_reason: None,
    };
    validate_execution_log_row(&run_failure)
        .map_err(|e| format!("schema failure log invalid: {e}"))?;
    let failure_value =
        serde_json::to_value(run_failure).map_err(|e| format!("failed serialize run log: {e}"))?;
    append_jsonl(&run_log, &failure_value)?;

    Ok(qid)
}

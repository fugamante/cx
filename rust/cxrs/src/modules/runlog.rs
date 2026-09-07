use serde_json::json;
use std::env;

use crate::config::app_config;
use crate::execmeta::{is_schema_tool, make_execution_id, prompt_preview, utc_now_iso};
use crate::llm::effective_input_tokens;
use crate::logs::{append_jsonl, validate_execution_log_row};
use crate::paths::{repo_root_hint, resolve_log_file, resolve_schema_fail_log_file};
use crate::provider_adapter::{
    http_profile_opt, selected_adapter_name, selected_http_parser_mode_opt,
    selected_http_provider_format_opt, selected_provider_status, selected_provider_transport,
};
use crate::quarantine::quarantine_store_with_attempts;
use crate::runtime::{llm_backend, llm_model};
use crate::schema::schema_name_for_tool;
use crate::state::{current_task_id, current_task_parent_id};
use crate::types::{CaptureStats, ExecutionLog, QuarantineAttempt, UsageStats};
use crate::util::sha256_hex;

pub struct RunLogInput<'a> {
    pub tool: &'a str,
    pub prompt: &'a str,
    pub prompt_raw: Option<&'a str>,
    pub prompt_filtered: Option<&'a str>,
    pub schema_prompt: Option<&'a str>,
    pub schema_raw: Option<&'a str>,
    pub schema_attempt: Option<u64>,
    pub timed_out: Option<bool>,
    pub timeout_secs: Option<u64>,
    pub system_status: Option<i32>,
    pub command_label: Option<&'a str>,
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

pub struct TaskRunAllSummaryLogInput<'a> {
    pub mode: &'a str,
    pub halt_on_critical: bool,
    pub scheduled: u64,
    pub complete: u64,
    pub failed: u64,
    pub blocked: u64,
    pub retryable_failures: u64,
    pub non_retryable_failures: u64,
    pub critical_errors: u64,
    pub halted_remaining: u64,
    pub backend_fallback_rows: u64,
    pub backend_fallbacks: Option<String>,
    pub wave_pressure_kind: Option<&'a str>,
    pub wave_pressure_suggested_mode: Option<&'a str>,
    pub latest_wave_index: Option<u64>,
    pub max_queue_wave_index: Option<u64>,
    pub max_queue_wave_ms: Option<u64>,
    pub worker_count: Option<u64>,
    pub workers: Option<&'a str>,
    pub max_retry_attempt: Option<u32>,
    pub first_queue_started_at: Option<&'a str>,
    pub first_task_started_at: Option<&'a str>,
    pub last_task_finished_at: Option<&'a str>,
    pub invocation_command: Option<&'a str>,
    pub failure_pattern: Option<&'a str>,
    pub recommended_resume_point: Option<&'a str>,
    pub duration_ms: u64,
}

fn cwd_scope_root() -> (String, String, String) {
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let root = repo_root_hint()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let scope = if root.is_empty() { "global" } else { "repo" }.to_string();
    (cwd, root, scope)
}

fn current_task_fields() -> (Option<String>, Option<String>) {
    let task_id = current_task_id().unwrap_or_default();
    let task_parent_id = current_task_parent_id().unwrap_or_default();
    (
        if task_id.is_empty() {
            None
        } else {
            Some(task_id)
        },
        if task_parent_id.is_empty() {
            None
        } else {
            Some(task_parent_id)
        },
    )
}

fn base_execution_log(
    tool: &str,
    ts: String,
    cwd: String,
    scope: String,
    root: String,
) -> ExecutionLog {
    let backend = llm_backend();
    let model = llm_model();
    let adapter_type = selected_adapter_name().to_string();
    let execution_lane = env::var("CX_EXECUTION_LANE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| Some("host".to_string()));
    let execution_lane_detail = env::var("CX_EXECUTION_LANE_DETAIL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let model_opt = if model.is_empty() {
        None
    } else {
        Some(model.clone())
    };
    let backend_selected = backend.clone();
    let broker_policy = app_config().broker_policy.clone();
    let route_reason = if backend == "ollama" {
        if model.is_empty() {
            "ollama_selected_model_unset".to_string()
        } else {
            "ollama_selected".to_string()
        }
    } else {
        "primary_selected".to_string()
    };
    let replica_index = env::var("CX_TASK_REPLICA_INDEX")
        .ok()
        .and_then(|v| v.parse::<u32>().ok());
    let replica_count = env::var("CX_TASK_REPLICA_COUNT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok());
    let converge_mode = env::var("CX_TASK_CONVERGE_MODE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let worker_id = env::var("CX_TASK_WORKER_ID")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let converge_winner = env::var("CX_TASK_CONVERGE_WINNER")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let converge_votes = env::var("CX_TASK_CONVERGE_VOTES")
        .ok()
        .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok());
    let queue_ms = env::var("CX_TASK_QUEUE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    let wave_index = env::var("CX_TASK_WAVE_INDEX")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    let wave_mode = env::var("CX_TASK_WAVE_MODE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let wave_size = env::var("CX_TASK_WAVE_SIZE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    let queue_started_at = env::var("CX_TASK_QUEUE_STARTED_AT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let task_started_at = env::var("CX_TASK_STARTED_AT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let task_finished_at = env::var("CX_TASK_FINISHED_AT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let retry_attempt = env::var("CX_TASK_RETRY_ATTEMPT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok());
    let retry_max = env::var("CX_TASK_RETRY_MAX")
        .ok()
        .and_then(|v| v.parse::<u32>().ok());
    let retry_reason = env::var("CX_TASK_RETRY_REASON")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let retry_backoff_ms = env::var("CX_TASK_RETRY_BACKOFF_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    let (task_id, task_parent_id) = current_task_fields();
    let mut row = ExecutionLog {
        execution_id: make_execution_id(tool),
        timestamp: ts.clone(),
        ts,
        command: tool.to_string(),
        tool: tool.to_string(),
        cwd,
        scope,
        repo_root: root,
        backend_used: backend.clone(),
        llm_backend: backend,
        llm_model: model_opt.clone(),
        adapter_type: Some(adapter_type),
        provider_transport: Some(selected_provider_transport().to_string()),
        provider_status: selected_provider_status().map(str::to_string),
        execution_lane,
        execution_lane_detail,
        http_request_profile: http_profile_opt().map(str::to_string),
        http_provider_format: selected_http_provider_format_opt().map(str::to_string),
        http_parser_mode: selected_http_parser_mode_opt().map(str::to_string),
        backend_selected: Some(backend_selected),
        model_selected: model_opt,
        route_policy: Some(broker_policy),
        route_reason: Some(route_reason),
        worker_id,
        replica_index,
        replica_count,
        converge_mode,
        converge_winner,
        converge_votes,
        queue_ms,
        wave_index,
        wave_mode,
        wave_size,
        queue_started_at,
        task_started_at,
        task_finished_at,
        retry_attempt,
        retry_max,
        retry_reason,
        retry_backoff_ms,
        task_id,
        task_parent_id,
        ..Default::default()
    };
    row.execution_mode = app_config().cx_mode.clone();
    row.schema_valid = true;
    row.schema_ok = true;
    row
}

fn base_run_row(tool: &str, cwd: String, scope: String, root: String) -> ExecutionLog {
    let ts = utc_now_iso();
    let mut row = base_execution_log(tool, ts, cwd, scope, root);
    row.execution_mode = app_config().cx_mode.clone();
    row.schema_enforced = is_schema_tool(tool);
    row.schema_valid = true;
    row.schema_ok = true;
    row
}

fn finalize_and_append_run(run_log: &std::path::Path, row: ExecutionLog) -> Result<(), String> {
    validate_execution_log_row(&row)?;
    let value = serde_json::to_value(row).map_err(|e| format!("failed serialize run log: {e}"))?;
    append_jsonl(run_log, &value)
}

pub fn log_primary_run(input: RunLogInput<'_>) -> Result<(), String> {
    let run_log = resolve_log_file().ok_or_else(|| "unable to resolve run log file".to_string())?;
    let (cwd, root, scope) = cwd_scope_root();

    let input_tokens = input.usage.and_then(|u| u.input_tokens);
    let cached = input.usage.and_then(|u| u.cached_input_tokens);
    let output = input.usage.and_then(|u| u.output_tokens);
    let effective = effective_input_tokens(input_tokens, cached);
    let cap = input.capture.cloned().unwrap_or_default();

    let mut row = base_run_row(input.tool, cwd, scope, root);
    let raw_prompt = input.prompt_raw.unwrap_or(input.prompt);
    let filtered_prompt = input.prompt_filtered.unwrap_or(input.prompt);
    row.duration_ms = Some(input.duration_ms);
    row.schema_name = input.schema_name.map(|s| s.to_string());
    row.schema_valid = input.schema_ok;
    row.schema_ok = input.schema_ok;
    row.schema_reason = input.schema_reason.map(|s| s.to_string());
    row.quarantine_id = input.quarantine_id.map(|s| s.to_string());
    row.capture_provider = cap.capture_provider.clone();
    row.input_tokens = input_tokens;
    row.cached_input_tokens = cached;
    row.effective_input_tokens = effective;
    row.output_tokens = output;
    row.system_output_len_raw = cap.system_output_len_raw;
    row.system_output_len_processed = cap.system_output_len_processed;
    row.system_output_len_clipped = cap.system_output_len_clipped;
    row.system_output_lines_raw = cap.system_output_lines_raw;
    row.system_output_lines_processed = cap.system_output_lines_processed;
    row.system_output_lines_clipped = cap.system_output_lines_clipped;
    row.clipped = cap.clipped;
    row.budget_chars = cap.budget_chars;
    row.budget_lines = cap.budget_lines;
    row.clip_mode = cap.clip_mode;
    row.clip_footer = cap.clip_footer;
    row.rtk_used = cap.rtk_used;
    row.prompt_sha256 = Some(sha256_hex(filtered_prompt));
    row.prompt_sha256_raw = Some(sha256_hex(raw_prompt));
    row.prompt_sha256_filtered = Some(sha256_hex(filtered_prompt));
    row.prompt_len_raw = Some(raw_prompt.chars().count() as u64);
    row.prompt_len_filtered = Some(filtered_prompt.chars().count() as u64);
    row.prompt_filter_applied = Some(raw_prompt != filtered_prompt);
    row.capture_prompt_profile = cap.capture_prompt_profile.clone();
    row.capture_prompt_profile_applied = cap.capture_prompt_profile_applied;
    row.capture_prompt_reducer_kind = cap.capture_prompt_reducer_kind.clone();
    row.capture_prompt_fallback_reason = cap.capture_prompt_fallback_reason.clone();
    row.schema_prompt_sha256 = input.schema_prompt.map(sha256_hex);
    row.schema_sha256 = input.schema_raw.map(sha256_hex);
    row.schema_attempt = input.schema_attempt;
    row.timed_out = input.timed_out;
    row.timeout_secs = input.timeout_secs;
    row.system_status = input.system_status;
    row.command_label = input.command_label.map(|s| s.to_string());
    row.prompt_preview = Some(prompt_preview(filtered_prompt, 180));
    row.policy_blocked = input.policy_blocked;
    row.policy_reason = input.policy_reason.map(|s| s.to_string());
    if row.task_id.is_some() && row.task_finished_at.is_none() {
        row.task_finished_at = Some(utc_now_iso());
    }

    finalize_and_append_run(&run_log, row)
}

pub fn log_task_run_all_summary(input: TaskRunAllSummaryLogInput<'_>) -> Result<(), String> {
    let run_log = resolve_log_file().ok_or_else(|| "unable to resolve run log file".to_string())?;
    let (cwd, root, scope) = cwd_scope_root();
    let mut row = base_run_row("cxtask_runall", cwd, scope, root);
    row.duration_ms = Some(input.duration_ms);
    row.command_label = Some("task_run_all_summary".to_string());
    row.prompt_sha256 = Some(sha256_hex("task run-all summary"));
    row.prompt_preview = Some(format!("task run-all mode={}", input.mode));
    row.run_all_mode = Some(input.mode.to_string());
    row.halt_on_critical = Some(input.halt_on_critical);
    row.run_all_scheduled = Some(input.scheduled);
    row.run_all_complete = Some(input.complete);
    row.run_all_failed = Some(input.failed);
    row.run_all_blocked = Some(input.blocked);
    row.run_all_retryable_failures = Some(input.retryable_failures);
    row.run_all_non_retryable_failures = Some(input.non_retryable_failures);
    row.run_all_critical_errors = Some(input.critical_errors);
    row.run_all_halted_remaining = Some(input.halted_remaining);
    row.run_all_backend_fallback_rows = Some(input.backend_fallback_rows);
    row.run_all_backend_fallbacks = input.backend_fallbacks.map(|s| s.to_string());
    row.run_all_wave_pressure_kind = input.wave_pressure_kind.map(|s| s.to_string());
    row.run_all_wave_pressure_suggested_mode =
        input.wave_pressure_suggested_mode.map(|s| s.to_string());
    row.run_all_latest_wave_index = input.latest_wave_index;
    row.run_all_max_queue_wave_index = input.max_queue_wave_index;
    row.run_all_max_queue_wave_ms = input.max_queue_wave_ms;
    row.run_all_worker_count = input.worker_count;
    row.run_all_workers = input.workers.map(|s| s.to_string());
    row.run_all_max_retry_attempt = input.max_retry_attempt;
    row.run_all_first_queue_started_at = input.first_queue_started_at.map(|s| s.to_string());
    row.run_all_first_task_started_at = input.first_task_started_at.map(|s| s.to_string());
    row.run_all_last_task_finished_at = input.last_task_finished_at.map(|s| s.to_string());
    row.run_all_invocation_command = input.invocation_command.map(|s| s.to_string());
    row.run_all_failure_pattern = input.failure_pattern.map(|s| s.to_string());
    row.run_all_recommended_resume_point = input.recommended_resume_point.map(|s| s.to_string());
    finalize_and_append_run(&run_log, row)
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
    let (cwd, root, scope) = cwd_scope_root();
    let mut row = base_run_row(tool, cwd, scope, root);
    row.schema_enforced = true;
    row.schema_name = schema_name_for_tool(tool).map(|s| s.to_string());
    row.schema_valid = false;
    row.schema_ok = false;
    row.schema_reason = Some(reason.to_string());
    row.quarantine_id = Some(qid.clone());
    row.schema_sha256 = Some(sha256_hex(schema));
    row.schema_prompt_sha256 = Some(sha256_hex(prompt));

    finalize_and_append_run(&run_log, row)?;
    Ok(qid)
}

use std::time::Instant;

use crate::llm::LlmRunError;
use crate::runlog::{RunLogInput, log_codex_run};
use crate::types::{CaptureStats, TaskSpec, UsageStats};

pub(crate) struct LogExecutionErrorInput<'a> {
    pub spec: &'a TaskSpec,
    pub prompt: &'a str,
    pub capture_stats: &'a CaptureStats,
    pub usage: &'a UsageStats,
    pub schema_name: Option<&'a str>,
    pub schema_prompt: Option<&'a str>,
    pub schema_raw: Option<&'a str>,
    pub schema_attempt: Option<u64>,
    pub err: &'a LlmRunError,
    pub started: &'a Instant,
}

pub(crate) fn log_execution_error(input: LogExecutionErrorInput<'_>) {
    let LogExecutionErrorInput {
        spec,
        prompt,
        capture_stats,
        usage,
        schema_name,
        schema_prompt,
        schema_raw,
        schema_attempt,
        err,
        started,
    } = input;
    if !spec.logging_enabled {
        return;
    }
    let timed_out = err.timeout.is_some();
    let timeout_secs = err.timeout.as_ref().map(|v| v.timeout_secs);
    let command_label = err.timeout.as_ref().map(|v| v.label.as_str());
    let _ = log_codex_run(RunLogInput {
        tool: &spec.command_name,
        prompt,
        schema_prompt,
        schema_raw,
        schema_attempt,
        timed_out: Some(timed_out),
        timeout_secs,
        command_label,
        duration_ms: started.elapsed().as_millis() as u64,
        usage: Some(usage),
        capture: Some(capture_stats),
        schema_ok: false,
        schema_reason: Some(err.message.as_str()),
        schema_name,
        quarantine_id: None,
        policy_blocked: None,
        policy_reason: None,
    });
}

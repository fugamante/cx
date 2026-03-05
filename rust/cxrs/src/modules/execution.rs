use serde_json::Value;
use std::time::Instant;

use crate::config::app_config;
use crate::execmeta::make_execution_id;
use crate::execution_logging::{LogExecutionErrorInput, log_execution_error};
use crate::llm::{LlmRunError, extract_agent_text, usage_from_jsonl};
use crate::prompt_filter::process_prompt;
use crate::provider_adapter::{resolve_provider_adapter, run_jsonl_with_current_adapter};
use crate::runlog::log_schema_failure;
use crate::schema::{build_schema_prompt_envelope, validate_schema_instance};
use crate::types::{
    CaptureStats, ExecutionResult, LlmOutputKind, QuarantineAttempt, TaskInput, TaskSpec,
    UsageStats,
};
use crate::util::sha256_hex;

pub fn run_llm_jsonl(prompt: &str) -> Result<String, String> {
    run_jsonl_with_current_adapter(prompt).map_err(|e| e.message)
}

pub fn execute_task(spec: TaskSpec) -> Result<ExecutionResult, String> {
    let started = Instant::now();
    let execution_id = make_execution_id(&spec.command_name);

    let (prompt, capture_stats, system_status) = match &spec.input {
        TaskInput::Prompt(p) => (p.clone(), CaptureStats::default(), None),
        TaskInput::SystemCommand(cmd) => {
            let (captured, status, stats) = crate::capture::run_system_command_capture(cmd)?;
            (captured, stats, Some(status))
        }
    };
    let capture_stats = spec
        .capture_override
        .as_ref()
        .cloned()
        .unwrap_or(capture_stats);
    let prompt_raw = prompt.clone();
    let prompt_tx = process_prompt(&prompt_raw, spec.output_kind == LlmOutputKind::SchemaJson);
    let prompt = prompt_tx.filtered.clone();

    let mut schema_valid: Option<bool> = None;
    let mut quarantine_id: Option<String> = None;
    let mut schema_prompt_for_log: Option<String> = None;
    let mut schema_raw_for_log: Option<String> = None;
    let mut schema_attempt_for_log: Option<u64> = None;
    let mut usage = UsageStats::default();
    let stdout: String;
    let stderr = String::new();
    let adapter = match resolve_provider_adapter() {
        Ok(v) => v,
        Err(e) => {
            log_execution_error(LogExecutionErrorInput {
                spec: &spec,
                prompt: &prompt,
                prompt_raw: &prompt_raw,
                prompt_filtered: &prompt,
                capture_stats: &capture_stats,
                usage: &usage,
                schema_name: None,
                schema_prompt: None,
                schema_raw: None,
                schema_attempt: None,
                err: &e,
                started: &started,
            });
            return Err(e.message);
        }
    };

    match spec.output_kind {
        LlmOutputKind::Plain => {
            stdout = match adapter.run_plain(&prompt) {
                Ok(v) => v,
                Err(e) => {
                    log_execution_error(LogExecutionErrorInput {
                        spec: &spec,
                        prompt: &prompt,
                        prompt_raw: &prompt_raw,
                        prompt_filtered: &prompt,
                        capture_stats: &capture_stats,
                        usage: &usage,
                        schema_name: None,
                        schema_prompt: None,
                        schema_raw: None,
                        schema_attempt: None,
                        err: &e,
                        started: &started,
                    });
                    return Err(e.message);
                }
            };
        }
        LlmOutputKind::Jsonl => {
            let jsonl = match adapter.run_jsonl(&prompt) {
                Ok(v) => v,
                Err(e) => {
                    log_execution_error(LogExecutionErrorInput {
                        spec: &spec,
                        prompt: &prompt,
                        prompt_raw: &prompt_raw,
                        prompt_filtered: &prompt,
                        capture_stats: &capture_stats,
                        usage: &usage,
                        schema_name: None,
                        schema_prompt: None,
                        schema_raw: None,
                        schema_attempt: None,
                        err: &e,
                        started: &started,
                    });
                    return Err(e.message);
                }
            };
            usage = usage_from_jsonl(&jsonl);
            stdout = jsonl;
        }
        LlmOutputKind::AgentText => {
            let jsonl = match adapter.run_jsonl(&prompt) {
                Ok(v) => v,
                Err(e) => {
                    log_execution_error(LogExecutionErrorInput {
                        spec: &spec,
                        prompt: &prompt,
                        prompt_raw: &prompt_raw,
                        prompt_filtered: &prompt,
                        capture_stats: &capture_stats,
                        usage: &usage,
                        schema_name: None,
                        schema_prompt: None,
                        schema_raw: None,
                        schema_attempt: None,
                        err: &e,
                        started: &started,
                    });
                    return Err(e.message);
                }
            };
            usage = usage_from_jsonl(&jsonl);
            stdout = extract_agent_text(&jsonl).unwrap_or_default();
        }
        LlmOutputKind::SchemaJson => {
            let schema = spec
                .schema
                .as_ref()
                .ok_or_else(|| "schema execution missing schema".to_string())?;
            let task_input = spec
                .schema_task_input
                .as_deref()
                .unwrap_or(&prompt)
                .to_string();
            let schema_pretty = serde_json::to_string_pretty(&schema.value)
                .unwrap_or_else(|_| schema.value.to_string());
            let retry_allowed = !app_config().schema_relaxed;
            let mut attempts: Vec<QuarantineAttempt> = Vec::new();
            let mut final_reason: Option<String> = None;
            let mut prompt_envelope =
                build_schema_prompt_envelope(&schema_pretty, &task_input, None);
            schema_raw_for_log = Some(schema_pretty.clone());
            schema_attempt_for_log = Some(1);

            let run_attempt =
                |full_prompt: &str| -> Result<(String, UsageStats, String), LlmRunError> {
                    let prompt_tx = process_prompt(full_prompt, true);
                    let jsonl = adapter.run_jsonl(&prompt_tx.filtered)?;
                    let usage = usage_from_jsonl(&jsonl);
                    let raw = extract_agent_text(&jsonl).unwrap_or_default();
                    Ok((raw, usage, prompt_tx.filtered))
                };

            let validate_raw = |raw: &str| -> Result<Value, String> {
                if raw.trim().is_empty() {
                    return Err("empty_agent_message".to_string());
                }
                validate_schema_instance(schema, raw)
            };

            let (first_raw, first_usage, first_prompt_filtered) =
                match run_attempt(&prompt_envelope.full_prompt) {
                    Ok(v) => v,
                    Err(e) => {
                        log_execution_error(LogExecutionErrorInput {
                            spec: &spec,
                            prompt: &task_input,
                            prompt_raw: &prompt_envelope.full_prompt,
                            prompt_filtered: &prompt_envelope.full_prompt,
                            capture_stats: &capture_stats,
                            usage: &usage,
                            schema_name: Some(schema.name.as_str()),
                            schema_prompt: Some(prompt_envelope.full_prompt.as_str()),
                            schema_raw: Some(schema_pretty.as_str()),
                            schema_attempt: Some(1),
                            err: &e,
                            started: &started,
                        });
                        return Err(e.message);
                    }
                };
            let mut last_schema_prompt_raw = prompt_envelope.full_prompt.clone();
            let mut last_schema_prompt_filtered = first_prompt_filtered.clone();
            schema_prompt_for_log = Some(first_prompt_filtered.clone());
            usage = first_usage;

            match validate_raw(&first_raw) {
                Ok(valid) => {
                    schema_valid = Some(true);
                    stdout = valid.to_string();
                }
                Err(reason_first) => {
                    attempts.push(QuarantineAttempt {
                        reason: reason_first.clone(),
                        prompt: prompt_envelope.full_prompt.clone(),
                        prompt_sha256: prompt_envelope.prompt_sha256.clone(),
                        raw_response: first_raw.clone(),
                        raw_sha256: sha256_hex(&first_raw),
                    });

                    if retry_allowed {
                        prompt_envelope = build_schema_prompt_envelope(
                            &schema_pretty,
                            &task_input,
                            Some(&reason_first),
                        );
                        schema_attempt_for_log = Some(2);
                        let (retry_raw, retry_usage, retry_prompt_filtered) =
                            match run_attempt(&prompt_envelope.full_prompt) {
                                Ok(v) => v,
                                Err(e) => {
                                    log_execution_error(LogExecutionErrorInput {
                                        spec: &spec,
                                        prompt: &task_input,
                                        prompt_raw: &prompt_envelope.full_prompt,
                                        prompt_filtered: &prompt_envelope.full_prompt,
                                        capture_stats: &capture_stats,
                                        usage: &usage,
                                        schema_name: Some(schema.name.as_str()),
                                        schema_prompt: Some(prompt_envelope.full_prompt.as_str()),
                                        schema_raw: Some(schema_pretty.as_str()),
                                        schema_attempt: Some(2),
                                        err: &e,
                                        started: &started,
                                    });
                                    return Err(e.message);
                                }
                            };
                        last_schema_prompt_raw = prompt_envelope.full_prompt.clone();
                        last_schema_prompt_filtered = retry_prompt_filtered.clone();
                        schema_prompt_for_log = Some(retry_prompt_filtered.clone());
                        usage = retry_usage;
                        match validate_raw(&retry_raw) {
                            Ok(valid) => {
                                schema_valid = Some(true);
                                stdout = valid.to_string();
                            }
                            Err(reason_retry) => {
                                attempts.push(QuarantineAttempt {
                                    reason: reason_retry.clone(),
                                    prompt: prompt_envelope.full_prompt.clone(),
                                    prompt_sha256: prompt_envelope.prompt_sha256.clone(),
                                    raw_response: retry_raw.clone(),
                                    raw_sha256: sha256_hex(&retry_raw),
                                });
                                final_reason = Some(reason_retry.clone());
                                schema_valid = Some(false);
                                let qid = log_schema_failure(
                                    &spec.command_name,
                                    &reason_retry,
                                    &retry_raw,
                                    &schema_pretty,
                                    &task_input,
                                    attempts,
                                )?;
                                quarantine_id = Some(qid);
                                stdout = retry_raw;
                            }
                        }
                    } else {
                        final_reason = Some(reason_first.clone());
                        schema_valid = Some(false);
                        let qid = log_schema_failure(
                            &spec.command_name,
                            &reason_first,
                            &first_raw,
                            &schema_pretty,
                            &task_input,
                            attempts,
                        )?;
                        quarantine_id = Some(qid);
                        stdout = first_raw;
                    }

                    if spec.logging_enabled {
                        let _ = crate::runlog::log_codex_run(crate::runlog::RunLogInput {
                            tool: &spec.command_name,
                            prompt: &last_schema_prompt_filtered,
                            prompt_raw: Some(&last_schema_prompt_raw),
                            prompt_filtered: Some(&last_schema_prompt_filtered),
                            schema_prompt: schema_prompt_for_log.as_deref(),
                            schema_raw: schema_raw_for_log.as_deref(),
                            schema_attempt: schema_attempt_for_log,
                            timed_out: None,
                            timeout_secs: None,
                            command_label: None,
                            duration_ms: started.elapsed().as_millis() as u64,
                            usage: Some(&usage),
                            capture: Some(&capture_stats),
                            schema_ok: schema_valid == Some(true),
                            schema_reason: final_reason.as_deref(),
                            schema_name: Some(schema.name.as_str()),
                            quarantine_id: quarantine_id.as_deref(),
                            policy_blocked: None,
                            policy_reason: None,
                        });
                    }
                    return Ok(ExecutionResult {
                        stdout,
                        stderr,
                        duration_ms: started.elapsed().as_millis() as u64,
                        schema_valid,
                        quarantine_id,
                        capture_stats,
                        execution_id,
                        usage,
                        system_status,
                    });
                }
            }
        }
    }

    if spec.logging_enabled {
        let _ = crate::runlog::log_codex_run(crate::runlog::RunLogInput {
            tool: &spec.command_name,
            prompt: &prompt,
            prompt_raw: Some(&prompt_raw),
            prompt_filtered: Some(&prompt),
            schema_prompt: schema_prompt_for_log.as_deref(),
            schema_raw: schema_raw_for_log.as_deref(),
            schema_attempt: schema_attempt_for_log,
            timed_out: None,
            timeout_secs: None,
            command_label: None,
            duration_ms: started.elapsed().as_millis() as u64,
            usage: Some(&usage),
            capture: Some(&capture_stats),
            schema_ok: schema_valid != Some(false),
            schema_reason: None,
            schema_name: spec.schema.as_ref().map(|s| s.name.as_str()),
            quarantine_id: quarantine_id.as_deref(),
            policy_blocked: None,
            policy_reason: None,
        });
    }

    Ok(ExecutionResult {
        stdout,
        stderr,
        duration_ms: started.elapsed().as_millis() as u64,
        schema_valid,
        quarantine_id,
        capture_stats,
        execution_id,
        usage,
        system_status,
    })
}

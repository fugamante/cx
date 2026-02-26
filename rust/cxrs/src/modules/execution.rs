use serde_json::Value;
use std::env;
use std::time::Instant;

use crate::execmeta::make_execution_id;
use crate::llm::{
    extract_agent_text, run_codex_jsonl, run_codex_plain, run_ollama_plain, usage_from_jsonl,
    wrap_agent_text_as_jsonl,
};
use crate::runlog::log_schema_failure;
use crate::runtime::{llm_backend, resolve_ollama_model_for_run};
use crate::schema::{build_strict_schema_prompt, validate_schema_instance};
use crate::types::{
    CaptureStats, ExecutionResult, LlmOutputKind, QuarantineAttempt, TaskInput, TaskSpec,
    UsageStats,
};
use crate::util::sha256_hex;

pub fn run_llm_plain(prompt: &str) -> Result<String, String> {
    if llm_backend() == "ollama" {
        run_ollama_plain(prompt, &resolve_ollama_model_for_run()?)
    } else {
        run_codex_plain(prompt)
    }
}

pub fn run_llm_jsonl(prompt: &str) -> Result<String, String> {
    if llm_backend() != "ollama" {
        return run_codex_jsonl(prompt);
    }
    let text = run_ollama_plain(prompt, &resolve_ollama_model_for_run()?)?;
    wrap_agent_text_as_jsonl(&text)
}

pub fn execute_task(spec: TaskSpec) -> Result<ExecutionResult, String> {
    let started = Instant::now();
    let execution_id = make_execution_id(&spec.command_name);

    let (prompt, capture_stats, system_status) = match spec.input {
        TaskInput::Prompt(p) => (p, CaptureStats::default(), None),
        TaskInput::SystemCommand(cmd) => {
            let (captured, status, stats) = crate::capture::run_system_command_capture(&cmd)?;
            (captured, stats, Some(status))
        }
    };
    let capture_stats = spec.capture_override.unwrap_or(capture_stats);

    let mut schema_valid: Option<bool> = None;
    let mut quarantine_id: Option<String> = None;
    let mut usage = UsageStats::default();
    let stdout: String;
    let stderr = String::new();

    match spec.output_kind {
        LlmOutputKind::Plain => {
            stdout = run_llm_plain(&prompt)?;
        }
        LlmOutputKind::Jsonl => {
            let jsonl = run_llm_jsonl(&prompt)?;
            usage = usage_from_jsonl(&jsonl);
            stdout = jsonl;
        }
        LlmOutputKind::AgentText => {
            let jsonl = run_llm_jsonl(&prompt)?;
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
            let retry_allowed = env::var("CX_SCHEMA_RELAXED").ok().as_deref() != Some("1");
            let mut attempts: Vec<QuarantineAttempt> = Vec::new();
            let mut final_reason: Option<String> = None;
            let mut last_full_prompt = build_strict_schema_prompt(&schema_pretty, &task_input);

            let run_attempt = |full_prompt: &str| -> Result<(String, UsageStats), String> {
                let jsonl = run_llm_jsonl(full_prompt)?;
                let usage = usage_from_jsonl(&jsonl);
                let raw = extract_agent_text(&jsonl).unwrap_or_default();
                Ok((raw, usage))
            };

            let validate_raw = |raw: &str| -> Result<Value, String> {
                if raw.trim().is_empty() {
                    return Err("empty_agent_message".to_string());
                }
                validate_schema_instance(schema, raw)
            };

            let (first_raw, first_usage) = run_attempt(&last_full_prompt)?;
            usage = first_usage;

            match validate_raw(&first_raw) {
                Ok(valid) => {
                    schema_valid = Some(true);
                    stdout = valid.to_string();
                }
                Err(reason_first) => {
                    attempts.push(QuarantineAttempt {
                        reason: reason_first.clone(),
                        prompt: last_full_prompt.clone(),
                        prompt_sha256: sha256_hex(&last_full_prompt),
                        raw_response: first_raw.clone(),
                        raw_sha256: sha256_hex(&first_raw),
                    });

                    if retry_allowed {
                        last_full_prompt = format!(
                            "{}\n\nThe previous response failed validation with reason: {}.\nReturn STRICT JSON only and satisfy the schema exactly.",
                            build_strict_schema_prompt(&schema_pretty, &task_input),
                            reason_first
                        );
                        let (retry_raw, retry_usage) = run_attempt(&last_full_prompt)?;
                        usage = retry_usage;
                        match validate_raw(&retry_raw) {
                            Ok(valid) => {
                                schema_valid = Some(true);
                                stdout = valid.to_string();
                            }
                            Err(reason_retry) => {
                                attempts.push(QuarantineAttempt {
                                    reason: reason_retry.clone(),
                                    prompt: last_full_prompt.clone(),
                                    prompt_sha256: sha256_hex(&last_full_prompt),
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
                            prompt: &task_input,
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

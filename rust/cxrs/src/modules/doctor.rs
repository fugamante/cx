use serde_json::Value;
use std::env;
use std::path::Path;
use std::process::Command;

use crate::config::{cli_app_name, command_matches_cli, command_with_cli};
use crate::llm::extract_agent_text;
use crate::logs::load_values;
use crate::paths::resolve_log_file;
use crate::process::run_command_output_with_timeout;
use crate::provider_adapter::adapter_policy_value;
use crate::runtime::{llm_backend, llm_bin_name};
use crate::task_cmds::task_readiness_value;
use crate::tasks::read_tasks;

type JsonlRunner = fn(&str) -> Result<String, String>;
type CxoRunner = fn(&[String]) -> i32;
const WAVE_QUEUE_PRESSURE_MS: u64 = 2000;

fn bin_in_path(bin: &str) -> bool {
    let path = match env::var_os("PATH") {
        Some(v) => v,
        None => return false,
    };
    env::split_paths(&path).any(|dir| {
        let candidate = dir.join(bin);
        Path::new(&candidate).is_file()
    })
}

fn check_required_bins(backend: &str, llm_bin: &str) -> usize {
    let required = ["git", "jq"];
    let mut missing_required = 0usize;
    for bin in required {
        if bin_in_path(bin) {
            println!("OK: {bin}");
        } else {
            println!("MISSING: {bin}");
            missing_required += 1;
        }
    }
    let llm_display = if backend == "primary" {
        "primary"
    } else {
        llm_bin
    };
    if bin_in_path(llm_bin) {
        println!("OK: {llm_display} (selected backend: {backend})");
    } else {
        println!("MISSING: {llm_display} (selected backend: {backend})");
        missing_required += 1;
    }
    if backend != "primary" {
        if bin_in_path(concat!("co", "dex")) {
            println!("OK: primary (recommended primary backend)");
        } else {
            println!("WARN: primary not found (recommended primary backend)");
        }
    }
    missing_required
}

fn probe_json_pipeline(backend: &str, run_llm_jsonl: JsonlRunner) -> Result<(), i32> {
    println!();
    println!("== llm json pipeline ({backend}) ==");
    let probe = run_llm_jsonl("ping").map_err(|e| {
        crate::cx_eprintln!("FAIL: {backend} json pipeline failed: {e}");
        1
    })?;
    let mut agent_count = 0u64;
    let mut reasoning_count = 0u64;
    for line in probe.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("item.completed") {
            continue;
        }
        let t = v
            .get("item")
            .and_then(|i| i.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if t == "agent_message" {
            agent_count += 1;
        } else if t == "reasoning" {
            reasoning_count += 1;
        }
    }
    println!("agent_message events: {agent_count}");
    println!("reasoning events:     {reasoning_count}");
    if agent_count < 1 {
        crate::cx_eprintln!("FAIL: expected >=1 agent_message event");
        return Err(1);
    }
    Ok(())
}

fn probe_text_pipeline(backend: &str, run_llm_jsonl: JsonlRunner) -> Result<(), i32> {
    println!();
    println!("== _codex_text equivalent ==");
    let probe2 = run_llm_jsonl("2+2? (just the number)").map_err(|e| {
        crate::cx_eprintln!("FAIL: {backend} text probe failed: {e}");
        1
    })?;
    let txt = extract_agent_text(&probe2).unwrap_or_default();
    println!("output: {txt}");
    if txt.trim() != "4" {
        println!("WARN: expected '4', got '{}'", txt.trim());
    }
    Ok(())
}

fn print_git_context() {
    println!();
    println!("== git context (optional) ==");
    let mut repo_cmd = Command::new("git");
    repo_cmd.args(["rev-parse", "--is-inside-work-tree"]);
    match run_command_output_with_timeout(repo_cmd, "git rev-parse --is-inside-work-tree") {
        Ok(out) if out.status.success() => {
            println!("in git repo: yes");
            let mut branch_cmd = Command::new("git");
            branch_cmd.args(["rev-parse", "--abbrev-ref", "HEAD"]);
            if let Ok(branch_out) =
                run_command_output_with_timeout(branch_cmd, "git rev-parse --abbrev-ref HEAD")
            {
                let branch = String::from_utf8_lossy(&branch_out.stdout)
                    .trim()
                    .to_string();
                if !branch.is_empty() {
                    println!("branch: {branch}");
                }
            }
        }
        _ => println!("in git repo: no (skip git-based checks)"),
    }
}

fn readiness_summary_lines(task_readiness: &Value) -> Vec<String> {
    vec![
        format!(
            "task_readiness_mode: {}",
            task_readiness
                .get("recommended_mode")
                .and_then(Value::as_str)
                .unwrap_or("sequential")
        ),
        format!(
            "task_readiness_mixed: {}",
            task_readiness
                .get("can_run_mixed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
        format!(
            "task_readiness_parallel: {}",
            task_readiness
                .get("can_run_parallel")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
        format!(
            "task_readiness_waves: {}",
            task_readiness
                .get("waves")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        format!(
            "task_readiness_parallel_waves: {}",
            task_readiness
                .get("parallel_waves")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        format!(
            "task_readiness_largest_parallel_wave: {}",
            task_readiness
                .get("largest_parallel_wave")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
    ]
}

fn print_task_readiness() {
    println!();
    println!("== task readiness ==");
    let task_readiness = match read_tasks() {
        Ok(tasks) => task_readiness_value(&tasks, "pending"),
        Err(e) => serde_json::json!({
            "recommended_mode": "sequential",
            "can_run_mixed": false,
            "can_run_parallel": false,
            "waves": 0,
            "parallel_waves": 0,
            "largest_parallel_wave": 0,
            "strict_plan_reason": format!("task_read_failed: {e}")
        }),
    };
    for line in readiness_summary_lines(&task_readiness) {
        println!("{line}");
    }
    if let Some(reason) = task_readiness
        .get("strict_plan_reason")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
    {
        println!("task_readiness_reason: {reason}");
    }
}

pub(crate) fn latest_run_all_sum() -> Option<Value> {
    resolve_log_file()
        .and_then(|p| load_values(&p, 200).ok())
        .and_then(|rows| {
            rows.into_iter()
                .rev()
                .find(|v| v.get("tool").and_then(Value::as_str) == Some("cxtask_runall"))
        })
}

pub(crate) fn recent_run_all_sums(limit: usize) -> Vec<Value> {
    resolve_log_file()
        .and_then(|p| load_values(&p, 400).ok())
        .map(|rows| {
            rows.into_iter()
                .rev()
                .filter(|v| v.get("tool").and_then(Value::as_str) == Some("cxtask_runall"))
                .take(limit)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn latest_wave_sum() -> Option<Value> {
    let rows = resolve_log_file().and_then(|p| load_values(&p, 200).ok())?;
    let mut latest_wave_index: Option<u64> = None;
    let mut latest_wave_mode: Option<String> = None;
    let mut latest_wave_size = 0u64;
    let mut max_queue_wave_index: Option<u64> = None;
    let mut max_queue_wave_ms = 0u64;

    for row in rows {
        let Some(wave_index) = row.get("wave_index").and_then(Value::as_u64) else {
            continue;
        };
        let has_task = row
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|task| !task.is_empty());
        if !has_task {
            continue;
        }
        latest_wave_index = Some(wave_index);
        latest_wave_mode = row
            .get("wave_mode")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        latest_wave_size = row.get("wave_size").and_then(Value::as_u64).unwrap_or(0);
        let queue_ms = row.get("queue_ms").and_then(Value::as_u64).unwrap_or(0);
        if queue_ms >= max_queue_wave_ms {
            max_queue_wave_ms = queue_ms;
            max_queue_wave_index = Some(wave_index);
        }
    }

    latest_wave_index.map(|idx| {
        serde_json::json!({
            "latest_wave_index": idx,
            "latest_wave_mode": latest_wave_mode,
            "latest_wave_size": latest_wave_size,
            "max_queue_wave_index": max_queue_wave_index,
            "max_queue_wave_ms": max_queue_wave_ms
        })
    })
}

fn wave_pressure_note(mode: &str, wave: &Value) -> Option<String> {
    let max_queue_wave_ms = wave
        .get("max_queue_wave_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_queue_wave_index = wave
        .get("max_queue_wave_index")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if max_queue_wave_index <= 1 || max_queue_wave_ms < WAVE_QUEUE_PRESSURE_MS {
        return None;
    }
    Some(format!(
        "queue pressure is concentrating in later waves; latest mode={mode}, max queue was {}ms in wave {}",
        max_queue_wave_ms, max_queue_wave_index
    ))
}

fn next_action_kind(command: &str) -> &'static str {
    if command_matches_cli(command, "task run-all") {
        "rerun"
    } else if command_matches_cli(command, "scheduler") {
        "inspect_scheduler"
    } else if command_matches_cli(command, "task check") {
        "inspect_tasks"
    } else if command_matches_cli(command, "diag") {
        "inspect_diag"
    } else if command_matches_cli(command, "doctor") {
        "inspect_doctor"
    } else {
        "inspect"
    }
}

pub(crate) fn next_cost_class(command: &str) -> &'static str {
    if command_matches_cli(command, "task run-all")
        || command_matches_cli(command, "task check")
        || command_matches_cli(command, "scheduler")
    {
        "cheap"
    } else {
        "moderate"
    }
}

pub(crate) fn next_reasoning_required(command: &str) -> &'static str {
    if command_matches_cli(command, "task run-all")
        || command_matches_cli(command, "task check")
        || command_matches_cli(command, "scheduler")
    {
        "none"
    } else if command_matches_cli(command, "doctor") {
        "deep"
    } else {
        "light"
    }
}

pub(crate) fn next_quality_risk(command: &str) -> &'static str {
    if command_matches_cli(command, "task run-all")
        || command_matches_cli(command, "task check")
        || command_matches_cli(command, "scheduler")
    {
        "low"
    } else {
        "medium"
    }
}

fn next_escalates_if(command: &str) -> &'static str {
    if command_matches_cli(command, "task run-all") {
        "rerun still fails, leaves work unscheduled, or state changes"
    } else if command_matches_cli(command, "task check") {
        "plan remains blocked or recommendation conflicts with latest scheduler state"
    } else if command_matches_cli(command, "scheduler") {
        "scheduler inspection does not explain the latest halt, fallback, or queue pressure"
    } else if command_matches_cli(command, "diag") {
        "diagnostic summary remains ambiguous or quality risk rises"
    } else if command_matches_cli(command, "doctor") {
        "doctor guidance conflicts with current task or scheduler evidence"
    } else {
        "current structured path does not resolve the issue"
    }
}

fn summary_failed(summary: &Value) -> bool {
    summary
        .get("run_all_halted_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
        || summary
            .get("run_all_critical_errors")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        || summary
            .get("run_all_failed")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        || summary
            .get("run_all_blocked")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
}

pub(crate) fn exec_context_value(
    latest_summary: Option<&Value>,
    current_resume: Option<&str>,
) -> Value {
    let recent = recent_run_all_sums(8);
    let last_successful_action = recent
        .iter()
        .find(|summary| !summary_failed(summary))
        .and_then(|summary| summary.get("run_all_invocation_command"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let last_failed_action = recent
        .iter()
        .find(|summary| summary_failed(summary))
        .and_then(|summary| summary.get("run_all_invocation_command"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let last_mode_used = latest_summary
        .and_then(|summary| summary.get("run_all_mode"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let repeated_failure_pattern = latest_summary
        .filter(|summary| summary_failed(summary))
        .and_then(|summary| {
            let current = summary
                .get("run_all_failure_pattern")
                .and_then(Value::as_str)
                .filter(|pattern| !pattern.is_empty() && *pattern != "clean")?;
            if recent.iter().skip(1).any(|prior| {
                summary_failed(prior)
                    && prior.get("run_all_failure_pattern").and_then(Value::as_str) == Some(current)
            }) {
                Some(current.to_string())
            } else {
                None
            }
        });
    let prior_resume = latest_summary
        .and_then(|summary| summary.get("run_all_recommended_resume_point"))
        .and_then(Value::as_str);
    let recommended_resume_point = current_resume.or(prior_resume).map(ToString::to_string);
    let resume_reuses_prior_action = current_resume.is_some()
        && current_resume == prior_resume
        && repeated_failure_pattern.is_none();
    serde_json::json!({
        "last_successful_action": last_successful_action,
        "last_failed_action": last_failed_action,
        "last_mode_used": last_mode_used,
        "repeated_failure_pattern": repeated_failure_pattern,
        "recommended_resume_point": recommended_resume_point,
        "resume_reuses_prior_action": resume_reuses_prior_action
    })
}

pub(crate) fn phase7_metrics_value(limit: usize) -> Value {
    let mut recent = recent_run_all_sums(limit);
    let window_runs = recent.len() as u64;
    if recent.is_empty() {
        return serde_json::json!({
            "window_runs": 0,
            "actions_until_resolution": 0,
            "expensive_action_rate": 0.0,
            "repeat_diagnosis_rate": 0.0,
            "resume_reuse_rate": 0.0,
            "structured_action_success_rate": 0.0
        });
    }
    recent.reverse();
    let expensive_action_rows = recent
        .iter()
        .filter(|summary| {
            summary
                .get("run_all_recommended_resume_point")
                .and_then(Value::as_str)
                .is_some_and(|cmd| next_cost_class(cmd) == "expensive")
        })
        .count() as u64;
    let mut repeat_diagnosis_rows = 0u64;
    let mut resume_reuse_rows = 0u64;
    let mut structured_action_total = 0u64;
    let mut structured_action_success_rows = 0u64;
    let mut streak = 0u64;
    let mut actions_until_resolution = 0u64;

    for (idx, summary) in recent.iter().enumerate() {
        streak += 1;
        if idx > 0 {
            let prior = &recent[idx - 1];
            let current_pattern = summary
                .get("run_all_failure_pattern")
                .and_then(Value::as_str)
                .unwrap_or("clean");
            let prior_pattern = prior
                .get("run_all_failure_pattern")
                .and_then(Value::as_str)
                .unwrap_or("clean");
            if current_pattern != "clean" && current_pattern == prior_pattern {
                repeat_diagnosis_rows += 1;
            }
            let current_resume = summary
                .get("run_all_recommended_resume_point")
                .and_then(Value::as_str);
            let prior_resume = prior
                .get("run_all_recommended_resume_point")
                .and_then(Value::as_str);
            if current_resume.is_some() && current_resume == prior_resume {
                resume_reuse_rows += 1;
            }
            if let Some(prior_resume_cmd) = prior_resume
                && next_cost_class(prior_resume_cmd) == "cheap"
                && next_reasoning_required(prior_resume_cmd) == "none"
            {
                structured_action_total += 1;
                if !summary_failed(summary) {
                    structured_action_success_rows += 1;
                }
            }
        }
        if !summary_failed(summary) {
            actions_until_resolution = streak;
            streak = 0;
        }
    }
    if streak > 0 {
        actions_until_resolution = streak;
    }
    let transition_count = window_runs.saturating_sub(1);
    let repeat_diagnosis_rate = if transition_count == 0 {
        0.0
    } else {
        repeat_diagnosis_rows as f64 / transition_count as f64
    };
    let resume_reuse_rate = if transition_count == 0 {
        0.0
    } else {
        resume_reuse_rows as f64 / transition_count as f64
    };
    let expensive_action_rate = expensive_action_rows as f64 / window_runs as f64;
    let structured_action_success_rate = if structured_action_total == 0 {
        0.0
    } else {
        structured_action_success_rows as f64 / structured_action_total as f64
    };
    serde_json::json!({
        "window_runs": window_runs,
        "actions_until_resolution": actions_until_resolution,
        "expensive_action_rate": expensive_action_rate,
        "repeat_diagnosis_rate": repeat_diagnosis_rate,
        "resume_reuse_rate": resume_reuse_rate,
        "structured_action_success_rate": structured_action_success_rate
    })
}

pub(crate) fn phase7_metric_lines(limit: usize) -> Vec<String> {
    let value = phase7_metrics_value(limit);
    vec![
        format!(
            "phase7_window_runs: {}",
            value
                .get("window_runs")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        format!(
            "phase7_actions_until_resolution: {}",
            value
                .get("actions_until_resolution")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        format!(
            "phase7_expensive_action_rate: {:.3}",
            value
                .get("expensive_action_rate")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        ),
        format!(
            "phase7_repeat_diagnosis_rate: {:.3}",
            value
                .get("repeat_diagnosis_rate")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        ),
        format!(
            "phase7_resume_reuse_rate: {:.3}",
            value
                .get("resume_reuse_rate")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        ),
        format!(
            "phase7_structured_action_success_rate: {:.3}",
            value
                .get("structured_action_success_rate")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        ),
    ]
}

pub(crate) fn adapter_policy_lines() -> Vec<String> {
    let value = adapter_policy_value();
    vec![
        format!(
            "adapter_rollout_default_transport: {}",
            value
                .get("default_transport")
                .and_then(Value::as_str)
                .unwrap_or("process")
        ),
        format!(
            "adapter_rollout_http_opt_in: {}",
            value
                .get("http_transport_opt_in")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        ),
        format!(
            "adapter_rollout_http_override_required: {}",
            value
                .get("explicit_override_required_for_http")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        ),
        format!(
            "adapter_rollout_selected_adapter: {}",
            value
                .get("selected_adapter")
                .and_then(Value::as_str)
                .unwrap_or("primary-cli")
        ),
        format!(
            "adapter_rollout_selected_transport: {}",
            value
                .get("selected_transport")
                .and_then(Value::as_str)
                .unwrap_or("process")
        ),
        format!(
            "adapter_rollout_selected_status: {}",
            value
                .get("selected_status")
                .and_then(Value::as_str)
                .unwrap_or("stable")
        ),
        format!(
            "adapter_rollout_explicit_override_set: {}",
            value
                .get("explicit_override_set")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
        format!(
            "adapter_rollout_default_switch_guard: {}",
            value
                .get("default_switch_guard")
                .and_then(Value::as_str)
                .unwrap_or("two_green_ci_windows")
        ),
    ]
}

fn phase7_bias_value(latest_summary: Option<&Value>) -> Value {
    let metrics = phase7_metrics_value(20);
    let repeat_diagnosis_rate = metrics
        .get("repeat_diagnosis_rate")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let resume_reuse_rate = metrics
        .get("resume_reuse_rate")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let resume_point = latest_summary
        .and_then(|summary| summary.get("run_all_recommended_resume_point"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let applied =
        repeat_diagnosis_rate >= 0.5 && resume_reuse_rate >= 0.5 && resume_point.is_some();
    let reason = if applied {
        "repeated diagnosis and repeated resume reuse indicate the known resume point should lead"
    } else {
        "no phase7 ranking bias applied"
    };
    serde_json::json!({
        "applied": applied,
        "reason": reason,
        "repeat_diagnosis_rate": repeat_diagnosis_rate,
        "resume_reuse_rate": resume_reuse_rate,
        "resume_point": resume_point
    })
}

pub(crate) fn reasoning_gate_value(
    command: Option<&str>,
    cost_class: Option<&str>,
    reasoning_required: Option<&str>,
    quality_risk: Option<&str>,
    blockers: &[String],
    allow_no_reasoning: bool,
) -> Value {
    let mode = if allow_no_reasoning {
        "no_reasoning_needed"
    } else if command.is_some_and(|cmd| command_matches_cli(cmd, "task run-all"))
        && cost_class == Some("cheap")
        && reasoning_required == Some("none")
        && quality_risk == Some("low")
    {
        "cheap_structured_action"
    } else if blockers
        .iter()
        .any(|blocker| blocker == "repeated_failure_pattern")
        || reasoning_required == Some("deep")
        || quality_risk == Some("high")
    {
        "expensive_reasoning_required"
    } else {
        "cheap_diagnosis"
    };
    let why = match mode {
        "no_reasoning_needed" => {
            "current state is executable without additional reasoning".to_string()
        }
        "cheap_structured_action" => {
            "a typed structured action is sufficient on current evidence".to_string()
        }
        "cheap_diagnosis" => {
            "a low-cost diagnostic surface should run before deeper reasoning".to_string()
        }
        _ => "state remains ambiguous or quality-sensitive; escalate reasoning explicitly"
            .to_string(),
    };
    serde_json::json!({
        "mode": mode,
        "why": why,
        "blockers": blockers
    })
}

fn next_action_value(advice: &str, recommendations: &[Value]) -> Value {
    let default_command = command_with_cli("diag --json");
    let primary = recommendations
        .first()
        .and_then(Value::as_str)
        .unwrap_or(default_command.as_str());
    serde_json::json!({
        "kind": next_action_kind(primary),
        "command": primary,
        "reason": advice,
        "cost_class": next_cost_class(primary),
        "reasoning_required": next_reasoning_required(primary),
        "quality_risk": next_quality_risk(primary),
        "escalates_if": next_escalates_if(primary)
    })
}

pub(crate) fn exec_action_value(task_execution: &Value) -> Option<Value> {
    let next_action = task_execution.get("next_action")?;
    let command = next_action.get("command").and_then(Value::as_str)?;
    if command.trim().is_empty() {
        return None;
    }
    let kind = next_action
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("operator_followup");
    let advice = task_execution
        .get("advice")
        .and_then(Value::as_str)
        .unwrap_or("Review latest task execution state.");
    let pressure_kind = task_execution
        .get("wave_pressure")
        .and_then(|v| v.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    let halted_remaining = task_execution
        .get("halted_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let severity = if halted_remaining > 0 {
        "critical"
    } else {
        "warning"
    };
    let phase7_bias = task_execution
        .get("phase7_bias")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let bias_note = if phase7_bias
        .get("applied")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        phase7_bias
            .get("reason")
            .and_then(Value::as_str)
            .map(|reason| format!(" Phase VII bias: {reason}."))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let rationale = if pressure_kind != "none" {
        format!("{advice} Wave pressure: {pressure_kind}.{bias_note}")
    } else {
        format!("{advice}{bias_note}")
    };
    Some(serde_json::json!({
        "id": format!("task_execution_{kind}"),
        "severity": severity,
        "rationale": rationale,
        "command": command
    }))
}

fn exec_next_lines(latest_summary: Option<&Value>, wave_summary: Option<&Value>) -> Vec<String> {
    let value = exec_diag_value(latest_summary, wave_summary);
    let default_command = command_with_cli("diag --json");
    let kind = value
        .get("next_action")
        .and_then(|v| v.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("inspect");
    let command = value
        .get("next_action")
        .and_then(|v| v.get("command"))
        .and_then(Value::as_str)
        .unwrap_or(default_command.as_str());
    vec![
        format!("task_execution_next_action_kind: {kind}"),
        format!("task_execution_next_action_command: {command}"),
    ]
}

fn exec_gate_lines(latest_summary: Option<&Value>, wave_summary: Option<&Value>) -> Vec<String> {
    let value = exec_diag_value(latest_summary, wave_summary);
    let gate = value
        .get("reasoning_gate")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let mode = gate
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("cheap_diagnosis");
    let why = gate
        .get("why")
        .and_then(Value::as_str)
        .unwrap_or("reasoning gate unavailable");
    let mut lines = vec![
        format!("task_execution_reasoning_gate_mode: {mode}"),
        format!("task_execution_reasoning_gate_why: {why}"),
    ];
    if let Some(blockers) = gate.get("blockers").and_then(Value::as_array) {
        for (idx, blocker) in blockers.iter().filter_map(Value::as_str).enumerate() {
            lines.push(format!(
                "task_execution_reasoning_gate_blocker_{}: {blocker}",
                idx + 1
            ));
        }
    }
    lines
}

fn exec_context_lines(latest_summary: Option<&Value>, wave_summary: Option<&Value>) -> Vec<String> {
    let value = exec_diag_value(latest_summary, wave_summary);
    let context = value
        .get("recent_context")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let mut lines = Vec::new();
    if let Some(v) = context
        .get("last_successful_action")
        .and_then(Value::as_str)
    {
        lines.push(format!(
            "task_execution_context_last_successful_action: {v}"
        ));
    }
    if let Some(v) = context.get("last_failed_action").and_then(Value::as_str) {
        lines.push(format!("task_execution_context_last_failed_action: {v}"));
    }
    if let Some(v) = context.get("last_mode_used").and_then(Value::as_str) {
        lines.push(format!("task_execution_context_last_mode_used: {v}"));
    }
    if let Some(v) = context
        .get("repeated_failure_pattern")
        .and_then(Value::as_str)
    {
        lines.push(format!(
            "task_execution_context_repeated_failure_pattern: {v}"
        ));
    }
    if let Some(v) = context
        .get("recommended_resume_point")
        .and_then(Value::as_str)
    {
        lines.push(format!(
            "task_execution_context_recommended_resume_point: {v}"
        ));
    }
    lines.push(format!(
        "task_execution_context_resume_reuses_prior_action: {}",
        context
            .get("resume_reuses_prior_action")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    ));
    lines
}

fn exec_bias_lines(latest_summary: Option<&Value>) -> Vec<String> {
    let value = phase7_bias_value(latest_summary);
    let applied = value
        .get("applied")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("no phase7 ranking bias applied");
    let repeat_diagnosis_rate = value
        .get("repeat_diagnosis_rate")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let resume_reuse_rate = value
        .get("resume_reuse_rate")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let mut lines = vec![
        format!("task_execution_phase7_bias_applied: {applied}"),
        format!("task_execution_phase7_bias_reason: {reason}"),
        format!(
            "task_execution_phase7_bias_repeat_diagnosis_rate: {:.3}",
            repeat_diagnosis_rate
        ),
        format!(
            "task_execution_phase7_bias_resume_reuse_rate: {:.3}",
            resume_reuse_rate
        ),
    ];
    if let Some(resume_point) = value.get("resume_point").and_then(Value::as_str) {
        lines.push(format!(
            "task_execution_phase7_bias_resume_point: {resume_point}"
        ));
    }
    lines
}

fn exec_wave_lines(latest_summary: Option<&Value>, wave_summary: Option<&Value>) -> Vec<String> {
    let value = wave_pressure_value(latest_summary, wave_summary);
    let kind = value.get("kind").and_then(Value::as_str).unwrap_or("none");
    let suggested_mode = value
        .get("suggested_mode")
        .and_then(Value::as_str)
        .unwrap_or("none");
    vec![
        format!("task_execution_wave_pressure: {kind}"),
        format!("task_execution_wave_pressure_mode: {suggested_mode}"),
    ]
}

fn exec_concurrency_value(latest_summary: Option<&Value>) -> Value {
    let worker_count = latest_summary
        .and_then(|summary| summary.get("run_all_worker_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let workers = latest_summary
        .and_then(|summary| summary.get("run_all_workers"))
        .and_then(Value::as_str)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    let max_retry_attempt = latest_summary
        .and_then(|summary| summary.get("run_all_max_retry_attempt"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let first_queue_started_at = latest_summary
        .and_then(|summary| summary.get("run_all_first_queue_started_at"))
        .cloned()
        .unwrap_or(Value::Null);
    let first_task_started_at = latest_summary
        .and_then(|summary| summary.get("run_all_first_task_started_at"))
        .cloned()
        .unwrap_or(Value::Null);
    let last_task_finished_at = latest_summary
        .and_then(|summary| summary.get("run_all_last_task_finished_at"))
        .cloned()
        .unwrap_or(Value::Null);
    serde_json::json!({
        "worker_count": worker_count,
        "workers": workers,
        "max_retry_attempt": max_retry_attempt,
        "first_queue_started_at": first_queue_started_at,
        "first_task_started_at": first_task_started_at,
        "last_task_finished_at": last_task_finished_at
    })
}

fn exec_invariants_value(latest_summary: Option<&Value>) -> Value {
    let Some(summary) = latest_summary else {
        return serde_json::json!({
            "ok": false,
            "status": "unknown",
            "issues": ["missing_run_summary"],
            "outcome_accounting_ok": false,
            "failure_accounting_ok": false,
            "critical_halt_ok": false,
            "worker_summary_ok": false,
            "timing_window_ok": false
        });
    };

    let scheduled = summary
        .get("run_all_scheduled")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let complete = summary
        .get("run_all_complete")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let failed = summary
        .get("run_all_failed")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let blocked = summary
        .get("run_all_blocked")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let retryable_failures = summary
        .get("run_all_retryable_failures")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let non_retryable_failures = summary
        .get("run_all_non_retryable_failures")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let critical_errors = summary
        .get("run_all_critical_errors")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let halted_remaining = summary
        .get("run_all_halted_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let halt_on_critical = summary
        .get("halt_on_critical")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let worker_count = summary
        .get("run_all_worker_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let workers = summary
        .get("run_all_workers")
        .and_then(Value::as_str)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .count() as u64
        })
        .unwrap_or(0);
    let first_queue_started_at = summary
        .get("run_all_first_queue_started_at")
        .and_then(Value::as_str);
    let first_task_started_at = summary
        .get("run_all_first_task_started_at")
        .and_then(Value::as_str);
    let last_task_finished_at = summary
        .get("run_all_last_task_finished_at")
        .and_then(Value::as_str);

    let outcome_accounting_ok = scheduled == complete + failed + halted_remaining;
    let failure_accounting_ok = failed == blocked + retryable_failures + non_retryable_failures;
    let critical_halt_ok =
        critical_errors <= non_retryable_failures && (halted_remaining == 0 || halt_on_critical);
    let worker_summary_ok =
        worker_count == workers && !(complete + failed > 0 && worker_count == 0);
    let timing_window_ok = match (
        first_queue_started_at,
        first_task_started_at,
        last_task_finished_at,
    ) {
        (None, None, None) => true,
        (Some(queue), Some(start), Some(finish)) => queue <= start && start <= finish,
        _ => false,
    };

    let mut issues = Vec::new();
    if !outcome_accounting_ok {
        issues.push("outcome_accounting_mismatch");
    }
    if !failure_accounting_ok {
        issues.push("failure_accounting_mismatch");
    }
    if !critical_halt_ok {
        issues.push("critical_halt_mismatch");
    }
    if !worker_summary_ok {
        issues.push("worker_summary_mismatch");
    }
    if !timing_window_ok {
        issues.push("timing_window_mismatch");
    }

    serde_json::json!({
        "ok": issues.is_empty(),
        "status": if issues.is_empty() { "clean" } else { "violated" },
        "issues": issues,
        "outcome_accounting_ok": outcome_accounting_ok,
        "failure_accounting_ok": failure_accounting_ok,
        "critical_halt_ok": critical_halt_ok,
        "worker_summary_ok": worker_summary_ok,
        "timing_window_ok": timing_window_ok
    })
}

fn wave_pressure_value(latest_summary: Option<&Value>, wave_summary: Option<&Value>) -> Value {
    let latest_wave_index = latest_summary
        .and_then(|s| s.get("run_all_latest_wave_index"))
        .cloned()
        .or_else(|| {
            wave_summary
                .and_then(|w| w.get("latest_wave_index"))
                .cloned()
        })
        .unwrap_or(Value::Null);
    let max_queue_wave_index = latest_summary
        .and_then(|s| s.get("run_all_max_queue_wave_index"))
        .cloned()
        .or_else(|| {
            wave_summary
                .and_then(|w| w.get("max_queue_wave_index"))
                .cloned()
        })
        .unwrap_or(Value::Null);
    let max_queue_wave_ms = latest_summary
        .and_then(|s| s.get("run_all_max_queue_wave_ms"))
        .and_then(Value::as_u64)
        .or_else(|| {
            wave_summary
                .and_then(|w| w.get("max_queue_wave_ms"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0);
    let kind = latest_summary
        .and_then(|s| s.get("run_all_wave_pressure_kind"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if max_queue_wave_index.as_u64().unwrap_or(0) > 1
                && max_queue_wave_ms >= WAVE_QUEUE_PRESSURE_MS
            {
                "later_wave_queue".to_string()
            } else {
                "none".to_string()
            }
        });
    let suggested_mode = latest_summary
        .and_then(|s| s.get("run_all_wave_pressure_suggested_mode"))
        .cloned()
        .unwrap_or(Value::Null);
    serde_json::json!({
        "kind": kind,
        "suggested_mode": suggested_mode,
        "latest_wave_index": latest_wave_index,
        "max_queue_wave_index": max_queue_wave_index,
        "max_queue_wave_ms": max_queue_wave_ms
    })
}

pub(crate) fn exec_recommendations_value(
    latest_summary: Option<&Value>,
    wave_summary: Option<&Value>,
) -> Vec<Value> {
    let Some(summary) = latest_summary else {
        let wave = wave_pressure_value(None, wave_summary);
        let commands = if wave_pressure_note("unknown", &wave).is_some() {
            vec![
                command_with_cli("task run-all --dry-run --json"),
                command_with_cli("scheduler --json --window 20"),
            ]
        } else {
            vec![
                command_with_cli("task check --json"),
                command_with_cli("scheduler --json"),
            ]
        };
        return commands.into_iter().map(Value::String).collect();
    };
    let halted_remaining = summary
        .get("run_all_halted_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_rows = summary
        .get("run_all_backend_fallback_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let failed = summary
        .get("run_all_failed")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let critical = summary
        .get("run_all_critical_errors")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mode = summary
        .get("run_all_mode")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let wave = wave_pressure_value(Some(summary), wave_summary);

    let commands = if halted_remaining > 0 {
        vec![
            command_with_cli("scheduler --json --window 20"),
            command_with_cli("task run-all --status pending"),
        ]
    } else if critical > 0 {
        vec![
            command_with_cli("scheduler --json --window 20"),
            command_with_cli("task check --json"),
        ]
    } else if fallback_rows > 0 {
        vec![
            command_with_cli("scheduler --json --window 20"),
            command_with_cli("doctor"),
        ]
    } else if failed > 0 {
        vec![
            command_with_cli("task check --json"),
            command_with_cli("task run-all --status pending"),
        ]
    } else if wave_pressure_note(mode, &wave).is_some() {
        let narrower = match mode {
            "parallel" => command_with_cli("task run-all --mode mixed --status pending"),
            "mixed" => command_with_cli("task run-all --mode sequential --status pending"),
            _ => command_with_cli("scheduler --json --window 20"),
        };
        vec![narrower, command_with_cli("scheduler --json --window 20")]
    } else {
        vec![
            command_with_cli("diag --json"),
            command_with_cli("scheduler --json"),
        ]
    };
    let bias = phase7_bias_value(latest_summary);
    let mut commands: Vec<Value> = commands.into_iter().map(Value::String).collect();
    if bias.get("applied").and_then(Value::as_bool) == Some(true)
        && let Some(resume_point) = bias.get("resume_point").and_then(Value::as_str)
        && let Some(idx) = commands
            .iter()
            .position(|value| value.as_str() == Some(resume_point))
        && idx > 0
    {
        let prioritized = commands.remove(idx);
        commands.insert(0, prioritized);
    }
    commands
}

fn exec_advice_value(latest_summary: Option<&Value>, wave_summary: Option<&Value>) -> Value {
    let Some(summary) = latest_summary else {
        let wave = wave_pressure_value(None, wave_summary);
        return Value::String(
            wave_pressure_note("unknown", &wave)
                .unwrap_or_else(|| "no recent task run-all summary".to_string()),
        );
    };
    let halted_remaining = summary
        .get("run_all_halted_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_rows = summary
        .get("run_all_backend_fallback_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_map = summary
        .get("run_all_backend_fallbacks")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .unwrap_or("none");
    let failed = summary
        .get("run_all_failed")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let critical = summary
        .get("run_all_critical_errors")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mode = summary
        .get("run_all_mode")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let wave = wave_pressure_value(Some(summary), wave_summary);

    let advice = if halted_remaining > 0 {
        format!(
            "rerun pending work after resolving the critical stop; {halted_remaining} task(s) were left unscheduled"
        )
    } else if critical > 0 {
        "inspect the latest critical failure before widening concurrency".to_string()
    } else if fallback_rows > 0 {
        format!("review backend availability or pool policy; fallback observed: {fallback_map}")
    } else if failed > 0 {
        "inspect failed task ids from the latest run-all summary before retrying".to_string()
    } else if let Some(advice) = wave_pressure_note(mode, &wave) {
        advice
    } else {
        "latest run-all summary is operationally clean".to_string()
    };
    Value::String(advice)
}

pub(crate) fn exec_advice_lines(
    latest_summary: Option<&Value>,
    wave_summary: Option<&Value>,
) -> Vec<String> {
    let Some(summary) = latest_summary else {
        let wave = wave_pressure_value(None, wave_summary);
        let latest_wave_index = wave
            .get("latest_wave_index")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let max_queue_wave_index = wave
            .get("max_queue_wave_index")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let max_queue_wave_ms = wave
            .get("max_queue_wave_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let mut lines = vec![
            "task_execution_last_mode: unknown".to_string(),
            "task_execution_halted_remaining: 0".to_string(),
            "task_execution_backend_fallback_rows: 0".to_string(),
            format!("task_execution_latest_wave_index: {latest_wave_index}"),
            format!("task_execution_max_queue_wave_index: {max_queue_wave_index}"),
            format!("task_execution_max_queue_wave_ms: {max_queue_wave_ms}"),
            "task_execution_worker_count: 0".to_string(),
            "task_execution_max_retry_attempt: 0".to_string(),
        ];
        let invariants = exec_invariants_value(None);
        lines.push(format!(
            "task_execution_invariants_status: {}",
            invariants
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ));
        let advice_value = exec_advice_value(None, wave_summary);
        let advice = advice_value
            .as_str()
            .unwrap_or("no recent task run-all summary");
        lines.push(format!("task_execution_advice: {advice}"));
        return lines;
    };
    let halted_remaining = summary
        .get("run_all_halted_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_rows = summary
        .get("run_all_backend_fallback_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mode = summary
        .get("run_all_mode")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let wave = wave_pressure_value(Some(summary), wave_summary);
    let latest_wave_index = wave
        .get("latest_wave_index")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_queue_wave_index = wave
        .get("max_queue_wave_index")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_queue_wave_ms = wave
        .get("max_queue_wave_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let concurrency = exec_concurrency_value(Some(summary));
    let worker_count = concurrency
        .get("worker_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_retry_attempt = concurrency
        .get("max_retry_attempt")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let invariants = exec_invariants_value(Some(summary));

    let mut lines = vec![
        format!("task_execution_last_mode: {mode}"),
        format!("task_execution_halted_remaining: {halted_remaining}"),
        format!("task_execution_backend_fallback_rows: {fallback_rows}"),
        format!("task_execution_latest_wave_index: {latest_wave_index}"),
        format!("task_execution_max_queue_wave_index: {max_queue_wave_index}"),
        format!("task_execution_max_queue_wave_ms: {max_queue_wave_ms}"),
        format!("task_execution_worker_count: {worker_count}"),
        format!("task_execution_max_retry_attempt: {max_retry_attempt}"),
        format!(
            "task_execution_invariants_status: {}",
            invariants
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
    ];
    if let Some(ok) = invariants.get("ok").and_then(Value::as_bool) {
        lines.push(format!("task_execution_invariants_ok: {ok}"));
    }
    if let Some(issues) = invariants.get("issues").and_then(Value::as_array) {
        for (idx, issue) in issues.iter().filter_map(Value::as_str).enumerate() {
            lines.push(format!(
                "task_execution_invariant_issue_{}: {issue}",
                idx + 1
            ));
        }
    }
    if let Some(workers) = concurrency.get("workers").and_then(Value::as_array)
        && !workers.is_empty()
    {
        let workers = workers
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<&str>>()
            .join(",");
        lines.push(format!("task_execution_workers: {workers}"));
    }
    if let Some(v) = concurrency
        .get("first_queue_started_at")
        .and_then(Value::as_str)
    {
        lines.push(format!("task_execution_first_queue_started_at: {v}"));
    }
    if let Some(v) = concurrency
        .get("first_task_started_at")
        .and_then(Value::as_str)
    {
        lines.push(format!("task_execution_first_task_started_at: {v}"));
    }
    if let Some(v) = concurrency
        .get("last_task_finished_at")
        .and_then(Value::as_str)
    {
        lines.push(format!("task_execution_last_task_finished_at: {v}"));
    }

    let advice_value = exec_advice_value(Some(summary), wave_summary);
    let advice = advice_value
        .as_str()
        .unwrap_or("no recent task run-all summary");
    lines.push(format!("task_execution_advice: {advice}"));
    lines
}

pub(crate) fn exec_reco_lines(
    latest_summary: Option<&Value>,
    wave_summary: Option<&Value>,
) -> Vec<String> {
    exec_recommendations_value(latest_summary, wave_summary)
        .into_iter()
        .enumerate()
        .filter_map(|(idx, cmd)| {
            cmd.as_str()
                .map(|command| format!("task_execution_recommendation_{}: {command}", idx + 1))
        })
        .collect()
}

pub(crate) fn exec_diag_value(
    latest_summary: Option<&Value>,
    wave_summary: Option<&Value>,
) -> Value {
    let wave_pressure = wave_pressure_value(latest_summary, wave_summary);
    let latest_wave_index = wave_pressure
        .get("latest_wave_index")
        .cloned()
        .unwrap_or(Value::Null);
    let max_queue_wave_index = wave_pressure
        .get("max_queue_wave_index")
        .cloned()
        .unwrap_or(Value::Null);
    let max_queue_wave_ms = wave_pressure
        .get("max_queue_wave_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let advice = exec_advice_value(latest_summary, wave_summary)
        .as_str()
        .unwrap_or("no recent task run-all summary")
        .to_string();
    let recommendations = exec_recommendations_value(latest_summary, wave_summary);
    let mode = latest_summary
        .and_then(|summary| summary.get("run_all_mode"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let halted_remaining = latest_summary
        .and_then(|summary| summary.get("run_all_halted_remaining"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_rows = latest_summary
        .and_then(|summary| summary.get("run_all_backend_fallback_rows"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let concurrency = exec_concurrency_value(latest_summary);
    let invariants = exec_invariants_value(latest_summary);
    let next_action = next_action_value(&advice, &recommendations);
    let mut blockers = Vec::new();
    if latest_summary.is_none() {
        blockers.push("missing_run_summary".to_string());
    }
    if halted_remaining > 0 {
        blockers.push("halted_remaining".to_string());
    }
    if fallback_rows > 0 {
        blockers.push("backend_fallback".to_string());
    }
    if wave_pressure
        .get("kind")
        .and_then(Value::as_str)
        .is_some_and(|v| v != "none")
    {
        blockers.push("later_wave_queue".to_string());
    }
    let recent_context = exec_context_value(
        latest_summary,
        next_action.get("command").and_then(Value::as_str),
    );
    let phase7_bias = phase7_bias_value(latest_summary);
    if recent_context
        .get("repeated_failure_pattern")
        .and_then(Value::as_str)
        .is_some()
    {
        blockers.push("repeated_failure_pattern".to_string());
    }
    let reasoning_gate = reasoning_gate_value(
        next_action.get("command").and_then(Value::as_str),
        next_action.get("cost_class").and_then(Value::as_str),
        next_action
            .get("reasoning_required")
            .and_then(Value::as_str),
        next_action.get("quality_risk").and_then(Value::as_str),
        &blockers,
        false,
    );
    serde_json::json!({
        "last_mode": mode,
        "halted_remaining": halted_remaining,
        "backend_fallback_rows": fallback_rows,
        "latest_wave_index": latest_wave_index,
        "max_queue_wave_index": max_queue_wave_index,
        "max_queue_wave_ms": max_queue_wave_ms,
        "concurrency": concurrency,
        "invariants": invariants,
        "wave_pressure": wave_pressure,
        "advice": advice,
        "recommendations": recommendations,
        "next_action": next_action,
        "recent_context": recent_context,
        "phase7_bias": phase7_bias,
        "reasoning_gate": reasoning_gate
    })
}

fn print_exec_advice() {
    println!();
    println!("== task execution advice ==");
    let latest_summary = latest_run_all_sum();
    let latest_wave = latest_wave_sum();
    for line in exec_advice_lines(latest_summary.as_ref(), latest_wave.as_ref()) {
        println!("{line}");
    }
    for line in exec_wave_lines(latest_summary.as_ref(), latest_wave.as_ref()) {
        println!("{line}");
    }
    for line in exec_next_lines(latest_summary.as_ref(), latest_wave.as_ref()) {
        println!("{line}");
    }
    for line in exec_context_lines(latest_summary.as_ref(), latest_wave.as_ref()) {
        println!("{line}");
    }
    for line in exec_bias_lines(latest_summary.as_ref()) {
        println!("{line}");
    }
    for line in exec_gate_lines(latest_summary.as_ref(), latest_wave.as_ref()) {
        println!("{line}");
    }
    for line in exec_reco_lines(latest_summary.as_ref(), latest_wave.as_ref()) {
        println!("{line}");
    }
    for line in phase7_metric_lines(20) {
        println!("{line}");
    }
    for line in adapter_policy_lines() {
        println!("{line}");
    }
}

pub fn print_doctor(run_llm_jsonl: JsonlRunner) -> i32 {
    let backend = llm_backend();
    let llm_bin = llm_bin_name();
    println!("== {} doctor ==", cli_app_name());
    let missing_required = check_required_bins(&backend, llm_bin);
    if missing_required > 0 {
        println!(
            "FAIL: install required binaries before using {}.",
            cli_app_name()
        );
        return 1;
    }
    if let Err(code) = probe_json_pipeline(&backend, run_llm_jsonl) {
        return code;
    }
    if let Err(code) = probe_text_pipeline(&backend, run_llm_jsonl) {
        return code;
    }
    print_task_readiness();
    print_exec_advice();
    print_git_context();

    println!();
    println!("PASS: core pipeline looks healthy.");
    0
}

pub fn cmd_health(run_llm_jsonl: JsonlRunner, run_cxo: CxoRunner) -> i32 {
    let backend = llm_backend();
    let llm_bin = llm_bin_name();
    println!("== {backend} version ==");
    let mut version_cmd = Command::new(llm_bin);
    version_cmd.arg("--version");
    match run_command_output_with_timeout(version_cmd, &format!("{llm_bin} --version")) {
        Ok(out) => print!("{}", String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            crate::cx_eprintln!("{} health: {backend} --version failed: {e}", cli_app_name());
            return 1;
        }
    }
    println!();
    println!("== {backend} json ==");
    let jsonl = match run_llm_jsonl("ping") {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} health: {backend} json failed: {e}", cli_app_name());
            return 1;
        }
    };
    let lines: Vec<&str> = jsonl.lines().collect();
    let keep = lines.len().saturating_sub(4);
    for line in &lines[keep..] {
        println!("{line}");
    }
    println!();
    println!("== _codex_text ==");
    let txt = extract_agent_text(&jsonl).unwrap_or_default();
    println!("{txt}");
    println!();
    println!("== cxo test ==");
    let code = run_cxo(&["git".to_string(), "status".to_string()]);
    if code != 0 {
        return code;
    }
    println!();
    println!("All systems operational.");
    0
}

#[cfg(test)]
mod tests {
    use super::{
        exec_advice_lines, exec_context_lines, exec_context_value, exec_diag_value,
        exec_gate_lines, exec_next_lines, exec_reco_lines, exec_wave_lines, phase7_metric_lines,
        readiness_summary_lines,
    };
    use serde_json::Value;

    #[test]
    fn readiness_summary_cov() {
        let lines = readiness_summary_lines(&serde_json::json!({
            "recommended_mode": "mixed",
            "can_run_mixed": true,
            "can_run_parallel": false,
            "waves": 3,
            "parallel_waves": 2,
            "largest_parallel_wave": 4
        }));
        let joined = lines.join("\n");
        assert!(joined.contains("task_readiness_mode: mixed"), "{joined}");
        assert!(joined.contains("task_readiness_mixed: true"), "{joined}");
        assert!(
            joined.contains("task_readiness_parallel: false"),
            "{joined}"
        );
        assert!(joined.contains("task_readiness_waves: 3"), "{joined}");
        assert!(
            joined.contains("task_readiness_parallel_waves: 2"),
            "{joined}"
        );
        assert!(
            joined.contains("task_readiness_largest_parallel_wave: 4"),
            "{joined}"
        );
    }

    #[test]
    fn exec_advice_cov() {
        let lines = exec_advice_lines(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_halted_remaining": 2,
                "run_all_backend_fallback_rows": 1,
                "run_all_backend_fallbacks": "primary->ollama=1",
                "run_all_wave_pressure_kind": "later_wave_queue",
                "run_all_wave_pressure_suggested_mode": "sequential",
                "run_all_latest_wave_index": 4,
                "run_all_max_queue_wave_index": 4,
                "run_all_max_queue_wave_ms": 2600,
                "run_all_failed": 1,
                "run_all_critical_errors": 1
            })),
            None,
        );
        let joined = lines.join("\n");
        assert!(
            joined.contains("task_execution_last_mode: mixed"),
            "{joined}"
        );
        assert!(
            joined.contains("task_execution_halted_remaining: 2"),
            "{joined}"
        );
        assert!(
            joined.contains("task_execution_backend_fallback_rows: 1"),
            "{joined}"
        );
        assert!(
            joined.contains("rerun pending work after resolving the critical stop"),
            "{joined}"
        );
    }

    #[test]
    fn exec_reco_cov() {
        let lines = exec_reco_lines(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_halted_remaining": 2,
                "run_all_backend_fallback_rows": 1,
                "run_all_failed": 1,
                "run_all_critical_errors": 1
            })),
            None,
        );
        let joined = lines.join("\n");
        assert!(
            joined.contains("task_execution_recommendation_1: xshelf scheduler --json --window 20"),
            "{joined}"
        );
        assert!(
            joined
                .contains("task_execution_recommendation_2: xshelf task run-all --status pending"),
            "{joined}"
        );
    }

    #[test]
    fn exec_diag_cov() {
        let value = exec_diag_value(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_halted_remaining": 2,
                "run_all_backend_fallback_rows": 1,
                "run_all_backend_fallbacks": "primary->ollama=1",
                "run_all_wave_pressure_kind": "later_wave_queue",
                "run_all_wave_pressure_suggested_mode": "sequential",
                "run_all_latest_wave_index": 4,
                "run_all_max_queue_wave_index": 4,
                "run_all_max_queue_wave_ms": 2600,
                "run_all_failed": 1,
                "run_all_critical_errors": 1
            })),
            None,
        );
        assert_eq!(
            value.get("last_mode").and_then(Value::as_str),
            Some("mixed")
        );
        assert_eq!(
            value.get("halted_remaining").and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            value.get("backend_fallback_rows").and_then(Value::as_u64),
            Some(1)
        );
        let recs = value
            .get("recommendations")
            .and_then(Value::as_array)
            .expect("recommendations");
        assert!(
            recs.iter()
                .any(|v| v.as_str() == Some("xshelf scheduler --json --window 20")),
            "{value}"
        );
        assert_eq!(
            value
                .get("next_action")
                .and_then(|v| v.get("kind"))
                .and_then(Value::as_str),
            Some("inspect_scheduler")
        );
        assert_eq!(
            value
                .get("next_action")
                .and_then(|v| v.get("cost_class"))
                .and_then(Value::as_str),
            Some("cheap")
        );
        assert_eq!(
            value
                .get("next_action")
                .and_then(|v| v.get("reasoning_required"))
                .and_then(Value::as_str),
            Some("none")
        );
        assert_eq!(
            value
                .get("next_action")
                .and_then(|v| v.get("quality_risk"))
                .and_then(Value::as_str),
            Some("low")
        );
        assert!(
            value
                .get("next_action")
                .and_then(|v| v.get("escalates_if"))
                .and_then(Value::as_str)
                .is_some_and(|v| v.contains("scheduler inspection")),
            "{value}"
        );
        assert_eq!(
            value
                .get("recent_context")
                .and_then(|v| v.get("last_mode_used"))
                .and_then(Value::as_str),
            Some("mixed")
        );
        assert_eq!(
            value
                .get("recent_context")
                .and_then(|v| v.get("recommended_resume_point"))
                .and_then(Value::as_str),
            Some("xshelf scheduler --json --window 20")
        );
        assert_eq!(
            value
                .get("wave_pressure")
                .and_then(|v| v.get("kind"))
                .and_then(Value::as_str),
            Some("later_wave_queue")
        );
        let next_lines = exec_next_lines(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_halted_remaining": 2,
                "run_all_backend_fallback_rows": 1,
                "run_all_backend_fallbacks": "primary->ollama=1",
                "run_all_failed": 1,
                "run_all_critical_errors": 1
            })),
            None,
        )
        .join("\n");
        assert!(
            next_lines.contains("task_execution_next_action_kind: inspect_scheduler"),
            "{next_lines}"
        );
        let gate_lines = exec_gate_lines(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_halted_remaining": 2,
                "run_all_backend_fallback_rows": 1,
                "run_all_backend_fallbacks": "primary->ollama=1",
                "run_all_failed": 1,
                "run_all_critical_errors": 1
            })),
            None,
        )
        .join("\n");
        assert!(
            gate_lines.contains("task_execution_reasoning_gate_mode: cheap_diagnosis"),
            "{gate_lines}"
        );
        assert!(
            gate_lines.contains("task_execution_reasoning_gate_blocker_1: halted_remaining"),
            "{gate_lines}"
        );
        let context_lines = exec_context_lines(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_halted_remaining": 2,
                "run_all_backend_fallback_rows": 1,
                "run_all_backend_fallbacks": "primary->ollama=1",
                "run_all_failed": 1,
                "run_all_critical_errors": 1
            })),
            None,
        )
        .join("\n");
        assert!(
            context_lines.contains("task_execution_context_last_mode_used: mixed"),
            "{context_lines}"
        );
        assert!(
            context_lines.contains(
                "task_execution_context_recommended_resume_point: xshelf scheduler --json --window 20"
            ),
            "{context_lines}"
        );
        let wave_lines = exec_wave_lines(
            Some(&serde_json::json!({
                "run_all_wave_pressure_kind": "later_wave_queue",
                "run_all_wave_pressure_suggested_mode": "sequential"
            })),
            None,
        )
        .join("\n");
        assert!(
            wave_lines.contains("task_execution_wave_pressure: later_wave_queue"),
            "{wave_lines}"
        );
    }

    #[test]
    fn exec_wave_cov() {
        let summary = serde_json::json!({
            "run_all_mode": "mixed",
            "run_all_halted_remaining": 0,
            "run_all_backend_fallback_rows": 0,
            "run_all_failed": 0,
            "run_all_critical_errors": 0
        });
        let wave = serde_json::json!({
            "latest_wave_index": 3,
            "max_queue_wave_index": 3,
            "max_queue_wave_ms": 2400
        });
        let lines = exec_advice_lines(Some(&summary), Some(&wave));
        let joined = lines.join("\n");
        assert!(
            joined.contains("task_execution_latest_wave_index: 3"),
            "{joined}"
        );
        assert!(
            joined.contains("queue pressure is concentrating in later waves"),
            "{joined}"
        );
        let recs = exec_reco_lines(Some(&summary), Some(&wave)).join("\n");
        assert!(
            recs.contains("task_execution_recommendation_1: xshelf task run-all --mode sequential --status pending"),
            "{recs}"
        );
        let value = exec_diag_value(Some(&summary), Some(&wave));
        assert_eq!(
            value.get("max_queue_wave_ms").and_then(Value::as_u64),
            Some(2400)
        );
        assert_eq!(
            value
                .get("next_action")
                .and_then(|v| v.get("command"))
                .and_then(Value::as_str),
            Some("xshelf task run-all --mode sequential --status pending")
        );
        assert_eq!(
            value
                .get("recent_context")
                .and_then(|v| v.get("recommended_resume_point"))
                .and_then(Value::as_str),
            Some("xshelf task run-all --mode sequential --status pending")
        );
    }

    #[test]
    fn exec_wave_missing() {
        let wave = serde_json::json!({
            "latest_wave_index": 4,
            "max_queue_wave_index": 4,
            "max_queue_wave_ms": 2600
        });
        let lines = exec_advice_lines(None, Some(&wave)).join("\n");
        assert!(
            lines.contains("task_execution_last_mode: unknown"),
            "{lines}"
        );
        assert!(
            lines.contains("queue pressure is concentrating in later waves"),
            "{lines}"
        );
        let recs = exec_reco_lines(None, Some(&wave)).join("\n");
        assert!(
            recs.contains("task_execution_recommendation_1: xshelf task run-all --dry-run --json"),
            "{recs}"
        );
        let value = exec_diag_value(None, Some(&wave));
        assert_eq!(
            value.get("latest_wave_index").and_then(Value::as_u64),
            Some(4)
        );
        assert!(
            value
                .get("advice")
                .and_then(Value::as_str)
                .is_some_and(|v| v.contains("queue pressure is concentrating")),
            "{value}"
        );
        assert_eq!(
            value
                .get("next_action")
                .and_then(|v| v.get("kind"))
                .and_then(Value::as_str),
            Some("rerun")
        );
        let next_lines = exec_next_lines(None, Some(&wave)).join("\n");
        assert!(
            next_lines.contains(
                "task_execution_next_action_command: xshelf task run-all --dry-run --json"
            ),
            "{next_lines}"
        );
        let gate_lines = exec_gate_lines(None, Some(&wave)).join("\n");
        assert!(
            gate_lines.contains("task_execution_reasoning_gate_mode: cheap_structured_action"),
            "{gate_lines}"
        );
        let context_lines = exec_context_lines(None, Some(&wave)).join("\n");
        assert!(
            context_lines.contains(
                "task_execution_context_recommended_resume_point: xshelf task run-all --dry-run --json"
            ),
            "{context_lines}"
        );
        let wave_lines = exec_wave_lines(None, Some(&wave)).join("\n");
        assert!(
            wave_lines.contains("task_execution_wave_pressure: later_wave_queue"),
            "{wave_lines}"
        );
    }

    #[test]
    fn exec_context_cov() {
        let value = exec_context_value(
            Some(&serde_json::json!({
                "run_all_mode": "parallel",
                "run_all_recommended_resume_point": "xshelf scheduler --json --window 20",
                "run_all_failed": 1
            })),
            Some("xshelf scheduler --json --window 20"),
        );
        assert_eq!(
            value.get("last_mode_used").and_then(Value::as_str),
            Some("parallel")
        );
        assert_eq!(
            value
                .get("recommended_resume_point")
                .and_then(Value::as_str),
            Some("xshelf scheduler --json --window 20")
        );
        assert_eq!(
            value
                .get("resume_reuses_prior_action")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn exec_invariants_cov() {
        let value = exec_diag_value(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_scheduled": 4,
                "run_all_complete": 1,
                "run_all_failed": 1,
                "run_all_blocked": 0,
                "run_all_retryable_failures": 0,
                "run_all_non_retryable_failures": 0,
                "run_all_critical_errors": 0,
                "run_all_halted_remaining": 0,
                "run_all_worker_count": 2,
                "run_all_workers": "worker-1",
                "run_all_first_queue_started_at": "2026-04-22T17:00:10Z",
                "run_all_first_task_started_at": "2026-04-22T17:00:05Z",
                "run_all_last_task_finished_at": "2026-04-22T17:00:30Z",
                "halt_on_critical": false
            })),
            None,
        );
        assert_eq!(
            value
                .get("invariants")
                .and_then(|v| v.get("status"))
                .and_then(Value::as_str),
            Some("violated")
        );
        let issues = value
            .get("invariants")
            .and_then(|v| v.get("issues"))
            .and_then(Value::as_array)
            .expect("invariant issues");
        assert!(
            issues
                .iter()
                .filter_map(Value::as_str)
                .any(|v| v == "outcome_accounting_mismatch"),
            "{value}"
        );
        assert!(
            issues
                .iter()
                .filter_map(Value::as_str)
                .any(|v| v == "worker_summary_mismatch"),
            "{value}"
        );
        assert!(
            issues
                .iter()
                .filter_map(Value::as_str)
                .any(|v| v == "timing_window_mismatch"),
            "{value}"
        );
    }

    #[test]
    fn phase7_metrics_cov() {
        let current = serde_json::json!({
            "tool":"cxtask_runall",
            "run_all_failure_pattern":"retryable_failure",
            "run_all_recommended_resume_point":"xshelf task check --json",
            "run_all_mode":"mixed",
            "run_all_failed":1
        });
        let prior = serde_json::json!({
            "tool":"cxtask_runall",
            "run_all_failure_pattern":"retryable_failure",
            "run_all_recommended_resume_point":"xshelf task check --json",
            "run_all_mode":"mixed",
            "run_all_failed":1
        });
        let earlier = serde_json::json!({
            "tool":"cxtask_runall",
            "run_all_failure_pattern":"clean",
            "run_all_recommended_resume_point":"xshelf task run-all --status pending",
            "run_all_mode":"mixed",
            "run_all_failed":0
        });
        let mut recent = [current, prior, earlier];
        recent.reverse();
        let window_runs = recent.len() as u64;
        let expensive_action_rows = recent
            .iter()
            .filter(|summary| {
                summary
                    .get("run_all_recommended_resume_point")
                    .and_then(Value::as_str)
                    .is_some_and(|cmd| super::next_cost_class(cmd) == "expensive")
            })
            .count() as u64;
        let mut repeat_diagnosis_rows = 0u64;
        let mut resume_reuse_rows = 0u64;
        let mut structured_action_total = 0u64;
        let mut structured_action_success_rows = 0u64;
        let mut streak = 0u64;
        let mut actions_until_resolution = 0u64;
        for (idx, summary) in recent.iter().enumerate() {
            streak += 1;
            if idx > 0 {
                let prior = &recent[idx - 1];
                let current_pattern = summary
                    .get("run_all_failure_pattern")
                    .and_then(Value::as_str)
                    .unwrap_or("clean");
                let prior_pattern = prior
                    .get("run_all_failure_pattern")
                    .and_then(Value::as_str)
                    .unwrap_or("clean");
                if current_pattern != "clean" && current_pattern == prior_pattern {
                    repeat_diagnosis_rows += 1;
                }
                let current_resume = summary
                    .get("run_all_recommended_resume_point")
                    .and_then(Value::as_str);
                let prior_resume = prior
                    .get("run_all_recommended_resume_point")
                    .and_then(Value::as_str);
                if current_resume.is_some() && current_resume == prior_resume {
                    resume_reuse_rows += 1;
                }
                if let Some(prior_resume_cmd) = prior_resume
                    && super::next_cost_class(prior_resume_cmd) == "cheap"
                    && super::next_reasoning_required(prior_resume_cmd) == "none"
                {
                    structured_action_total += 1;
                    if !super::summary_failed(summary) {
                        structured_action_success_rows += 1;
                    }
                }
            }
            if !super::summary_failed(summary) {
                actions_until_resolution = streak;
                streak = 0;
            }
        }
        if streak > 0 {
            actions_until_resolution = streak;
        }
        let transition_count = window_runs.saturating_sub(1);
        let value = serde_json::json!({
            "window_runs": window_runs,
            "actions_until_resolution": actions_until_resolution,
            "expensive_action_rate": expensive_action_rows as f64 / window_runs as f64,
            "repeat_diagnosis_rate": repeat_diagnosis_rows as f64 / transition_count as f64,
            "resume_reuse_rate": resume_reuse_rows as f64 / transition_count as f64,
            "structured_action_success_rate": structured_action_success_rows as f64 / structured_action_total as f64
        });
        assert_eq!(value.get("window_runs").and_then(Value::as_u64), Some(3));
        assert_eq!(
            value
                .get("actions_until_resolution")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            value.get("repeat_diagnosis_rate").and_then(Value::as_f64),
            Some(0.5)
        );
        assert_eq!(
            value.get("resume_reuse_rate").and_then(Value::as_f64),
            Some(0.5)
        );
    }

    #[test]
    fn p7_lines_cov() {
        let lines = phase7_metric_lines(20).join("\n");
        assert!(lines.contains("phase7_window_runs:"), "{lines}");
        assert!(lines.contains("phase7_resume_reuse_rate:"), "{lines}");
    }

    #[test]
    fn adapter_policy_cov() {
        let lines = super::adapter_policy_lines().join("\n");
        assert!(
            lines.contains("adapter_rollout_default_transport: process"),
            "{lines}"
        );
        assert!(
            lines.contains("adapter_rollout_http_opt_in: true"),
            "{lines}"
        );
        assert!(
            lines.contains("adapter_rollout_default_switch_guard: two_green_ci_windows"),
            "{lines}"
        );
    }
}

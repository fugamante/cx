use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

use crate::config::{app_config, command_matches_cli};
use crate::contract_versions::{
    ACTIONS_JSON_CONTRACT_VERSION, DIAG_JSON_CONTRACT_VERSION, SCHEDULER_JSON_CONTRACT_VERSION,
};
use crate::doctor::{
    adapter_policy_lines, exec_action_value, exec_diag_value, latest_run_all_sum, latest_wave_sum,
    phase7_metric_lines, phase7_metrics_value,
};
use crate::execmeta::{toolchain_version_string, utc_now_iso};
use crate::json_mode::resolve_json_mode;
use crate::logs::file_len;
use crate::logs::load_values;
use crate::paths::{repo_root_hint, resolve_log_file};
use crate::provider_adapter::{
    adapter_policy_value, runtime_caps_json, selected_runtime_caps, selected_tq_caps,
};
use crate::routing::{bash_type_of_function, route_handler_for};
use crate::runtime::{llm_backend, llm_model};
use crate::task_cmds::task_readiness_value;
use crate::tasks::read_tasks;

fn resolved_provider(cfg_provider: &str) -> &'static str {
    let _ = cfg_provider;
    "native"
}

fn schema_count(schema_dir: &Path) -> usize {
    if !schema_dir.is_dir() {
        return 0;
    }
    fs::read_dir(schema_dir)
        .ok()
        .map(|iter| {
            iter.filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .count()
        })
        .unwrap_or(0)
}

fn last_run_id() -> String {
    resolve_log_file()
        .and_then(|p| {
            let len = file_len(&p);
            last_appended_json_value(&p, len.saturating_sub(8192))
        })
        .and_then(|v| {
            v.get("execution_id")
                .and_then(Value::as_str)
                .map(|s| s.to_string())
                .or_else(|| {
                    v.get("prompt_sha256")
                        .and_then(Value::as_str)
                        .map(|s| s.to_string())
                })
        })
        .unwrap_or_else(|| "<none>".to_string())
}

fn percentile_u64(values: &[u64], pct: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let p = pct.clamp(0.0, 1.0);
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted.get(idx).copied()
}

fn print_scheduler_diag(log_file_path: &str, window: usize) {
    let scheduler = scheduler_diag_value(log_file_path, window);
    let workers_seen = scheduler
        .get("workers_seen")
        .and_then(Value::as_array)
        .map(|arr| {
            let mut out: Vec<String> = arr
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect();
            out.sort();
            if out.is_empty() {
                "<none>".to_string()
            } else {
                out.join(",")
            }
        })
        .unwrap_or_else(|| "<none>".to_string());
    let worker_distribution = scheduler
        .get("worker_distribution")
        .and_then(Value::as_object)
        .map(|m| {
            if m.is_empty() {
                "<none>".to_string()
            } else {
                m.iter()
                    .map(|(k, v)| format!("{}={}", k, v.as_u64().unwrap_or(0)))
                    .collect::<Vec<String>>()
                    .join(",")
            }
        })
        .unwrap_or_else(|| "<none>".to_string());
    let backend_distribution = scheduler
        .get("backend_distribution")
        .and_then(Value::as_object)
        .map(|m| {
            if m.is_empty() {
                "<none>".to_string()
            } else {
                m.iter()
                    .map(|(k, v)| format!("{}={}", k, v.as_u64().unwrap_or(0)))
                    .collect::<Vec<String>>()
                    .join(",")
            }
        })
        .unwrap_or_else(|| "<none>".to_string());

    println!(
        "scheduler_window_runs: {}",
        scheduler
            .get("window_runs")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "scheduler_queue_rows: {}",
        scheduler
            .get("queue_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "scheduler_queue_ms_avg: {}",
        scheduler
            .get("queue_ms_avg")
            .and_then(Value::as_u64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "scheduler_queue_ms_p95: {}",
        scheduler
            .get("queue_ms_p95")
            .and_then(Value::as_u64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "scheduler_rows_with_retry_attempt: {}",
        scheduler
            .get("rows_with_retry_attempt")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "scheduler_rows_with_queue_started_at: {}",
        scheduler
            .get("rows_with_queue_started_at")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "scheduler_rows_with_task_started_at: {}",
        scheduler
            .get("rows_with_task_started_at")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "scheduler_rows_with_task_finished_at: {}",
        scheduler
            .get("rows_with_task_finished_at")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!("scheduler_workers_seen: {workers_seen}");
    println!("scheduler_worker_distribution: {worker_distribution}");
    println!("scheduler_backend_distribution: {backend_distribution}");
}

fn readiness_diag_value() -> Value {
    match read_tasks() {
        Ok(tasks) => task_readiness_value(&tasks, "pending"),
        Err(e) => serde_json::json!({
            "status_filter": "pending",
            "selected": 0,
            "waves": 0,
            "blocked_total": 0,
            "blocked_dependencies": 0,
            "blocked_resources": 0,
            "can_run": false,
            "can_run_mixed": false,
            "can_run_parallel": false,
            "strict_plan_ok": false,
            "strict_plan_reason": format!("task_read_failed: {e}"),
            "sequential_waves": 0,
            "parallel_waves": 0,
            "largest_parallel_wave": 0,
            "recommended_mode": "sequential",
            "recommended_reason": "task_read_failed",
            "blocked": []
        }),
    }
}

fn print_readiness_diag(task_readiness: &Value) {
    println!(
        "task_readiness_selected: {}",
        task_readiness
            .get("selected")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "task_readiness_can_run_mixed: {}",
        task_readiness
            .get("can_run_mixed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    );
    println!(
        "task_readiness_can_run_parallel: {}",
        task_readiness
            .get("can_run_parallel")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    );
    println!(
        "task_readiness_recommended_mode: {}",
        task_readiness
            .get("recommended_mode")
            .and_then(Value::as_str)
            .unwrap_or("sequential")
    );
    println!(
        "task_readiness_parallel_waves: {}",
        task_readiness
            .get("parallel_waves")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "task_readiness_largest_parallel_wave: {}",
        task_readiness
            .get("largest_parallel_wave")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
}

fn retry_diag_value(log_file_path: &str, window: usize) -> Value {
    let path = Path::new(log_file_path);
    if !path.exists() {
        return serde_json::json!({
            "window_runs": 0,
            "rows_with_retry_metadata": 0,
            "rows_after_retry": 0,
            "rows_after_retry_success": 0,
            "rows_after_retry_success_rate": 0.0,
            "tasks_with_retry": 0,
            "tasks_retry_recovered": 0,
            "tasks_retry_recovery_rate": 0.0,
            "attempt_histogram": {}
        });
    }
    let rows = load_values(path, window).unwrap_or_default();
    let mut attempt_histogram: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows_with_retry_metadata = 0usize;
    let mut rows_after_retry = 0usize;
    let mut rows_after_retry_success = 0usize;
    let mut task_timeout_seen: BTreeMap<String, bool> = BTreeMap::new();
    let mut task_recovered: BTreeMap<String, bool> = BTreeMap::new();

    for v in &rows {
        let attempt = v.get("retry_attempt").and_then(Value::as_u64);
        if let Some(a) = attempt {
            rows_with_retry_metadata += 1;
            *attempt_histogram.entry(a.to_string()).or_insert(0) += 1;
            if a > 1 {
                rows_after_retry += 1;
                let timed_out = v.get("timed_out").and_then(Value::as_bool) == Some(true);
                let schema_valid = v.get("schema_valid").and_then(Value::as_bool) != Some(false);
                let policy_blocked = v.get("policy_blocked").and_then(Value::as_bool) == Some(true);
                if !timed_out && schema_valid && !policy_blocked {
                    rows_after_retry_success += 1;
                }
            }
        }
        let task_id = v
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        if let Some(tid) = task_id {
            if attempt.is_some() {
                task_timeout_seen.entry(tid.clone()).or_insert(false);
                task_recovered.entry(tid.clone()).or_insert(false);
            }
            if v.get("timed_out").and_then(Value::as_bool) == Some(true) {
                task_timeout_seen.insert(tid, true);
            } else if attempt.unwrap_or(0) > 1 {
                task_recovered.insert(tid, true);
            }
        }
    }

    let rows_after_retry_success_rate = if rows_after_retry == 0 {
        0.0
    } else {
        rows_after_retry_success as f64 / rows_after_retry as f64
    };
    let tasks_with_retry = task_timeout_seen.iter().filter(|(_, saw)| **saw).count();
    let tasks_retry_recovered = task_timeout_seen
        .iter()
        .filter(|(tid, saw)| **saw && task_recovered.get(*tid) == Some(&true))
        .count();
    let tasks_retry_recovery_rate = if tasks_with_retry == 0 {
        0.0
    } else {
        tasks_retry_recovered as f64 / tasks_with_retry as f64
    };

    serde_json::json!({
        "window_runs": rows.len(),
        "rows_with_retry_metadata": rows_with_retry_metadata,
        "rows_after_retry": rows_after_retry,
        "rows_after_retry_success": rows_after_retry_success,
        "rows_after_retry_success_rate": rows_after_retry_success_rate,
        "tasks_with_retry": tasks_with_retry,
        "tasks_retry_recovered": tasks_retry_recovered,
        "tasks_retry_recovery_rate": tasks_retry_recovery_rate,
        "attempt_histogram": attempt_histogram
    })
}

fn print_retry_diag(log_file_path: &str, window: usize) {
    let retry = retry_diag_value(log_file_path, window);
    let hist = retry
        .get("attempt_histogram")
        .and_then(Value::as_object)
        .map(|m| {
            if m.is_empty() {
                "<none>".to_string()
            } else {
                m.iter()
                    .map(|(k, v)| format!("{k}={}", v.as_u64().unwrap_or(0)))
                    .collect::<Vec<String>>()
                    .join(",")
            }
        })
        .unwrap_or_else(|| "<none>".to_string());
    println!(
        "retry_rows_with_metadata: {}",
        retry
            .get("rows_with_retry_metadata")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "retry_rows_after_retry: {}",
        retry
            .get("rows_after_retry")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "retry_rows_after_retry_success: {}",
        retry
            .get("rows_after_retry_success")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "retry_rows_after_retry_success_rate: {:.2}",
        retry
            .get("rows_after_retry_success_rate")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    );
    println!(
        "retry_tasks_with_retry: {}",
        retry
            .get("tasks_with_retry")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "retry_tasks_recovered: {}",
        retry
            .get("tasks_retry_recovered")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "retry_tasks_recovery_rate: {:.2}",
        retry
            .get("tasks_retry_recovery_rate")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    );
    println!("retry_attempt_histogram: {hist}");
}

fn capture_prompt_diag(log_file_path: &str, window: usize) -> Value {
    let defaults = || {
        serde_json::json!({
            "window_runs": 0,
            "rows_with_explicit_profile": 0,
            "shadow_narrow_configured_runs": 0,
            "shadow_narrow_applied_runs": 0,
            "shadow_narrow_fallback_runs": 0,
            "latest_execution_id": null,
            "latest_timestamp": null,
            "latest_command": null,
            "latest_profile": null,
            "latest_profile_applied": null,
            "latest_reducer_kind": null,
            "latest_fallback_reason": null,
            "recommendation": "No explicit capture prompt profile runs observed in the current diagnostic window."
        })
    };

    let path = Path::new(log_file_path);
    if !path.exists() {
        return defaults();
    }

    let rows = load_values(path, window).unwrap_or_default();
    let mut rows_with_explicit_profile = 0usize;
    let mut shadow_narrow_configured_runs = 0usize;
    let mut shadow_narrow_applied_runs = 0usize;
    let mut shadow_narrow_fallback_runs = 0usize;
    let mut latest_explicit_profile: Option<&Value> = None;

    for row in &rows {
        let profile = row
            .get("capture_prompt_profile")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(profile) = profile else {
            continue;
        };
        rows_with_explicit_profile += 1;
        latest_explicit_profile = Some(row);
        if profile == "shadow_narrow" {
            shadow_narrow_configured_runs += 1;
            if row
                .get("capture_prompt_profile_applied")
                .and_then(Value::as_bool)
                == Some(true)
            {
                shadow_narrow_applied_runs += 1;
            } else {
                shadow_narrow_fallback_runs += 1;
            }
        }
    }

    let latest_execution_id = latest_explicit_profile
        .and_then(|row| row.get("execution_id"))
        .cloned()
        .unwrap_or(Value::Null);
    let latest_timestamp = latest_explicit_profile
        .and_then(|row| row.get("timestamp").or_else(|| row.get("ts")))
        .cloned()
        .unwrap_or(Value::Null);
    let latest_command = latest_explicit_profile
        .and_then(|row| row.get("command").or_else(|| row.get("tool")))
        .cloned()
        .unwrap_or(Value::Null);
    let latest_profile = latest_explicit_profile
        .and_then(|row| row.get("capture_prompt_profile"))
        .cloned()
        .unwrap_or(Value::Null);
    let latest_profile_applied = latest_explicit_profile
        .and_then(|row| row.get("capture_prompt_profile_applied"))
        .cloned()
        .unwrap_or(Value::Null);
    let latest_reducer_kind = latest_explicit_profile
        .and_then(|row| row.get("capture_prompt_reducer_kind"))
        .cloned()
        .unwrap_or(Value::Null);
    let latest_fallback_reason = latest_explicit_profile
        .and_then(|row| row.get("capture_prompt_fallback_reason"))
        .cloned()
        .unwrap_or(Value::Null);
    let recommendation = if let Some(row) = latest_explicit_profile {
        let profile = row
            .get("capture_prompt_profile")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let applied = row
            .get("capture_prompt_profile_applied")
            .and_then(Value::as_bool);
        let fallback_reason = row
            .get("capture_prompt_fallback_reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if profile == "shadow_narrow" && applied == Some(true) {
            "Latest explicit shadow_narrow run applied cleanly; use reducer kind coverage to decide whether more fixture-backed reducers are ready.".to_string()
        } else if profile == "shadow_narrow" {
            format!(
                "Latest explicit shadow_narrow run fell back to legacy reduction{}; inspect telemetry before widening rollout.",
                fallback_reason
                    .map(|reason| format!(" ({reason})"))
                    .unwrap_or_default()
            )
        } else {
            format!(
                "Latest explicit capture prompt profile '{profile}' was observed; verify reducer eligibility and fallback handling before broadening rollout."
            )
        }
    } else {
        "No explicit capture prompt profile runs observed in the current diagnostic window."
            .to_string()
    };

    serde_json::json!({
        "window_runs": rows.len(),
        "rows_with_explicit_profile": rows_with_explicit_profile,
        "shadow_narrow_configured_runs": shadow_narrow_configured_runs,
        "shadow_narrow_applied_runs": shadow_narrow_applied_runs,
        "shadow_narrow_fallback_runs": shadow_narrow_fallback_runs,
        "latest_execution_id": latest_execution_id,
        "latest_timestamp": latest_timestamp,
        "latest_command": latest_command,
        "latest_profile": latest_profile,
        "latest_profile_applied": latest_profile_applied,
        "latest_reducer_kind": latest_reducer_kind,
        "latest_fallback_reason": latest_fallback_reason,
        "recommendation": recommendation
    })
}

fn print_prompt_diag(log_file_path: &str, window: usize) {
    let capture_prompt = capture_prompt_diag(log_file_path, window);
    println!(
        "capture_prompt_rows_with_explicit_profile: {}",
        capture_prompt
            .get("rows_with_explicit_profile")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "capture_prompt_shadow_narrow_configured_runs: {}",
        capture_prompt
            .get("shadow_narrow_configured_runs")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "capture_prompt_shadow_narrow_applied_runs: {}",
        capture_prompt
            .get("shadow_narrow_applied_runs")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "capture_prompt_shadow_narrow_fallback_runs: {}",
        capture_prompt
            .get("shadow_narrow_fallback_runs")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "capture_prompt_latest_profile: {}",
        capture_prompt
            .get("latest_profile")
            .and_then(Value::as_str)
            .unwrap_or("<none>")
    );
    println!(
        "capture_prompt_latest_fallback_reason: {}",
        capture_prompt
            .get("latest_fallback_reason")
            .and_then(Value::as_str)
            .unwrap_or("<none>")
    );
    println!(
        "capture_prompt_recommendation: {}",
        capture_prompt
            .get("recommendation")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
}

fn critical_diag_value(log_file_path: &str, window: usize) -> Value {
    let path = Path::new(log_file_path);
    if !path.exists() {
        return serde_json::json!({
            "window_runs": 0,
            "summary_rows": 0,
            "halt_enabled_rows": 0,
            "halted_rows": 0,
            "critical_errors_total": 0,
            "runs_with_critical_errors": 0
        });
    }
    let rows = load_values(path, window).unwrap_or_default();
    let mut summary_rows = 0u64;
    let mut halt_enabled_rows = 0u64;
    let mut halted_rows = 0u64;
    let mut critical_errors_total = 0u64;
    let mut runs_with_critical_errors = 0u64;
    for v in &rows {
        if v.get("tool").and_then(Value::as_str) != Some("cxtask_runall") {
            continue;
        }
        summary_rows += 1;
        if v.get("halt_on_critical").and_then(Value::as_bool) == Some(true) {
            halt_enabled_rows += 1;
        }
        let critical = v
            .get("run_all_critical_errors")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        critical_errors_total += critical;
        if critical > 0 {
            runs_with_critical_errors += 1;
        }
        let halted = v
            .get("run_all_scheduled")
            .and_then(Value::as_u64)
            .zip(v.get("run_all_complete").and_then(Value::as_u64))
            .zip(v.get("run_all_failed").and_then(Value::as_u64))
            .map(|((sched, ok), failed)| ok + failed < sched)
            .unwrap_or(false);
        if halted {
            halted_rows += 1;
        }
    }
    serde_json::json!({
        "window_runs": rows.len(),
        "summary_rows": summary_rows,
        "halt_enabled_rows": halt_enabled_rows,
        "halted_rows": halted_rows,
        "critical_errors_total": critical_errors_total,
        "runs_with_critical_errors": runs_with_critical_errors
    })
}

fn print_critical_diag(log_file_path: &str, window: usize) {
    let critical = critical_diag_value(log_file_path, window);
    println!(
        "critical_summary_rows: {}",
        critical
            .get("summary_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "critical_halt_enabled_rows: {}",
        critical
            .get("halt_enabled_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "critical_halted_rows: {}",
        critical
            .get("halted_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "critical_errors_total: {}",
        critical
            .get("critical_errors_total")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "critical_runs_with_errors: {}",
        critical
            .get("runs_with_critical_errors")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
}

fn print_diag_header(app_version: &str, cfg: &crate::config::AppConfig) {
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    let experiment_caps = selected_tq_caps();
    let runtime_caps = selected_runtime_caps();
    println!("== cxdiag ==");
    println!("timestamp: {}", utc_now_iso());
    println!("version: {}", toolchain_version_string(app_version));
    println!("mode: {}", cfg.cx_mode);
    println!("backend: {backend}");
    println!("active_model: {active_model}");
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
}

fn scheduler_diag_value(log_file_path: &str, window: usize) -> Value {
    let path = Path::new(log_file_path);
    if !path.exists() {
        return serde_json::json!({
            "window_runs": 0,
            "queue_rows": 0,
            "queue_ms_avg": Value::Null,
            "queue_ms_p95": Value::Null,
            "rows_with_retry_attempt": 0,
            "rows_with_queue_started_at": 0,
            "rows_with_task_started_at": 0,
            "rows_with_task_finished_at": 0,
            "workers_seen": [],
            "worker_distribution": {},
            "backend_distribution": {}
        });
    }
    let rows = load_values(path, window).unwrap_or_default();
    let mut queue_vals: Vec<u64> = Vec::new();
    let mut worker_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut backend_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows_with_retry_attempt = 0u64;
    let mut rows_with_queue_started_at = 0u64;
    let mut rows_with_task_started_at = 0u64;
    let mut rows_with_task_finished_at = 0u64;

    for v in &rows {
        if let Some(q) = v.get("queue_ms").and_then(Value::as_u64) {
            queue_vals.push(q);
        }
        if v.get("retry_attempt").and_then(Value::as_u64).is_some() {
            rows_with_retry_attempt += 1;
        }
        if v.get("queue_started_at")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|s| !s.is_empty())
        {
            rows_with_queue_started_at += 1;
        }
        if v.get("task_started_at")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|s| !s.is_empty())
        {
            rows_with_task_started_at += 1;
        }
        if v.get("task_finished_at")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|s| !s.is_empty())
        {
            rows_with_task_finished_at += 1;
        }
        if let Some(w) = v.get("worker_id").and_then(Value::as_str) {
            *worker_counts.entry(w.to_string()).or_insert(0) += 1;
        }
        if let Some(b) = v
            .get("backend_selected")
            .and_then(Value::as_str)
            .or_else(|| v.get("backend_used").and_then(Value::as_str))
        {
            *backend_counts.entry(b.to_string()).or_insert(0) += 1;
        }
    }
    let queue_avg = if queue_vals.is_empty() {
        None
    } else {
        Some(queue_vals.iter().sum::<u64>() / queue_vals.len() as u64)
    };
    let queue_p95 = percentile_u64(&queue_vals, 0.95);
    serde_json::json!({
        "window_runs": rows.len(),
        "queue_rows": queue_vals.len(),
        "queue_ms_avg": queue_avg,
        "queue_ms_p95": queue_p95,
        "rows_with_retry_attempt": rows_with_retry_attempt,
        "rows_with_queue_started_at": rows_with_queue_started_at,
        "rows_with_task_started_at": rows_with_task_started_at,
        "rows_with_task_finished_at": rows_with_task_finished_at,
        "workers_seen": worker_counts.keys().cloned().collect::<Vec<String>>(),
        "worker_distribution": worker_counts,
        "backend_distribution": backend_counts
    })
}

fn concurrency_diag_value(
    log_file_path: &str,
    window: usize,
    cfg: &crate::config::AppConfig,
) -> Value {
    let default_backend = cfg.llm_backend.to_lowercase();
    let default_backend_pool = if matches!(
        default_backend.as_str(),
        "primary" | "ollama" | "llamacpp" | "mlx"
    ) {
        vec![default_backend]
    } else {
        vec!["primary".to_string()]
    };
    let defaults = serde_json::json!({
        "run_all_mode": "sequential",
        "backend_pool": default_backend_pool,
        "backend_caps": {},
        "max_workers": 1,
        "fairness": "round_robin",
        "halt_on_critical": cfg.task_halt_on_critical
    });

    let path = Path::new(log_file_path);
    if !path.exists() {
        return serde_json::json!({
            "defaults": defaults,
            "observed": {
                "window_runs": 0,
                "run_all_rows": 0,
                "latest_run_all_mode": Value::Null,
                "run_all_mode_counts": {},
                "halt_on_critical_rows": 0,
                "halted_remaining_total": 0,
                "latest_halted_remaining": 0,
                "backend_fallback_rows": 0,
                "latest_backend_fallbacks": Value::Null,
                "latest_worker_count": 0,
                "latest_workers": Value::Null,
                "latest_max_retry_attempt": 0,
                "latest_first_queue_started_at": Value::Null,
                "latest_first_task_started_at": Value::Null,
                "latest_last_task_finished_at": Value::Null,
                "wave_task_rows": 0,
                "latest_wave_index": Value::Null,
                "latest_wave_mode": Value::Null,
                "latest_wave_size": 0,
                "largest_wave_index": Value::Null,
                "largest_wave_size": 0,
                "max_queue_wave_index": Value::Null,
                "max_queue_wave_ms": 0
            }
        });
    }

    let rows = load_values(path, window).unwrap_or_default();
    let mut run_all_rows = 0u64;
    let mut mode_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut halt_rows = 0u64;
    let mut latest_run_all_mode: Option<String> = None;
    let mut halted_remaining_total = 0u64;
    let mut latest_halted_remaining = 0u64;
    let mut backend_fallback_rows = 0u64;
    let mut latest_backend_fallbacks: Option<String> = None;
    let mut latest_worker_count = 0u64;
    let mut latest_workers: Option<String> = None;
    let mut latest_max_retry_attempt = 0u64;
    let mut latest_first_queue_started_at: Option<String> = None;
    let mut latest_first_task_started_at: Option<String> = None;
    let mut latest_last_task_finished_at: Option<String> = None;
    let mut wave_task_rows = 0u64;
    let mut latest_wave_index: Option<u64> = None;
    let mut latest_wave_mode: Option<String> = None;
    let mut latest_wave_size = 0u64;
    let mut largest_wave_index: Option<u64> = None;
    let mut largest_wave_size = 0u64;
    let mut max_queue_wave_index: Option<u64> = None;
    let mut max_queue_wave_ms = 0u64;
    for v in &rows {
        if let Some(wave_index) = v.get("wave_index").and_then(Value::as_u64) {
            let has_task = v
                .get("task_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .is_some_and(|task| !task.is_empty());
            if has_task {
                wave_task_rows += 1;
            }
            latest_wave_index = Some(wave_index);
            latest_wave_mode = v
                .get("wave_mode")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            latest_wave_size = v.get("wave_size").and_then(Value::as_u64).unwrap_or(0);
            if latest_wave_size >= largest_wave_size {
                largest_wave_size = latest_wave_size;
                largest_wave_index = Some(wave_index);
            }
            let queue_ms = v.get("queue_ms").and_then(Value::as_u64).unwrap_or(0);
            if queue_ms >= max_queue_wave_ms {
                max_queue_wave_ms = queue_ms;
                max_queue_wave_index = Some(wave_index);
            }
        }
        if v.get("tool").and_then(Value::as_str) != Some("cxtask_runall") {
            continue;
        }
        run_all_rows += 1;
        if let Some(mode) = v.get("run_all_mode").and_then(Value::as_str) {
            *mode_counts.entry(mode.to_string()).or_insert(0) += 1;
            latest_run_all_mode = Some(mode.to_string());
        }
        if v.get("halt_on_critical").and_then(Value::as_bool) == Some(true) {
            halt_rows += 1;
        }
        let halted_remaining = v
            .get("run_all_halted_remaining")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        halted_remaining_total += halted_remaining;
        latest_halted_remaining = halted_remaining;
        let fallback_rows = v
            .get("run_all_backend_fallback_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        backend_fallback_rows += fallback_rows;
        latest_worker_count = v
            .get("run_all_worker_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        latest_workers = v
            .get("run_all_workers")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        latest_max_retry_attempt = v
            .get("run_all_max_retry_attempt")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        latest_first_queue_started_at = v
            .get("run_all_first_queue_started_at")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        latest_first_task_started_at = v
            .get("run_all_first_task_started_at")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        latest_last_task_finished_at = v
            .get("run_all_last_task_finished_at")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        latest_backend_fallbacks = v
            .get("run_all_backend_fallbacks")
            .and_then(Value::as_str)
            .map(ToString::to_string);
    }

    serde_json::json!({
        "defaults": defaults,
        "observed": {
            "window_runs": rows.len(),
            "run_all_rows": run_all_rows,
            "latest_run_all_mode": latest_run_all_mode,
            "run_all_mode_counts": mode_counts,
            "halt_on_critical_rows": halt_rows,
            "halted_remaining_total": halted_remaining_total,
            "latest_halted_remaining": latest_halted_remaining,
            "backend_fallback_rows": backend_fallback_rows,
            "latest_backend_fallbacks": latest_backend_fallbacks,
            "latest_worker_count": latest_worker_count,
            "latest_workers": latest_workers,
            "latest_max_retry_attempt": latest_max_retry_attempt,
            "latest_first_queue_started_at": latest_first_queue_started_at,
            "latest_first_task_started_at": latest_first_task_started_at,
            "latest_last_task_finished_at": latest_last_task_finished_at,
            "wave_task_rows": wave_task_rows,
            "latest_wave_index": latest_wave_index,
            "latest_wave_mode": latest_wave_mode,
            "latest_wave_size": latest_wave_size,
            "largest_wave_index": largest_wave_index,
            "largest_wave_size": largest_wave_size,
            "max_queue_wave_index": max_queue_wave_index,
            "max_queue_wave_ms": max_queue_wave_ms
        }
    })
}

fn parse_severity_floor(raw: &str) -> Option<&'static str> {
    match raw {
        "warn" | "warning" => Some("warning"),
        "critical" => Some("critical"),
        _ => None,
    }
}

fn severity_rank(level: &str) -> i32 {
    match level {
        "critical" => 2,
        "warning" => 1,
        _ => 0,
    }
}

pub(crate) fn action_cost_rank(command: &str) -> u8 {
    if command_matches_cli(command, "task run-all --status pending")
        || command_matches_cli(command, "task run-all --mode ")
        || command_matches_cli(command, "task check")
    {
        0
    } else if command_matches_cli(command, "scheduler")
        || command_matches_cli(command, "diag")
        || command_matches_cli(command, "doctor")
        || command_matches_cli(command, "task run-plan")
    {
        1
    } else {
        2
    }
}

pub(crate) fn sort_actions_by_phase7(actions: &mut [serde_json::Value]) {
    actions.sort_by(|a, b| {
        let a_sev = a.get("severity").and_then(Value::as_str).unwrap_or("ok");
        let b_sev = b.get("severity").and_then(Value::as_str).unwrap_or("ok");
        severity_rank(b_sev)
            .cmp(&severity_rank(a_sev))
            .then_with(|| {
                let a_cmd = a.get("command").and_then(Value::as_str).unwrap_or_default();
                let b_cmd = b.get("command").and_then(Value::as_str).unwrap_or_default();
                action_cost_rank(a_cmd).cmp(&action_cost_rank(b_cmd))
            })
            .then_with(|| {
                let a_cmd = a.get("command").and_then(Value::as_str).unwrap_or_default();
                let b_cmd = b.get("command").and_then(Value::as_str).unwrap_or_default();
                a_cmd.cmp(b_cmd)
            })
    });
}

fn parse_diag_args(args: &[String]) -> Result<(bool, usize, bool, bool, Option<String>), String> {
    let mut as_json: Option<bool> = None;
    let mut window = 200usize;
    let mut strict = false;
    let mut actions = false;
    let mut severity_floor: Option<String> = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                as_json = Some(true);
                i += 1;
            }
            "--text" => {
                as_json = Some(false);
                i += 1;
            }
            "--strict" => {
                strict = true;
                i += 1;
            }
            "--actions" => {
                actions = true;
                i += 1;
            }
            "--severity" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    return Err("diag: --severity requires a value".to_string());
                };
                let Some(normalized) = parse_severity_floor(v) else {
                    return Err("diag: --severity must be warning|critical".to_string());
                };
                severity_floor = Some(normalized.to_string());
                i += 2;
            }
            "--window" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    return Err("diag: --window requires a value".to_string());
                };
                let parsed = v
                    .parse::<usize>()
                    .map_err(|_| "diag: --window must be an integer".to_string())?;
                if parsed == 0 {
                    return Err("diag: --window must be >= 1".to_string());
                }
                window = parsed;
                i += 2;
            }
            other => {
                return Err(format!("diag: unknown flag '{other}'"));
            }
        }
    }
    Ok((
        resolve_json_mode(as_json, false),
        window,
        strict,
        actions,
        severity_floor,
    ))
}

fn build_actions_from_reasons(
    reasons: &[String],
    window: usize,
    route_cmd: &str,
) -> Vec<serde_json::Value> {
    let mut actions: Vec<serde_json::Value> = Vec::new();
    for reason in reasons {
        let reason_key = reason.split(':').next().unwrap_or(reason.as_str());
        let (id, severity, rationale, command) = match reason_key {
            "queue_p95_high" => (
                "queue_p95_high",
                "warning",
                "Scheduler queue p95 latency is elevated.",
                format!(
                    "{} --window {window} --strict",
                    crate::config::command_with_cli("scheduler --json")
                ),
            ),
            "backend_skew_high" => (
                "backend_skew_high",
                "warning",
                "Runs are concentrated on one backend; rebalance or benchmark policy.",
                format!(
                    "{} --window {window} --json",
                    crate::config::command_with_cli("broker benchmark")
                ),
            ),
            "worker_spread_low" => (
                "worker_spread_low",
                "warning",
                "Worker distribution is narrow for current run volume.",
                crate::config::command_with_cli("task run-plan --status pending --json"),
            ),
            "retry_recovery_low" => (
                "retry_recovery_low",
                "critical",
                "Retry recovery rate is below target.",
                crate::config::command_with_cli(
                    "logs stats 200 --json --strict --severity critical",
                ),
            ),
            "retry_pressure_high" => (
                "retry_pressure_high",
                "warning",
                "Retry attempt volume is elevated.",
                crate::config::command_with_cli("optimize 200 --json --actions"),
            ),
            "timing_coverage_low" => (
                "timing_coverage_low",
                "warning",
                "Task timing attribution coverage is degraded; queue/start/finish metadata is incomplete.",
                crate::config::command_with_cli(
                    "logs stats 200 --json --strict --severity warning",
                ),
            ),
            "critical_halts_detected" => (
                "critical_halts_detected",
                "critical",
                "Task run-all critical halts were observed.",
                route_cmd.to_string(),
            ),
            other => (
                other,
                "warning",
                "Scheduler diagnostic anomaly detected.",
                crate::config::command_with_cli("diag --json --window 200 --actions"),
            ),
        };
        actions.push(serde_json::json!({
            "id": id,
            "severity": severity,
            "rationale": rationale,
            "command": command
        }));
    }
    sort_actions_by_phase7(&mut actions);
    actions
}

fn merge_exec_action(
    mut actions: Vec<serde_json::Value>,
    task_execution: &Value,
) -> Vec<serde_json::Value> {
    let Some(exec_action) = exec_action_value(task_execution) else {
        return actions;
    };
    let exec_command = exec_action
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let exec_id = exec_action.get("id").and_then(Value::as_str);
    let exec_rationale = exec_action
        .get("rationale")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Some(duplicate) = actions.iter_mut().find(|action| {
        action.get("command").and_then(Value::as_str) == Some(exec_command)
            || action.get("id").and_then(Value::as_str) == exec_id
    }) {
        if exec_rationale.contains("Phase VII bias:")
            && let Some(existing) = duplicate.get("rationale").and_then(Value::as_str)
            && !existing.contains("Phase VII bias:")
        {
            duplicate["rationale"] =
                serde_json::Value::String(format!("{existing} {exec_rationale}"));
        }
        return actions;
    }
    actions.insert(0, exec_action);
    actions
}

fn merge_prompt_action(
    mut actions: Vec<serde_json::Value>,
    capture_prompt: &Value,
) -> Vec<serde_json::Value> {
    let latest_profile = capture_prompt
        .get("latest_profile")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let latest_applied = capture_prompt
        .get("latest_profile_applied")
        .and_then(Value::as_bool);
    if latest_profile != "shadow_narrow" || latest_applied == Some(true) {
        return actions;
    }

    let fallback_reason = capture_prompt
        .get("latest_fallback_reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let rationale = if let Some(reason) = fallback_reason {
        format!(
            "Latest explicit shadow_narrow run fell back to legacy reduction ({reason}); inspect capture prompt telemetry before widening rollout."
        )
    } else {
        "Latest explicit shadow_narrow run never applied typed shadow assembly; inspect capture prompt telemetry before widening rollout.".to_string()
    };
    let action = serde_json::json!({
        "id": "capture_prompt_rollout_followup",
        "severity": "warning",
        "rationale": rationale,
        "command": crate::config::command_with_cli("telemetry 200 --json")
    });
    let action_command = action
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let action_id = action.get("id").and_then(Value::as_str);
    if actions.iter().any(|existing| {
        existing.get("command").and_then(Value::as_str) == Some(action_command)
            || existing.get("id").and_then(Value::as_str) == action_id
    }) {
        return actions;
    }
    actions.push(action);
    sort_actions_by_phase7(&mut actions);
    actions
}

fn max_action_severity(actions: &[serde_json::Value]) -> &'static str {
    let mut max_level = "ok";
    for action in actions {
        let level = action
            .get("severity")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("ok");
        if severity_rank(level) > severity_rank(max_level) {
            max_level = if level == "critical" {
                "critical"
            } else {
                "warning"
            };
        }
    }
    max_level
}

fn should_fail_strict(
    strict: bool,
    severity_floor: Option<&str>,
    diagnostic_severity: &str,
    actions: &[serde_json::Value],
) -> bool {
    if !strict {
        return false;
    }
    let threshold = severity_floor.unwrap_or("warning");
    let action_severity = max_action_severity(actions);
    let effective = if severity_rank(action_severity) > severity_rank(diagnostic_severity) {
        action_severity
    } else {
        diagnostic_severity
    };
    severity_rank(effective) >= severity_rank(threshold)
}

fn scheduler_severity(
    scheduler: &Value,
    retry: &Value,
    critical: &Value,
) -> (&'static str, Vec<String>) {
    let mut reasons: Vec<String> = Vec::new();
    let queue_p95 = scheduler
        .get("queue_ms_p95")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let queue_rows = scheduler
        .get("queue_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if queue_rows >= 3 && queue_p95 >= 2000 {
        reasons.push(format!("queue_p95_high:{queue_p95}"));
    }

    let backend_distribution = scheduler
        .get("backend_distribution")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let total_backend: u64 = backend_distribution
        .values()
        .filter_map(Value::as_u64)
        .sum();
    if total_backend >= 6 {
        let max_backend = backend_distribution
            .values()
            .filter_map(Value::as_u64)
            .max()
            .unwrap_or(0);
        if max_backend * 100 >= total_backend * 90 {
            reasons.push("backend_skew_high".to_string());
        }
    }

    let workers_seen = scheduler
        .get("workers_seen")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let window_runs = scheduler
        .get("window_runs")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if window_runs >= 6 && workers_seen <= 1 {
        reasons.push("worker_spread_low".to_string());
    }

    if window_runs >= 10 {
        let timing_counts = [
            scheduler
                .get("rows_with_retry_attempt")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            scheduler
                .get("rows_with_queue_started_at")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            scheduler
                .get("rows_with_task_started_at")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            scheduler
                .get("rows_with_task_finished_at")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        ];
        let min_ratio = timing_counts
            .iter()
            .map(|v| *v as f64 / window_runs as f64)
            .fold(1.0_f64, f64::min);
        if min_ratio < 0.80 {
            reasons.push(format!("timing_coverage_low:{:.0}", min_ratio * 100.0));
        }
    }

    let retry_rows = retry
        .get("rows_after_retry")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let retry_rows_success_rate = retry
        .get("rows_after_retry_success_rate")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    if retry_rows >= 3 && retry_rows_success_rate < 0.50 {
        reasons.push("retry_recovery_low".to_string());
    }
    let retry_rows_with_meta = retry
        .get("rows_with_retry_metadata")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if window_runs >= 10 && retry_rows_with_meta * 100 >= window_runs * 30 {
        reasons.push("retry_pressure_high".to_string());
    }
    let halted_rows = critical
        .get("halted_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if halted_rows > 0 {
        reasons.push("critical_halts_detected".to_string());
    }

    let severity = if reasons.len() >= 2 {
        "critical"
    } else if reasons.len() == 1 {
        "warning"
    } else {
        "ok"
    };
    (severity, reasons)
}

pub fn cmd_diag(app_version: &str, args: &[String]) -> i32 {
    let (as_json, window, strict, include_actions, severity_floor) = match parse_diag_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            crate::cx_eprintln!(
                "Usage: diag [--json|--text] [--window N] [--strict] [--actions] [--severity warning|critical]"
            );
            return 2;
        }
    };
    let cfg = app_config();
    let provider = cfg.capture_provider.clone();
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    let schema_dir = repo.join(".cx").join("schemas");
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() {
        "<unset>".to_string()
    } else {
        model
    };
    let experiment_caps = selected_tq_caps();
    let runtime_caps = selected_runtime_caps();
    let scheduler = scheduler_diag_value(&log_file, window);
    let retry = retry_diag_value(&log_file, window);
    let critical = critical_diag_value(&log_file, window);
    let concurrency = concurrency_diag_value(&log_file, window, cfg);
    let capture_prompt = capture_prompt_diag(&log_file, window);
    let task_readiness = readiness_diag_value();
    let latest_run = latest_run_all_sum();
    let latest_wave = latest_wave_sum();
    let task_execution = exec_diag_value(latest_run.as_ref(), latest_wave.as_ref());
    let phase7_metrics = phase7_metrics_value(20);
    let adapter_rollout_policy = adapter_policy_value();
    let sample_cmd = "cxo git status";
    let rust_handles = route_handler_for("cxo");
    let bash_handles = bash_type_of_function(&repo, "cxo").is_some();
    let route = if rust_handles.is_some() {
        "rust"
    } else if bash_handles {
        "bash"
    } else {
        "unknown"
    };
    let route_reason = if let Some(h) = rust_handles.as_ref() {
        format!("rust support found ({h})")
    } else if bash_handles {
        "bash fallback function exists".to_string()
    } else {
        "no rust route and no bash fallback".to_string()
    };
    let (severity, severity_reasons) = scheduler_severity(&scheduler, &retry, &critical);
    let actions = if include_actions {
        merge_prompt_action(
            merge_exec_action(
                build_actions_from_reasons(
                    &severity_reasons,
                    window,
                    &crate::config::command_with_cli("task run-all --status pending"),
                ),
                &task_execution,
            ),
            &capture_prompt,
        )
    } else {
        Vec::new()
    };

    if as_json {
        let mut payload = serde_json::json!({
            "contract_version": DIAG_JSON_CONTRACT_VERSION,
            "timestamp": utc_now_iso(),
            "version": toolchain_version_string(app_version),
            "mode": cfg.cx_mode,
            "backend": backend,
            "active_model": active_model,
            "backend_capabilities": {
                "turboquant": {
                    "cx_runtime_support": experiment_caps.turboquant_runtime_support,
                    "selected_backend_role": experiment_caps.turboquant_backend_role,
                    "memory_metric_kind": experiment_caps.turboquant_metric_kind,
                },
                "runtime": runtime_caps_json(runtime_caps)
            },
            "adapter_rollout_policy": adapter_rollout_policy,
            "capture_provider_config": provider,
            "capture_provider_resolved": resolved_provider(&cfg.capture_provider),
            "capture_external_dependencies": "none",
            "budget_chars": cfg.budget_chars,
            "budget_lines": cfg.budget_lines,
            "clip_mode": cfg.clip_mode,
            "clip_footer": cfg.clip_footer,
            "log_file": log_file,
            "last_run_id": last_run_id(),
            "schema_registry_dir": schema_dir.display().to_string(),
            "schema_registry_files": schema_count(&schema_dir),
            "scheduler": scheduler,
            "task_readiness": task_readiness,
            "task_execution": task_execution,
            "phase7_metrics": phase7_metrics,
            "retry": retry,
            "critical": critical,
            "concurrency": concurrency,
            "capture_prompt_profile_rollout": capture_prompt,
            "scheduler_window_requested": window,
            "severity": severity,
            "severity_reasons": severity_reasons,
            "routing_trace": {
                "sample": sample_cmd,
                "route": route,
                "reason": route_reason
            }
        });
        if include_actions {
            payload["actions_contract_version"] =
                serde_json::Value::String(ACTIONS_JSON_CONTRACT_VERSION.to_string());
            payload["actions"] = serde_json::Value::Array(actions.clone());
        }
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!(
                    "{} diag: failed to render json: {e}",
                    crate::config::cli_app_name()
                );
                return 1;
            }
        }
        return if should_fail_strict(strict, severity_floor.as_deref(), severity, &actions) {
            1
        } else {
            0
        };
    }

    print_diag_header(app_version, cfg);
    println!("capture_provider_config: {provider}");
    println!(
        "capture_provider_resolved: {}",
        resolved_provider(&provider)
    );
    println!("capture_external_dependencies: none");
    println!("budget_chars: {}", cfg.budget_chars);
    println!("budget_lines: {}", cfg.budget_lines);
    println!("clip_mode: {}", cfg.clip_mode);
    println!("clip_footer: {}", if cfg.clip_footer { "1" } else { "0" });
    println!("log_file: {log_file}");
    println!("last_run_id: {}", last_run_id());
    println!("schema_registry_dir: {}", schema_dir.display());
    println!("schema_registry_files: {}", schema_count(&schema_dir));
    print_scheduler_diag(&log_file, window);
    print_readiness_diag(&task_readiness);
    print_retry_diag(&log_file, window);
    print_critical_diag(&log_file, window);
    print_prompt_diag(&log_file, window);
    let observed = concurrency
        .get("observed")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let defaults = concurrency
        .get("defaults")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    println!(
        "concurrency_default_mode: {}",
        defaults
            .get("run_all_mode")
            .and_then(Value::as_str)
            .unwrap_or("sequential")
    );
    println!(
        "concurrency_default_workers: {}",
        defaults
            .get("max_workers")
            .and_then(Value::as_u64)
            .unwrap_or(1)
    );
    println!(
        "concurrency_observed_run_all_rows: {}",
        observed
            .get("run_all_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_mode: {}",
        observed
            .get("latest_run_all_mode")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_halted_remaining_total: {}",
        observed
            .get("halted_remaining_total")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_halted_remaining: {}",
        observed
            .get("latest_halted_remaining")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_backend_fallback_rows: {}",
        observed
            .get("backend_fallback_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_backend_fallbacks: {}",
        observed
            .get("latest_backend_fallbacks")
            .and_then(Value::as_str)
            .unwrap_or("none")
    );
    println!(
        "concurrency_observed_latest_worker_count: {}",
        observed
            .get("latest_worker_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_workers: {}",
        observed
            .get("latest_workers")
            .and_then(Value::as_str)
            .unwrap_or("none")
    );
    println!(
        "concurrency_observed_latest_max_retry_attempt: {}",
        observed
            .get("latest_max_retry_attempt")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_first_queue_started_at: {}",
        observed
            .get("latest_first_queue_started_at")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_latest_first_task_started_at: {}",
        observed
            .get("latest_first_task_started_at")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_latest_last_task_finished_at: {}",
        observed
            .get("latest_last_task_finished_at")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_wave_task_rows: {}",
        observed
            .get("wave_task_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_wave_index: {}",
        observed
            .get("latest_wave_index")
            .and_then(Value::as_u64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "concurrency_observed_latest_wave_mode: {}",
        observed
            .get("latest_wave_mode")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_latest_wave_size: {}",
        observed
            .get("latest_wave_size")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_largest_wave_index: {}",
        observed
            .get("largest_wave_index")
            .and_then(Value::as_u64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "concurrency_observed_largest_wave_size: {}",
        observed
            .get("largest_wave_size")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_max_queue_wave_index: {}",
        observed
            .get("max_queue_wave_index")
            .and_then(Value::as_u64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "concurrency_observed_max_queue_wave_ms: {}",
        observed
            .get("max_queue_wave_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    for line in phase7_metric_lines(20) {
        println!("{line}");
    }
    for line in adapter_policy_lines() {
        println!("{line}");
    }
    println!("scheduler_window_requested: {window}");
    println!("severity: {severity}");
    if !severity_reasons.is_empty() {
        println!("severity_reasons: {}", severity_reasons.join(","));
    }
    if include_actions {
        println!("actions:");
        if actions.is_empty() {
            println!("- none");
        } else {
            for action in &actions {
                println!(
                    "- [{}] {}: {} -> {}",
                    action
                        .get("severity")
                        .and_then(Value::as_str)
                        .unwrap_or("ok"),
                    action.get("id").and_then(Value::as_str).unwrap_or("action"),
                    action
                        .get("rationale")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    action.get("command").and_then(Value::as_str).unwrap_or("")
                );
            }
        }
    }

    println!(
        "routing_trace: sample='{}' route={} reason={}",
        sample_cmd, route, route_reason
    );
    if should_fail_strict(strict, severity_floor.as_deref(), severity, &actions) {
        1
    } else {
        0
    }
}

pub fn cmd_scheduler(args: &[String]) -> i32 {
    let (as_json, window, strict, include_actions, severity_floor) = match parse_diag_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            crate::cx_eprintln!(
                "Usage: scheduler [--json|--text] [--window N] [--strict] [--actions] [--severity warning|critical]"
            );
            return 2;
        }
    };
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let cfg = app_config();
    let scheduler = scheduler_diag_value(&log_file, window);
    let retry = retry_diag_value(&log_file, window);
    let critical = critical_diag_value(&log_file, window);
    let concurrency = concurrency_diag_value(&log_file, window, cfg);
    let task_readiness = readiness_diag_value();
    let latest_run = latest_run_all_sum();
    let latest_wave = latest_wave_sum();
    let task_execution = exec_diag_value(latest_run.as_ref(), latest_wave.as_ref());
    let phase7_metrics = phase7_metrics_value(20);
    let adapter_rollout_policy = adapter_policy_value();
    let experiment_caps = selected_tq_caps();
    let runtime_caps = selected_runtime_caps();
    let (severity, severity_reasons) = scheduler_severity(&scheduler, &retry, &critical);
    let actions = if include_actions {
        merge_exec_action(
            build_actions_from_reasons(
                &severity_reasons,
                window,
                &crate::config::command_with_cli("task run-all --status pending"),
            ),
            &task_execution,
        )
    } else {
        Vec::new()
    };

    if as_json {
        let mut payload = serde_json::json!({
            "contract_version": SCHEDULER_JSON_CONTRACT_VERSION,
            "log_file": log_file,
            "scheduler_window_requested": window,
            "backend_capabilities": {
                "turboquant": {
                    "cx_runtime_support": experiment_caps.turboquant_runtime_support,
                    "selected_backend_role": experiment_caps.turboquant_backend_role,
                    "memory_metric_kind": experiment_caps.turboquant_metric_kind,
                },
                "runtime": runtime_caps_json(runtime_caps)
            },
            "adapter_rollout_policy": adapter_rollout_policy,
            "scheduler": scheduler,
            "task_readiness": task_readiness,
            "task_execution": task_execution,
            "phase7_metrics": phase7_metrics,
            "retry": retry,
            "critical": critical,
            "concurrency": concurrency,
            "severity": severity,
            "severity_reasons": severity_reasons
        });
        if include_actions {
            payload["actions_contract_version"] =
                serde_json::Value::String(ACTIONS_JSON_CONTRACT_VERSION.to_string());
            payload["actions"] = serde_json::Value::Array(actions.clone());
        }
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!(
                    "{} scheduler: failed to render json: {e}",
                    crate::config::cli_app_name()
                );
                return 1;
            }
        }
        return if should_fail_strict(strict, severity_floor.as_deref(), severity, &actions) {
            1
        } else {
            0
        };
    }

    println!("== cxscheduler ==");
    println!("log_file: {log_file}");
    print_scheduler_diag(&log_file, window);
    print_readiness_diag(&task_readiness);
    print_retry_diag(&log_file, window);
    print_critical_diag(&log_file, window);
    let observed = concurrency
        .get("observed")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let defaults = concurrency
        .get("defaults")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    println!(
        "concurrency_default_mode: {}",
        defaults
            .get("run_all_mode")
            .and_then(Value::as_str)
            .unwrap_or("sequential")
    );
    println!(
        "concurrency_default_workers: {}",
        defaults
            .get("max_workers")
            .and_then(Value::as_u64)
            .unwrap_or(1)
    );
    println!(
        "concurrency_observed_run_all_rows: {}",
        observed
            .get("run_all_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_mode: {}",
        observed
            .get("latest_run_all_mode")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_halted_remaining_total: {}",
        observed
            .get("halted_remaining_total")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_halted_remaining: {}",
        observed
            .get("latest_halted_remaining")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_backend_fallback_rows: {}",
        observed
            .get("backend_fallback_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_backend_fallbacks: {}",
        observed
            .get("latest_backend_fallbacks")
            .and_then(Value::as_str)
            .unwrap_or("none")
    );
    println!(
        "concurrency_observed_latest_worker_count: {}",
        observed
            .get("latest_worker_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_workers: {}",
        observed
            .get("latest_workers")
            .and_then(Value::as_str)
            .unwrap_or("none")
    );
    println!(
        "concurrency_observed_latest_max_retry_attempt: {}",
        observed
            .get("latest_max_retry_attempt")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_first_queue_started_at: {}",
        observed
            .get("latest_first_queue_started_at")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_latest_first_task_started_at: {}",
        observed
            .get("latest_first_task_started_at")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_latest_last_task_finished_at: {}",
        observed
            .get("latest_last_task_finished_at")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_wave_task_rows: {}",
        observed
            .get("wave_task_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_latest_wave_index: {}",
        observed
            .get("latest_wave_index")
            .and_then(Value::as_u64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "concurrency_observed_latest_wave_mode: {}",
        observed
            .get("latest_wave_mode")
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    println!(
        "concurrency_observed_latest_wave_size: {}",
        observed
            .get("latest_wave_size")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_largest_wave_index: {}",
        observed
            .get("largest_wave_index")
            .and_then(Value::as_u64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "concurrency_observed_largest_wave_size: {}",
        observed
            .get("largest_wave_size")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "concurrency_observed_max_queue_wave_index: {}",
        observed
            .get("max_queue_wave_index")
            .and_then(Value::as_u64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "concurrency_observed_max_queue_wave_ms: {}",
        observed
            .get("max_queue_wave_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    for line in phase7_metric_lines(20) {
        println!("{line}");
    }
    for line in adapter_policy_lines() {
        println!("{line}");
    }
    println!("scheduler_window_requested: {window}");
    println!("severity: {severity}");
    if !severity_reasons.is_empty() {
        println!("severity_reasons: {}", severity_reasons.join(","));
    }
    if include_actions {
        println!("actions:");
        if actions.is_empty() {
            println!("- none");
        } else {
            for action in &actions {
                println!(
                    "- [{}] {}: {} -> {}",
                    action
                        .get("severity")
                        .and_then(Value::as_str)
                        .unwrap_or("ok"),
                    action.get("id").and_then(Value::as_str).unwrap_or("action"),
                    action
                        .get("rationale")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    action.get("command").and_then(Value::as_str).unwrap_or("")
                );
            }
        }
    }
    if should_fail_strict(strict, severity_floor.as_deref(), severity, &actions) {
        1
    } else {
        0
    }
}

pub fn last_appended_json_value(log_file: &Path, offset: u64) -> Option<Value> {
    if !log_file.exists() {
        return None;
    }
    let mut file = File::open(log_file).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let start = (offset as usize).min(bytes.len());
    let tail = String::from_utf8_lossy(&bytes[start..]);
    tail.lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
}

pub fn has_required_log_fields(v: &Value) -> bool {
    let required = [
        "execution_id",
        "backend_used",
        "capture_provider",
        "execution_mode",
        "schema_enforced",
        "schema_valid",
        "policy_blocked",
        "duration_ms",
    ];
    required.iter().all(|k| v.get(k).is_some())
}

#[cfg(test)]
mod tests {
    use super::{
        action_cost_rank, build_actions_from_reasons, has_required_log_fields, scheduler_severity,
    };
    use serde_json::json;

    #[test]
    fn require_policy_field() {
        let row_missing = json!({
            "execution_id":"e1",
            "backend_used":"primary",
            "capture_provider":"native",
            "execution_mode":"lean",
            "schema_enforced":false,
            "schema_valid":true,
            "duration_ms":10
        });
        assert!(
            !has_required_log_fields(&row_missing),
            "policy_blocked must be present"
        );

        let row_with = json!({
            "execution_id":"e2",
            "backend_used":"primary",
            "capture_provider":"native",
            "execution_mode":"lean",
            "schema_enforced":false,
            "schema_valid":true,
            "policy_blocked":false,
            "duration_ms":11
        });
        assert!(has_required_log_fields(&row_with));
    }

    #[test]
    fn action_timing_cov() {
        let actions = build_actions_from_reasons(
            &["timing_coverage_low:55".to_string()],
            200,
            &crate::config::command_with_cli("task run-all --status pending"),
        );
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].get("id").and_then(serde_json::Value::as_str),
            Some("timing_coverage_low")
        );
    }

    #[test]
    fn action_order_cov() {
        let actions = build_actions_from_reasons(
            &[
                "retry_pressure_high:7".to_string(),
                "queue_p95_high:2500".to_string(),
                "critical_halts_detected:1".to_string(),
            ],
            200,
            &crate::config::command_with_cli("task run-all --status pending"),
        );
        assert_eq!(actions.len(), 3);
        assert_eq!(
            actions[0].get("id").and_then(serde_json::Value::as_str),
            Some("critical_halts_detected")
        );
        assert_eq!(
            actions[1].get("id").and_then(serde_json::Value::as_str),
            Some("queue_p95_high")
        );
        let first_warning = actions[1]
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let second_warning = actions[2]
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        assert!(action_cost_rank(first_warning) <= action_cost_rank(second_warning));
    }

    #[test]
    fn scheduler_timing_cov() {
        let scheduler = json!({
            "queue_ms_p95": 0,
            "queue_rows": 12,
            "backend_distribution": {"primary": 6, "ollama": 6},
            "workers_seen": ["w1", "w2"],
            "window_runs": 12,
            "rows_with_retry_attempt": 12,
            "rows_with_queue_started_at": 12,
            "rows_with_task_started_at": 3,
            "rows_with_task_finished_at": 12
        });
        let retry = json!({
            "rows_after_retry": 0,
            "rows_after_retry_success_rate": 1.0,
            "rows_with_retry_metadata": 0
        });
        let critical = json!({"halted_rows": 0});
        let (sev, reasons) = scheduler_severity(&scheduler, &retry, &critical);
        assert_eq!(sev, "warning");
        assert!(
            reasons
                .iter()
                .any(|r| r.starts_with("timing_coverage_low:"))
        );
    }
}

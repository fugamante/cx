use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::config::app_config;
use crate::execmeta::{toolchain_version_string, utc_now_iso};
use crate::logs::file_len;
use crate::logs::load_values;
use crate::paths::{repo_root_hint, resolve_log_file};
use crate::process::{run_command_output_with_timeout, run_command_status_with_timeout};
use crate::routing::{bash_type_of_function, route_handler_for};
use crate::runtime::{llm_backend, llm_model};

fn bin_in_path(bin: &str) -> bool {
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", &format!("command -v {} >/dev/null 2>&1", bin)]);
    run_command_status_with_timeout(cmd, "command -v")
        .ok()
        .is_some_and(|s| s.success())
}

fn rtk_version_string() -> String {
    let mut cmd = Command::new("rtk");
    cmd.arg("--version");
    let out = match run_command_output_with_timeout(cmd, "rtk --version") {
        Ok(v) if v.status.success() => v,
        _ => return "<unavailable>".to_string(),
    };
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        "<unavailable>".to_string()
    } else {
        s
    }
}

fn resolved_provider(cfg_provider: &str) -> &'static str {
    match cfg_provider {
        "rtk" => "rtk",
        "native" => "native",
        _ => {
            if bin_in_path("rtk")
                && std::env::var("CX_RTK_SYSTEM").unwrap_or_else(|_| "1".to_string()) == "1"
            {
                "rtk"
            } else {
                "native"
            }
        }
    }
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
    println!("scheduler_workers_seen: {workers_seen}");
    println!("scheduler_worker_distribution: {worker_distribution}");
    println!("scheduler_backend_distribution: {backend_distribution}");
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
    println!("== cxdiag ==");
    println!("timestamp: {}", utc_now_iso());
    println!("version: {}", toolchain_version_string(app_version));
    println!("mode: {}", cfg.cx_mode);
    println!("backend: {backend}");
    println!("active_model: {active_model}");
}

fn scheduler_diag_value(log_file_path: &str, window: usize) -> Value {
    let path = Path::new(log_file_path);
    if !path.exists() {
        return serde_json::json!({
            "window_runs": 0,
            "queue_rows": 0,
            "queue_ms_avg": Value::Null,
            "queue_ms_p95": Value::Null,
            "workers_seen": [],
            "worker_distribution": {},
            "backend_distribution": {}
        });
    }
    let rows = load_values(path, window).unwrap_or_default();
    let mut queue_vals: Vec<u64> = Vec::new();
    let mut worker_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut backend_counts: BTreeMap<String, usize> = BTreeMap::new();

    for v in &rows {
        if let Some(q) = v.get("queue_ms").and_then(Value::as_u64) {
            queue_vals.push(q);
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
        "workers_seen": worker_counts.keys().cloned().collect::<Vec<String>>(),
        "worker_distribution": worker_counts,
        "backend_distribution": backend_counts
    })
}

fn parse_diag_args(args: &[String]) -> Result<(bool, usize, bool), String> {
    let mut as_json = false;
    let mut window = 200usize;
    let mut strict = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                as_json = true;
                i += 1;
            }
            "--strict" => {
                strict = true;
                i += 1;
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
    Ok((as_json, window, strict))
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
    let (as_json, window, strict) = match parse_diag_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            crate::cx_eprintln!("Usage: diag [--json] [--window N] [--strict]");
            return 2;
        }
    };
    let cfg = app_config();
    let provider = cfg.capture_provider.clone();
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    let schema_dir = repo.join(".codex").join("schemas");
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() {
        "<unset>".to_string()
    } else {
        model
    };
    let scheduler = scheduler_diag_value(&log_file, window);
    let retry = retry_diag_value(&log_file, window);
    let critical = critical_diag_value(&log_file, window);
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

    if as_json {
        let payload = serde_json::json!({
            "timestamp": utc_now_iso(),
            "version": toolchain_version_string(app_version),
            "mode": cfg.cx_mode,
            "backend": backend,
            "active_model": active_model,
            "capture_provider_config": provider,
            "capture_provider_resolved": resolved_provider(&cfg.capture_provider),
            "rtk_available": bin_in_path("rtk"),
            "rtk_version": rtk_version_string(),
            "budget_chars": cfg.budget_chars,
            "budget_lines": cfg.budget_lines,
            "clip_mode": cfg.clip_mode,
            "clip_footer": cfg.clip_footer,
            "log_file": log_file,
            "last_run_id": last_run_id(),
            "schema_registry_dir": schema_dir.display().to_string(),
            "schema_registry_files": schema_count(&schema_dir),
            "scheduler": scheduler,
            "retry": retry,
            "critical": critical,
            "scheduler_window_requested": window,
            "severity": severity,
            "severity_reasons": severity_reasons,
            "routing_trace": {
                "sample": sample_cmd,
                "route": route,
                "reason": route_reason
            }
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("cxrs diag: failed to render json: {e}");
                return 1;
            }
        }
        return if strict && severity != "ok" { 1 } else { 0 };
    }

    print_diag_header(app_version, cfg);
    println!("capture_provider_config: {provider}");
    println!(
        "capture_provider_resolved: {}",
        resolved_provider(&provider)
    );
    println!("rtk_available: {}", bin_in_path("rtk"));
    println!("rtk_version: {}", rtk_version_string());
    println!("budget_chars: {}", cfg.budget_chars);
    println!("budget_lines: {}", cfg.budget_lines);
    println!("clip_mode: {}", cfg.clip_mode);
    println!("clip_footer: {}", if cfg.clip_footer { "1" } else { "0" });
    println!("log_file: {log_file}");
    println!("last_run_id: {}", last_run_id());
    println!("schema_registry_dir: {}", schema_dir.display());
    println!("schema_registry_files: {}", schema_count(&schema_dir));
    print_scheduler_diag(&log_file, window);
    print_retry_diag(&log_file, window);
    print_critical_diag(&log_file, window);
    println!("scheduler_window_requested: {window}");
    println!("severity: {severity}");
    if !severity_reasons.is_empty() {
        println!("severity_reasons: {}", severity_reasons.join(","));
    }

    println!(
        "routing_trace: sample='{}' route={} reason={}",
        sample_cmd, route, route_reason
    );
    if strict && severity != "ok" { 1 } else { 0 }
}

pub fn cmd_scheduler(args: &[String]) -> i32 {
    let (as_json, window, strict) = match parse_diag_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            crate::cx_eprintln!("Usage: scheduler [--json] [--window N] [--strict]");
            return 2;
        }
    };
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let scheduler = scheduler_diag_value(&log_file, window);
    let retry = retry_diag_value(&log_file, window);
    let critical = critical_diag_value(&log_file, window);
    let (severity, severity_reasons) = scheduler_severity(&scheduler, &retry, &critical);

    if as_json {
        let payload = serde_json::json!({
            "log_file": log_file,
            "scheduler_window_requested": window,
            "scheduler": scheduler,
            "retry": retry,
            "critical": critical,
            "severity": severity,
            "severity_reasons": severity_reasons
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("cxrs scheduler: failed to render json: {e}");
                return 1;
            }
        }
        return if strict && severity != "ok" { 1 } else { 0 };
    }

    println!("== cxscheduler ==");
    println!("log_file: {log_file}");
    print_scheduler_diag(&log_file, window);
    print_retry_diag(&log_file, window);
    print_critical_diag(&log_file, window);
    println!("scheduler_window_requested: {window}");
    println!("severity: {severity}");
    if !severity_reasons.is_empty() {
        println!("severity_reasons: {}", severity_reasons.join(","));
    }
    if strict && severity != "ok" { 1 } else { 0 }
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
        "duration_ms",
    ];
    required.iter().all(|k| v.get(k).is_some())
}

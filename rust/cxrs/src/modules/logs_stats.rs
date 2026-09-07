use crate::config::cli_app_name;
use crate::contract_versions::TELEMETRY_JSON_CONTRACT_VERSION;
use crate::doctor::{exec_diag_value, latest_run_all_sum, latest_wave_sum, phase7_metrics_value};
use crate::json_mode::resolve_json_mode;
use crate::log_contract::REQUIRED_STRICT_FIELDS;
use crate::logs::load_values;
use crate::paths::resolve_log_file;
use crate::provider_adapter::{adapter_policy_value, selected_tq_caps};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

struct StatsArgs {
    n: usize,
    json_out: bool,
    strict: bool,
    severity: bool,
}

fn parse_stats_args(app_name: &str, args: &[String]) -> Result<StatsArgs, i32> {
    let mut n = 200usize;
    let mut json_out: Option<bool> = None;
    let mut strict = false;
    let mut severity = false;
    for a in args.iter().skip(1) {
        if a == "--json" {
            json_out = Some(true);
            continue;
        }
        if a == "--text" {
            json_out = Some(false);
            continue;
        }
        if a == "--strict" {
            strict = true;
            continue;
        }
        if a == "--severity" {
            severity = true;
            continue;
        }
        match a.parse::<usize>() {
            Ok(v) if v > 0 => n = v,
            _ => {
                crate::cx_eprintln!(
                    "Usage: {app_name} logs stats [N] [--json|--text] [--strict] [--severity]"
                );
                return Err(2);
            }
        }
    }
    Ok(StatsArgs {
        n,
        json_out: resolve_json_mode(json_out, false),
        strict,
        severity,
    })
}

fn key_union(rows: &[Value]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for r in rows {
        if let Some(obj) = r.as_object() {
            for k in obj.keys() {
                out.insert(k.to_string());
            }
        }
    }
    out
}

fn field_population(rows: &[Value], field: &str) -> (usize, usize) {
    let mut present = 0usize;
    let mut non_null = 0usize;
    for r in rows {
        let Some(obj) = r.as_object() else {
            continue;
        };
        if let Some(v) = obj.get(field) {
            present += 1;
            if !v.is_null() {
                non_null += 1;
            }
        }
    }
    (present, non_null)
}

fn coverage_lines(rows: &[Value]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for field in REQUIRED_STRICT_FIELDS {
        let (present, non_null) = field_population(rows, field);
        let total = rows.len().max(1);
        let present_pct = (present as f64 / total as f64) * 100.0;
        let non_null_pct = (non_null as f64 / total as f64) * 100.0;
        lines.push(format!(
            "{} present={}/{} ({:.0}%) non_null={}/{} ({:.0}%)",
            field,
            present,
            rows.len(),
            present_pct,
            non_null,
            rows.len(),
            non_null_pct
        ));
    }
    lines
}

struct StatsComputed {
    lines: Vec<String>,
    strict_violations: usize,
    new_in_second: Vec<String>,
    missing_in_second: Vec<String>,
    severity: &'static str,
    normalization: NormalizationStats,
    capture_prompt: CapturePromptStats,
    retry: RetryStats,
    critical: CriticalStats,
    timing: TimingStats,
    http_mode_stats: Vec<HttpModeStat>,
}

#[derive(Debug, Default, Clone)]
struct CapturePromptStats {
    rows_with_explicit_profile: usize,
    shadow_narrow_configured_runs: usize,
    shadow_narrow_applied_runs: usize,
    shadow_narrow_fallback_runs: usize,
    applied_reducer_kinds: BTreeMap<String, usize>,
    fallback_reasons: BTreeMap<String, usize>,
}

#[derive(Debug, Default, Clone)]
struct RetryStats {
    rows_with_retry_metadata: usize,
    rows_after_retry: usize,
    rows_after_retry_success: usize,
    rows_after_retry_success_rate: f64,
    tasks_with_retry: usize,
    tasks_retry_recovered: usize,
    tasks_retry_recovery_rate: f64,
    attempt_histogram: BTreeMap<u64, usize>,
}

#[derive(Debug, Default, Clone)]
struct CriticalStats {
    summary_rows: usize,
    halt_enabled_rows: usize,
    halted_rows: usize,
    critical_errors_total: u64,
    runs_with_critical_errors: usize,
}

#[derive(Debug, Default, Clone)]
struct TimingStats {
    rows_with_worker_id: usize,
    rows_with_queue_ms: usize,
    rows_with_wave_index: usize,
    rows_with_wave_mode: usize,
    rows_with_wave_size: usize,
    rows_with_queue_started_at: usize,
    rows_with_task_started_at: usize,
    rows_with_task_finished_at: usize,
    task_rows: usize,
}

#[derive(Debug, Clone)]
struct HttpModeStat {
    format: String,
    parser_mode: String,
    runs: usize,
    schema_invalid: usize,
    timed_out: usize,
    policy_blocked: usize,
    healthy_runs: usize,
}

#[derive(Debug, Default, Clone)]
struct NormalizationStats {
    modern_rows: usize,
    legacy_rows: usize,
    migrated_legacy_rows: usize,
}

fn severity_label(strict_violations: usize, new_keys: usize, missing_keys: usize) -> &'static str {
    if strict_violations > 0 || missing_keys > 0 {
        return "critical";
    }
    if new_keys > 0 {
        return "warning";
    }
    "ok"
}

fn drift_sets(rows: &[Value]) -> (Vec<String>, Vec<String>) {
    if rows.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let mid = rows.len() / 2;
    let first = key_union(&rows[..mid.max(1)]);
    let second = key_union(&rows[mid..]);
    let new_in_second: Vec<String> = second.difference(&first).cloned().collect();
    let missing_in_second: Vec<String> = first.difference(&second).cloned().collect();
    (new_in_second, missing_in_second)
}

fn compute_stats(rows: &[Value]) -> StatsComputed {
    let lines = coverage_lines(rows);
    let strict_violations = REQUIRED_STRICT_FIELDS
        .iter()
        .filter(|field| {
            let (present, _) = field_population(rows, field);
            present < rows.len()
        })
        .count();
    let (new_in_second, missing_in_second) = drift_sets(rows);
    let severity = severity_label(
        strict_violations,
        new_in_second.len(),
        missing_in_second.len(),
    );
    let normalization = compute_normalization_stats(rows);
    let capture_prompt = compute_prompt_stats(rows);
    let retry = compute_retry_stats(rows);
    let critical = compute_critical_stats(rows);
    let timing = compute_timing_stats(rows);
    let http_mode_stats = compute_http_mode_stats(rows);
    StatsComputed {
        lines,
        strict_violations,
        new_in_second,
        missing_in_second,
        severity,
        normalization,
        capture_prompt,
        retry,
        critical,
        timing,
        http_mode_stats,
    }
}

fn compute_normalization_stats(rows: &[Value]) -> NormalizationStats {
    let mut modern_rows = 0usize;
    let mut legacy_rows = 0usize;
    let mut migrated_legacy_rows = 0usize;
    for r in rows {
        let Some(obj) = r.as_object() else {
            continue;
        };
        let has_strict_fields = REQUIRED_STRICT_FIELDS
            .iter()
            .all(|field| obj.contains_key(*field));
        if has_strict_fields {
            modern_rows += 1;
        } else {
            legacy_rows += 1;
        }
        let mode = obj
            .get("execution_mode")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if mode.starts_with("legacy") {
            migrated_legacy_rows += 1;
        }
    }
    NormalizationStats {
        modern_rows,
        legacy_rows,
        migrated_legacy_rows,
    }
}

fn compute_prompt_stats(rows: &[Value]) -> CapturePromptStats {
    let mut out = CapturePromptStats::default();
    for r in rows {
        let Some(obj) = r.as_object() else {
            continue;
        };
        let profile = obj
            .get("capture_prompt_profile")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(profile) = profile else {
            continue;
        };
        out.rows_with_explicit_profile += 1;
        if profile != "shadow_narrow" {
            continue;
        }
        out.shadow_narrow_configured_runs += 1;
        let applied = obj
            .get("capture_prompt_profile_applied")
            .and_then(Value::as_bool)
            == Some(true);
        if applied {
            out.shadow_narrow_applied_runs += 1;
            if let Some(kind) = obj
                .get("capture_prompt_reducer_kind")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                *out.applied_reducer_kinds
                    .entry(kind.to_string())
                    .or_insert(0) += 1;
            }
            continue;
        }
        out.shadow_narrow_fallback_runs += 1;
        if let Some(reason) = obj
            .get("capture_prompt_fallback_reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            *out.fallback_reasons.entry(reason.to_string()).or_insert(0) += 1;
        }
    }
    out
}

fn compute_timing_stats(rows: &[Value]) -> TimingStats {
    let mut out = TimingStats::default();
    for r in rows {
        let Some(obj) = r.as_object() else {
            continue;
        };
        let has_task = obj
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|v| !v.is_empty());
        if has_task {
            out.task_rows += 1;
        }
        if obj
            .get("worker_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|v| !v.is_empty())
        {
            out.rows_with_worker_id += 1;
        }
        if obj.get("queue_ms").and_then(Value::as_u64).is_some() {
            out.rows_with_queue_ms += 1;
        }
        if obj.get("wave_index").and_then(Value::as_u64).is_some() {
            out.rows_with_wave_index += 1;
        }
        if obj
            .get("wave_mode")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|v| !v.is_empty())
        {
            out.rows_with_wave_mode += 1;
        }
        if obj.get("wave_size").and_then(Value::as_u64).is_some() {
            out.rows_with_wave_size += 1;
        }
        if obj
            .get("queue_started_at")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|v| !v.is_empty())
        {
            out.rows_with_queue_started_at += 1;
        }
        if obj
            .get("task_started_at")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|v| !v.is_empty())
        {
            out.rows_with_task_started_at += 1;
        }
        if obj
            .get("task_finished_at")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|v| !v.is_empty())
        {
            out.rows_with_task_finished_at += 1;
        }
    }
    out
}

fn compute_http_mode_stats(rows: &[Value]) -> Vec<HttpModeStat> {
    let mut agg: BTreeMap<(String, String), HttpModeStat> = BTreeMap::new();
    for r in rows {
        let Some(obj) = r.as_object() else {
            continue;
        };
        if obj.get("provider_transport").and_then(Value::as_str) != Some("http") {
            continue;
        }
        let format = obj
            .get("http_provider_format")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let parser_mode = obj
            .get("http_parser_mode")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let key = (format.clone(), parser_mode.clone());
        let entry = agg.entry(key).or_insert_with(|| HttpModeStat {
            format,
            parser_mode,
            runs: 0,
            schema_invalid: 0,
            timed_out: 0,
            policy_blocked: 0,
            healthy_runs: 0,
        });
        entry.runs += 1;
        let schema_invalid = obj.get("schema_valid").and_then(Value::as_bool) == Some(false);
        let timed_out = obj.get("timed_out").and_then(Value::as_bool) == Some(true);
        let policy_blocked = obj.get("policy_blocked").and_then(Value::as_bool) == Some(true);
        if schema_invalid {
            entry.schema_invalid += 1;
        }
        if timed_out {
            entry.timed_out += 1;
        }
        if policy_blocked {
            entry.policy_blocked += 1;
        }
        if !schema_invalid && !timed_out && !policy_blocked {
            entry.healthy_runs += 1;
        }
    }
    agg.into_values().collect()
}

fn compute_retry_stats(rows: &[Value]) -> RetryStats {
    let mut attempt_histogram: BTreeMap<u64, usize> = BTreeMap::new();
    let mut rows_with_retry_metadata = 0usize;
    let mut rows_after_retry = 0usize;
    let mut rows_after_retry_success = 0usize;
    let mut task_timeout_seen: BTreeMap<String, bool> = BTreeMap::new();
    let mut task_recovered: BTreeMap<String, bool> = BTreeMap::new();

    for r in rows {
        let Some(obj) = r.as_object() else {
            continue;
        };
        let attempt = obj.get("retry_attempt").and_then(Value::as_u64);
        if let Some(a) = attempt {
            rows_with_retry_metadata += 1;
            *attempt_histogram.entry(a).or_insert(0) += 1;
            if a > 1 {
                rows_after_retry += 1;
                let timed_out = obj.get("timed_out").and_then(Value::as_bool) == Some(true);
                let schema_valid = obj.get("schema_valid").and_then(Value::as_bool) != Some(false);
                let policy_blocked =
                    obj.get("policy_blocked").and_then(Value::as_bool) == Some(true);
                if !timed_out && schema_valid && !policy_blocked {
                    rows_after_retry_success += 1;
                }
            }
        }
        let task_id = obj
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        if let Some(tid) = task_id {
            let timed_out = obj.get("timed_out").and_then(Value::as_bool) == Some(true);
            if attempt.is_some() {
                task_timeout_seen.entry(tid.clone()).or_insert(false);
                task_recovered.entry(tid.clone()).or_insert(false);
            }
            if timed_out {
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
        .filter(|(tid, saw_timeout)| **saw_timeout && task_recovered.get(*tid) == Some(&true))
        .count();
    let tasks_retry_recovery_rate = if tasks_with_retry == 0 {
        0.0
    } else {
        tasks_retry_recovered as f64 / tasks_with_retry as f64
    };

    RetryStats {
        rows_with_retry_metadata,
        rows_after_retry,
        rows_after_retry_success,
        rows_after_retry_success_rate,
        tasks_with_retry,
        tasks_retry_recovered,
        tasks_retry_recovery_rate,
        attempt_histogram,
    }
}

fn compute_critical_stats(rows: &[Value]) -> CriticalStats {
    let mut summary_rows = 0usize;
    let mut halt_enabled_rows = 0usize;
    let mut halted_rows = 0usize;
    let mut critical_errors_total = 0u64;
    let mut runs_with_critical_errors = 0usize;
    for r in rows {
        let Some(obj) = r.as_object() else {
            continue;
        };
        if obj.get("tool").and_then(Value::as_str) != Some("cxtask_runall") {
            continue;
        }
        summary_rows += 1;
        if obj.get("halt_on_critical").and_then(Value::as_bool) == Some(true) {
            halt_enabled_rows += 1;
        }
        let critical = obj
            .get("run_all_critical_errors")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        critical_errors_total += critical;
        if critical > 0 {
            runs_with_critical_errors += 1;
        }
        let halted = obj
            .get("run_all_scheduled")
            .and_then(Value::as_u64)
            .zip(obj.get("run_all_complete").and_then(Value::as_u64))
            .zip(obj.get("run_all_failed").and_then(Value::as_u64))
            .map(|((scheduled, complete), failed)| complete + failed < scheduled)
            .unwrap_or(false);
        if halted {
            halted_rows += 1;
        }
    }
    CriticalStats {
        summary_rows,
        halt_enabled_rows,
        halted_rows,
        critical_errors_total,
        runs_with_critical_errors,
    }
}

fn print_stats_human(
    app_name: &str,
    log_file: &Path,
    rows: &[Value],
    stats: &StatsComputed,
    severity_only: bool,
) {
    println!("== {app_name} logs stats ==");
    println!("log_file: {}", log_file.display());
    println!("window_runs: {}", rows.len());
    println!("required_fields: {}", REQUIRED_STRICT_FIELDS.len());
    println!("severity: {}", stats.severity);
    println!("strict_violations: {}", stats.strict_violations);
    println!("normalization:");
    println!("- modern_rows: {}", stats.normalization.modern_rows);
    println!("- legacy_rows: {}", stats.normalization.legacy_rows);
    println!(
        "- migrated_legacy_rows: {}",
        stats.normalization.migrated_legacy_rows
    );
    if stats.normalization.legacy_rows > 0 {
        println!("- recommendation: run `cx logs migrate` to normalize legacy rows");
    } else {
        println!("- recommendation: none");
    }
    if severity_only {
        return;
    }
    println!("capture_prompt_telemetry:");
    println!(
        "- rows_with_explicit_profile: {}",
        stats.capture_prompt.rows_with_explicit_profile
    );
    println!(
        "- shadow_narrow_configured_runs: {}",
        stats.capture_prompt.shadow_narrow_configured_runs
    );
    println!(
        "- shadow_narrow_applied_runs: {}",
        stats.capture_prompt.shadow_narrow_applied_runs
    );
    println!(
        "- shadow_narrow_fallback_runs: {}",
        stats.capture_prompt.shadow_narrow_fallback_runs
    );
    let applied_kinds = if stats.capture_prompt.applied_reducer_kinds.is_empty() {
        "<none>".to_string()
    } else {
        stats
            .capture_prompt
            .applied_reducer_kinds
            .iter()
            .map(|(kind, count)| format!("{kind}:{count}"))
            .collect::<Vec<String>>()
            .join(",")
    };
    println!("- applied_reducer_kinds: {}", applied_kinds);
    let fallback_reasons = if stats.capture_prompt.fallback_reasons.is_empty() {
        "<none>".to_string()
    } else {
        stats
            .capture_prompt
            .fallback_reasons
            .iter()
            .map(|(reason, count)| format!("{reason}:{count}"))
            .collect::<Vec<String>>()
            .join(",")
    };
    println!("- fallback_reasons: {}", fallback_reasons);
    println!("retry_telemetry:");
    println!(
        "- rows_with_retry_metadata: {}",
        stats.retry.rows_with_retry_metadata
    );
    println!("- rows_after_retry: {}", stats.retry.rows_after_retry);
    println!(
        "- rows_after_retry_success: {}",
        stats.retry.rows_after_retry_success
    );
    println!(
        "- rows_after_retry_success_rate: {:.2}",
        stats.retry.rows_after_retry_success_rate
    );
    println!("- tasks_with_retry: {}", stats.retry.tasks_with_retry);
    println!(
        "- tasks_retry_recovered: {}",
        stats.retry.tasks_retry_recovered
    );
    println!(
        "- tasks_retry_recovery_rate: {:.2}",
        stats.retry.tasks_retry_recovery_rate
    );
    let attempt_hist = if stats.retry.attempt_histogram.is_empty() {
        "<none>".to_string()
    } else {
        stats
            .retry
            .attempt_histogram
            .iter()
            .map(|(attempt, count)| format!("{attempt}:{count}"))
            .collect::<Vec<String>>()
            .join(",")
    };
    println!("- retry_attempt_histogram: {}", attempt_hist);
    println!("critical_telemetry:");
    println!("- summary_rows: {}", stats.critical.summary_rows);
    println!("- halt_enabled_rows: {}", stats.critical.halt_enabled_rows);
    println!("- halted_rows: {}", stats.critical.halted_rows);
    println!(
        "- critical_errors_total: {}",
        stats.critical.critical_errors_total
    );
    println!(
        "- runs_with_critical_errors: {}",
        stats.critical.runs_with_critical_errors
    );
    println!("timing_telemetry:");
    println!("- task_rows: {}", stats.timing.task_rows);
    println!(
        "- rows_with_worker_id: {}",
        stats.timing.rows_with_worker_id
    );
    println!("- rows_with_queue_ms: {}", stats.timing.rows_with_queue_ms);
    println!(
        "- rows_with_wave_index: {}",
        stats.timing.rows_with_wave_index
    );
    println!(
        "- rows_with_wave_mode: {}",
        stats.timing.rows_with_wave_mode
    );
    println!(
        "- rows_with_wave_size: {}",
        stats.timing.rows_with_wave_size
    );
    println!(
        "- rows_with_queue_started_at: {}",
        stats.timing.rows_with_queue_started_at
    );
    println!(
        "- rows_with_task_started_at: {}",
        stats.timing.rows_with_task_started_at
    );
    println!(
        "- rows_with_task_finished_at: {}",
        stats.timing.rows_with_task_finished_at
    );
    println!("http_mode_stats:");
    if stats.http_mode_stats.is_empty() {
        println!("- <none>");
    } else {
        for mode in &stats.http_mode_stats {
            let success_rate = if mode.runs == 0 {
                0.0
            } else {
                mode.healthy_runs as f64 / mode.runs as f64
            };
            println!(
                "- format={} parser_mode={} runs={} healthy={} success_rate={:.2} schema_invalid={} timed_out={} policy_blocked={}",
                mode.format,
                mode.parser_mode,
                mode.runs,
                mode.healthy_runs,
                success_rate,
                mode.schema_invalid,
                mode.timed_out,
                mode.policy_blocked
            );
        }
    }
    println!("field_population:");
    for line in &stats.lines {
        println!("- {line}");
    }
    println!("contract_drift:");
    println!(
        "- new_keys_second_half: {}",
        if stats.new_in_second.is_empty() {
            "<none>".to_string()
        } else {
            stats.new_in_second.join(",")
        }
    );
    println!(
        "- missing_keys_second_half: {}",
        if stats.missing_in_second.is_empty() {
            "<none>".to_string()
        } else {
            stats.missing_in_second.join(",")
        }
    );
}

fn print_stats_json(log_file: &Path, rows: &[Value], stats: &StatsComputed) -> i32 {
    let fields: Vec<Value> = REQUIRED_STRICT_FIELDS
        .iter()
        .map(|field| {
            let (present, non_null) = field_population(rows, field);
            json!({
                "field": field,
                "present": present,
                "non_null": non_null,
                "total": rows.len()
            })
        })
        .collect();
    let experiment_caps = selected_tq_caps();
    let latest_run = latest_run_all_sum();
    let latest_wave = latest_wave_sum();
    let task_execution = exec_diag_value(latest_run.as_ref(), latest_wave.as_ref());
    let phase7_metrics = phase7_metrics_value(20);
    let adapter_rollout_policy = adapter_policy_value();
    let payload = json!({
        "contract_version": TELEMETRY_JSON_CONTRACT_VERSION,
        "log_file": log_file.display().to_string(),
        "window_runs": rows.len(),
        "required_fields": REQUIRED_STRICT_FIELDS.len(),
        "severity": stats.severity,
        "strict_violations": stats.strict_violations,
        "normalization": {
            "modern_rows": stats.normalization.modern_rows,
            "legacy_rows": stats.normalization.legacy_rows,
            "migrated_legacy_rows": stats.normalization.migrated_legacy_rows,
            "recommendation": if stats.normalization.legacy_rows > 0 {
                "run `cx logs migrate`"
            } else {
                "none"
            }
        },
        "backend_capabilities": {
            "turboquant": {
                "cx_runtime_support": experiment_caps.turboquant_runtime_support,
                "selected_backend_role": experiment_caps.turboquant_backend_role,
                "memory_metric_kind": experiment_caps.turboquant_metric_kind,
            }
        },
        "adapter_rollout_policy": adapter_rollout_policy,
        "task_execution": task_execution,
        "phase7_metrics": phase7_metrics,
        "fields": fields,
        "contract_drift": {
            "new_keys_second_half": stats.new_in_second,
            "missing_keys_second_half": stats.missing_in_second
        },
        "capture_prompt_telemetry": {
            "rows_with_explicit_profile": stats.capture_prompt.rows_with_explicit_profile,
            "shadow_narrow_configured_runs": stats.capture_prompt.shadow_narrow_configured_runs,
            "shadow_narrow_applied_runs": stats.capture_prompt.shadow_narrow_applied_runs,
            "shadow_narrow_fallback_runs": stats.capture_prompt.shadow_narrow_fallback_runs,
            "applied_reducer_kinds": stats.capture_prompt.applied_reducer_kinds.iter().map(|(reducer_kind, runs)| {
                json!({
                    "reducer_kind": reducer_kind,
                    "runs": runs
                })
            }).collect::<Vec<Value>>(),
            "fallback_reasons": stats.capture_prompt.fallback_reasons.iter().map(|(reason, runs)| {
                json!({
                    "reason": reason,
                    "runs": runs
                })
            }).collect::<Vec<Value>>()
        },
        "retry_telemetry": {
            "rows_with_retry_metadata": stats.retry.rows_with_retry_metadata,
            "rows_after_retry": stats.retry.rows_after_retry,
            "rows_after_retry_success": stats.retry.rows_after_retry_success,
            "rows_after_retry_success_rate": stats.retry.rows_after_retry_success_rate,
            "tasks_with_retry": stats.retry.tasks_with_retry,
            "tasks_retry_recovered": stats.retry.tasks_retry_recovered,
            "tasks_retry_recovery_rate": stats.retry.tasks_retry_recovery_rate,
            "attempt_histogram": stats.retry.attempt_histogram
        },
        "critical_telemetry": {
            "summary_rows": stats.critical.summary_rows,
            "halt_enabled_rows": stats.critical.halt_enabled_rows,
            "halted_rows": stats.critical.halted_rows,
            "critical_errors_total": stats.critical.critical_errors_total,
            "runs_with_critical_errors": stats.critical.runs_with_critical_errors
        },
        "timing_telemetry": {
            "task_rows": stats.timing.task_rows,
            "rows_with_worker_id": stats.timing.rows_with_worker_id,
            "rows_with_queue_ms": stats.timing.rows_with_queue_ms,
            "rows_with_wave_index": stats.timing.rows_with_wave_index,
            "rows_with_wave_mode": stats.timing.rows_with_wave_mode,
            "rows_with_wave_size": stats.timing.rows_with_wave_size,
            "rows_with_queue_started_at": stats.timing.rows_with_queue_started_at,
            "rows_with_task_started_at": stats.timing.rows_with_task_started_at,
            "rows_with_task_finished_at": stats.timing.rows_with_task_finished_at
        },
        "http_mode_stats": stats.http_mode_stats.iter().map(|m| {
            let success_rate = if m.runs == 0 {
                0.0
            } else {
                m.healthy_runs as f64 / m.runs as f64
            };
            json!({
                "format": m.format,
                "parser_mode": m.parser_mode,
                "runs": m.runs,
                "healthy_runs": m.healthy_runs,
                "success_rate": success_rate,
                "schema_invalid": m.schema_invalid,
                "timed_out": m.timed_out,
                "policy_blocked": m.policy_blocked
            })
        }).collect::<Vec<Value>>()
    });
    match serde_json::to_string_pretty(&payload) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!("{} logs stats: failed to render json: {e}", cli_app_name());
            1
        }
    }
}

pub fn handle_stats(app_name: &str, args: &[String]) -> i32 {
    let parsed = match parse_stats_args(app_name, args) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let Some(log_file) = resolve_log_file() else {
        crate::cx_eprintln!("{app_name} logs stats: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!(
            "{app_name} logs stats: no log file at {}",
            log_file.display()
        );
        return 0;
    }
    let rows = match load_values(&log_file, parsed.n) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{app_name} logs stats: {e}");
            return 1;
        }
    };
    let stats = compute_stats(&rows);
    if parsed.json_out {
        let code = print_stats_json(&log_file, &rows, &stats);
        if code != 0 {
            return code;
        }
    } else {
        print_stats_human(app_name, &log_file, &rows, &stats, parsed.severity);
    }
    if parsed.strict && stats.strict_violations > 0 {
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_mixed_rows() {
        let mut modern = serde_json::Map::new();
        for field in REQUIRED_STRICT_FIELDS {
            modern.insert(field.to_string(), Value::Null);
        }
        modern.insert("execution_mode".to_string(), json!("lean"));
        let mut migrated = modern.clone();
        migrated.insert("execution_mode".to_string(), json!("legacy_migrated"));
        let rows = vec![
            json!(modern),
            json!(migrated),
            json!({"command": "cx"}),
            Value::Null,
        ];
        let stats = compute_stats(&rows);
        assert_eq!(stats.normalization.modern_rows, 2);
        assert_eq!(stats.normalization.legacy_rows, 1);
        assert_eq!(stats.normalization.migrated_legacy_rows, 1);
        assert_eq!(stats.capture_prompt.rows_with_explicit_profile, 0);
    }
}

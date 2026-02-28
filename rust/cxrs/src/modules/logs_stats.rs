use crate::log_contract::REQUIRED_STRICT_FIELDS;
use crate::logs::load_values;
use crate::paths::resolve_log_file;
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
    let mut json_out = false;
    let mut strict = false;
    let mut severity = false;
    for a in args.iter().skip(1) {
        if a == "--json" {
            json_out = true;
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
                    "Usage: {app_name} logs stats [N] [--json] [--strict] [--severity]"
                );
                return Err(2);
            }
        }
    }
    Ok(StatsArgs {
        n,
        json_out,
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
    retry: RetryStats,
    http_mode_stats: Vec<HttpModeStat>,
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
    let retry = compute_retry_stats(rows);
    let http_mode_stats = compute_http_mode_stats(rows);
    StatsComputed {
        lines,
        strict_violations,
        new_in_second,
        missing_in_second,
        severity,
        retry,
        http_mode_stats,
    }
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
    if severity_only {
        return;
    }
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
    let payload = json!({
        "log_file": log_file.display().to_string(),
        "window_runs": rows.len(),
        "required_fields": REQUIRED_STRICT_FIELDS.len(),
        "severity": stats.severity,
        "strict_violations": stats.strict_violations,
        "fields": fields,
        "contract_drift": {
            "new_keys_second_half": stats.new_in_second,
            "missing_keys_second_half": stats.missing_in_second
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
            crate::cx_eprintln!("cxrs logs stats: failed to render json: {e}");
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

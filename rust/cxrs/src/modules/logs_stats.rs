use crate::paths::resolve_log_file;
use crate::logs::load_values;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

const REQUIRED_STRICT_FIELDS: [&str; 26] = [
    "execution_id",
    "timestamp",
    "command",
    "backend_used",
    "capture_provider",
    "execution_mode",
    "duration_ms",
    "schema_enforced",
    "schema_valid",
    "quarantine_id",
    "task_id",
    "system_output_len_raw",
    "system_output_len_processed",
    "system_output_len_clipped",
    "system_output_lines_raw",
    "system_output_lines_processed",
    "system_output_lines_clipped",
    "input_tokens",
    "cached_input_tokens",
    "effective_input_tokens",
    "output_tokens",
    "policy_blocked",
    "policy_reason",
    "timed_out",
    "timeout_secs",
    "command_label",
];

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
    StatsComputed {
        lines,
        strict_violations,
        new_in_second,
        missing_in_second,
        severity,
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
        }
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

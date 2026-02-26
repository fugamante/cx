use super::logs_read::{LogValidateOutcome, REQUIRED_STRICT_FIELDS};
use super::{load_values, migrate_runs_jsonl, validate_runs_jsonl_file};
use crate::paths::resolve_log_file;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

struct MigrateArgs {
    out_path: Option<PathBuf>,
    in_place: bool,
}

struct StatsArgs {
    n: usize,
    json_out: bool,
}

fn parse_migrate_args(app_name: &str, args: &[String]) -> Result<MigrateArgs, i32> {
    let mut out_path: Option<PathBuf> = None;
    let mut in_place = false;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("Usage: {app_name} logs migrate [--out PATH] [--in-place]");
                    return Err(2);
                };
                out_path = Some(PathBuf::from(v));
                i += 2;
            }
            "--in-place" => {
                in_place = true;
                i += 1;
            }
            other => {
                crate::cx_eprintln!("{app_name} logs migrate: unknown flag '{other}'");
                crate::cx_eprintln!("Usage: {app_name} logs migrate [--out PATH] [--in-place]");
                return Err(2);
            }
        }
    }
    Ok(MigrateArgs { out_path, in_place })
}

fn print_validate_summary(app_name: &str, log_file: &Path, outcome: &LogValidateOutcome) {
    println!("== {app_name} logs validate ==");
    println!("log_file: {}", log_file.display());
    println!("entries_scanned: {}", outcome.total);
    println!(
        "legacy_ok: {}",
        if outcome.legacy_ok { "true" } else { "false" }
    );
    if outcome.legacy_ok {
        println!("legacy_entries: {}", outcome.legacy_lines);
    }
    println!("corrupted_entries: {}", outcome.corrupted_lines.len());
    println!("issue_count: {}", outcome.issues.len());
    println!("invalid_json_entries: {}", outcome.invalid_json_lines);
}

fn print_validate_issues(outcome: &LogValidateOutcome) {
    for issue in outcome.issues.iter().take(20) {
        println!("- {issue}");
    }
    if outcome.issues.len() > 20 {
        println!("- ... and {} more", outcome.issues.len() - 20);
    }
}

fn validate_outcome_status(outcome: &LogValidateOutcome) -> i32 {
    if outcome.issues.is_empty() {
        println!("status: ok");
        return 0;
    }
    print_validate_issues(outcome);
    if outcome.legacy_ok && outcome.invalid_json_lines == 0 {
        println!("status: ok_with_warnings");
        return 0;
    }
    1
}

fn handle_validate(app_name: &str, args: &[String]) -> i32 {
    let legacy_ok = args.iter().any(|a| a == "--legacy-ok");
    let Some(log_file) = resolve_log_file() else {
        crate::cx_eprintln!("{app_name} logs validate: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!(
            "{app_name} logs validate: no log file at {}",
            log_file.display()
        );
        return 0;
    }
    let outcome = match validate_runs_jsonl_file(&log_file, legacy_ok) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{app_name} logs validate: {e}");
            return 1;
        }
    };
    print_validate_summary(app_name, &log_file, &outcome);
    validate_outcome_status(&outcome)
}

fn migrate_in_place(app_name: &str, log_file: &Path, target: &Path) -> Result<(), i32> {
    let bak = log_file.with_extension(format!(
        "jsonl.bak.{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    if let Err(e) = fs::copy(log_file, &bak) {
        crate::cx_eprintln!(
            "{app_name} logs migrate: failed to backup {} -> {}: {e}",
            log_file.display(),
            bak.display()
        );
        return Err(1);
    }
    if let Err(e) = fs::rename(target, log_file) {
        crate::cx_eprintln!(
            "{app_name} logs migrate: failed to replace {} with {}: {e}",
            log_file.display(),
            target.display()
        );
        crate::cx_eprintln!("backup: {}", bak.display());
        return Err(1);
    }
    println!("backup: {}", bak.display());
    println!("status: replaced");
    Ok(())
}

fn handle_migrate(app_name: &str, args: &[String]) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        crate::cx_eprintln!("{app_name} logs migrate: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        crate::cx_eprintln!(
            "{app_name} logs migrate: no log file at {}",
            log_file.display()
        );
        return 1;
    }
    let parsed = match parse_migrate_args(app_name, args) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let target = parsed.out_path.unwrap_or_else(|| {
        log_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("runs.migrated.jsonl")
    });

    println!("== {app_name} logs migrate ==");
    println!("in: {}", log_file.display());
    println!("out: {}", target.display());
    let summary = match migrate_runs_jsonl(&log_file, &target) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{app_name} logs migrate: {e}");
            return 1;
        }
    };

    println!("entries_in: {}", summary.entries_in);
    println!("entries_out: {}", summary.entries_out);
    println!("invalid_json_skipped: {}", summary.invalid_json_skipped);
    println!("legacy_normalized: {}", summary.legacy_normalized);
    println!("modern_normalized: {}", summary.modern_normalized);

    if parsed.in_place {
        return match migrate_in_place(app_name, &log_file, &target) {
            Ok(()) => 0,
            Err(code) => code,
        };
    }
    println!("status: wrote");
    0
}

fn parse_stats_args(app_name: &str, args: &[String]) -> Result<StatsArgs, i32> {
    let mut n = 200usize;
    let mut json_out = false;
    for a in args.iter().skip(1) {
        if a == "--json" {
            json_out = true;
            continue;
        }
        match a.parse::<usize>() {
            Ok(v) if v > 0 => n = v,
            _ => {
                crate::cx_eprintln!("Usage: {app_name} logs stats [N] [--json]");
                return Err(2);
            }
        }
    }
    Ok(StatsArgs { n, json_out })
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

fn print_stats_human(app_name: &str, log_file: &Path, rows: &[Value]) {
    let (new_in_second, missing_in_second) = drift_sets(rows);
    println!("== {app_name} logs stats ==");
    println!("log_file: {}", log_file.display());
    println!("window_runs: {}", rows.len());
    println!("required_fields: {}", REQUIRED_STRICT_FIELDS.len());
    println!("field_population:");
    for line in coverage_lines(rows) {
        println!("- {line}");
    }
    println!("contract_drift:");
    println!(
        "- new_keys_second_half: {}",
        if new_in_second.is_empty() {
            "<none>".to_string()
        } else {
            new_in_second.join(",")
        }
    );
    println!(
        "- missing_keys_second_half: {}",
        if missing_in_second.is_empty() {
            "<none>".to_string()
        } else {
            missing_in_second.join(",")
        }
    );
}

fn print_stats_json(log_file: &Path, rows: &[Value]) -> i32 {
    let (new_in_second, missing_in_second) = drift_sets(rows);
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
        "fields": fields,
        "contract_drift": {
            "new_keys_second_half": new_in_second,
            "missing_keys_second_half": missing_in_second
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

fn handle_stats(app_name: &str, args: &[String]) -> i32 {
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
    if parsed.json_out {
        return print_stats_json(&log_file, &rows);
    }
    print_stats_human(app_name, &log_file, &rows);
    0
}

pub fn cmd_logs(app_name: &str, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("validate") {
        "validate" => handle_validate(app_name, args),
        "migrate" => handle_migrate(app_name, args),
        "stats" => handle_stats(app_name, args),
        other => {
            crate::cx_eprintln!(
                "Usage: {app_name} logs <validate|migrate|stats> (unknown subcommand: {other})"
            );
            2
        }
    }
}

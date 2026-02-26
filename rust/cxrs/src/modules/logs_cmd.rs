use super::logs_read::LogValidateOutcome;
use super::{migrate_runs_jsonl, validate_runs_jsonl_file};
use crate::paths::resolve_log_file;
use std::fs;
use std::path::{Path, PathBuf};

struct MigrateArgs {
    out_path: Option<PathBuf>,
    in_place: bool,
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

pub fn cmd_logs(app_name: &str, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("validate") {
        "validate" => handle_validate(app_name, args),
        "migrate" => handle_migrate(app_name, args),
        "stats" => crate::logs_stats::handle_stats(app_name, args),
        other => {
            crate::cx_eprintln!(
                "Usage: {app_name} logs <validate|migrate|stats> (unknown subcommand: {other})"
            );
            2
        }
    }
}

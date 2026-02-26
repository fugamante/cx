use super::{migrate_runs_jsonl, validate_runs_jsonl_file};
use crate::paths::resolve_log_file;
use std::fs;
use std::path::Path;

pub fn cmd_logs(app_name: &str, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("validate") {
        "validate" => {
            let legacy_ok = args.iter().any(|a| a == "--legacy-ok");
            let Some(log_file) = resolve_log_file() else {
                eprintln!("{app_name} logs validate: unable to resolve log file");
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
                    eprintln!("{app_name} logs validate: {e}");
                    return 1;
                }
            };
            println!("== {app_name} logs validate ==");
            println!("log_file: {}", log_file.display());
            println!("entries_scanned: {}", outcome.total);
            if outcome.legacy_ok {
                println!("legacy_ok: true");
                println!("legacy_entries: {}", outcome.legacy_lines);
            } else {
                println!("legacy_ok: false");
            }
            println!("corrupted_entries: {}", outcome.corrupted_lines.len());
            println!("issue_count: {}", outcome.issues.len());
            println!("invalid_json_entries: {}", outcome.invalid_json_lines);
            if !outcome.issues.is_empty() {
                for issue in outcome.issues.iter().take(20) {
                    println!("- {issue}");
                }
                if outcome.issues.len() > 20 {
                    println!("- ... and {} more", outcome.issues.len() - 20);
                }
                if outcome.legacy_ok && outcome.invalid_json_lines == 0 {
                    println!("status: ok_with_warnings");
                    return 0;
                }
                return 1;
            }
            println!("status: ok");
            0
        }
        "migrate" => {
            let Some(log_file) = resolve_log_file() else {
                eprintln!("{app_name} logs migrate: unable to resolve log file");
                return 1;
            };
            if !log_file.exists() {
                eprintln!(
                    "{app_name} logs migrate: no log file at {}",
                    log_file.display()
                );
                return 1;
            }

            let mut out_path: Option<std::path::PathBuf> = None;
            let mut in_place = false;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--out" => {
                        let Some(v) = args.get(i + 1) else {
                            eprintln!("Usage: {app_name} logs migrate [--out PATH] [--in-place]");
                            return 2;
                        };
                        out_path = Some(std::path::PathBuf::from(v));
                        i += 2;
                    }
                    "--in-place" => {
                        in_place = true;
                        i += 1;
                    }
                    other => {
                        eprintln!("{app_name} logs migrate: unknown flag '{other}'");
                        eprintln!("Usage: {app_name} logs migrate [--out PATH] [--in-place]");
                        return 2;
                    }
                }
            }

            let target = out_path.unwrap_or_else(|| {
                log_file
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("runs.migrated.jsonl")
            });

            println!("== {app_name} logs migrate ==");
            println!("in: {}", log_file.display());
            println!("out: {}", target.display());
            match migrate_runs_jsonl(&log_file, &target) {
                Ok(summary) => {
                    println!("entries_in: {}", summary.entries_in);
                    println!("entries_out: {}", summary.entries_out);
                    println!("invalid_json_skipped: {}", summary.invalid_json_skipped);
                    println!("legacy_normalized: {}", summary.legacy_normalized);
                    println!("modern_normalized: {}", summary.modern_normalized);

                    if in_place {
                        let bak = log_file.with_extension(format!(
                            "jsonl.bak.{}",
                            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
                        ));
                        if let Err(e) = fs::copy(&log_file, &bak) {
                            eprintln!(
                                "{app_name} logs migrate: failed to backup {} -> {}: {e}",
                                log_file.display(),
                                bak.display()
                            );
                            return 1;
                        }
                        if let Err(e) = fs::rename(&target, &log_file) {
                            eprintln!(
                                "{app_name} logs migrate: failed to replace {} with {}: {e}",
                                log_file.display(),
                                target.display()
                            );
                            eprintln!("backup: {}", bak.display());
                            return 1;
                        }
                        println!("backup: {}", bak.display());
                        println!("status: replaced");
                    } else {
                        println!("status: wrote");
                    }
                    0
                }
                Err(e) => {
                    eprintln!("{app_name} logs migrate: {e}");
                    1
                }
            }
        }
        other => {
            eprintln!("Usage: {app_name} logs <validate|migrate> (unknown subcommand: {other})");
            2
        }
    }
}

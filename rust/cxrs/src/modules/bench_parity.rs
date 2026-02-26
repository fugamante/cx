use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use crate::analytics::parse_ts_epoch;
use crate::diagnostics::{has_required_log_fields, last_appended_json_value};
use crate::logs::{file_len, load_runs_appended};
use crate::paths::{repo_root_hint, resolve_log_file};
use crate::routing::{bash_function_names, route_handler_for};
use crate::types::RunEntry;

fn run_command_for_bench(
    command: &[String],
    disable_cx_log: bool,
    passthru: bool,
) -> Result<i32, String> {
    if command.is_empty() {
        return Err("missing command".to_string());
    }
    let mut cmd = Command::new(&command[0]);
    if command.len() > 1 {
        cmd.args(&command[1..]);
    }
    if disable_cx_log {
        cmd.env("CXLOG_ENABLED", "0");
    }
    let output = cmd
        .output()
        .map_err(|e| format!("failed to execute '{}': {e}", command[0]))?;
    if passthru {
        let mut out = std::io::stdout();
        let mut err = std::io::stderr();
        let _ = out.write_all(&output.stdout);
        let _ = err.write_all(&output.stderr);
    }
    Ok(output.status.code().unwrap_or(1))
}

pub fn cmd_bench(app_name: &str, runs: usize, command: &[String]) -> i32 {
    if runs == 0 {
        eprintln!("cxrs bench: runs must be > 0");
        return 2;
    }
    if command.is_empty() {
        eprintln!("Usage: {app_name} bench <runs> -- <command...>");
        return 2;
    }

    let disable_cx_log = env::var("CXBENCH_LOG").ok().as_deref() == Some("0");
    let passthru = env::var("CXBENCH_PASSTHRU").ok().as_deref() == Some("1");
    let log_file = resolve_log_file();
    let mut durations: Vec<u64> = Vec::with_capacity(runs);
    let mut eff_totals: Vec<u64> = Vec::new();
    let mut out_totals: Vec<u64> = Vec::new();
    let mut failures = 0usize;
    let mut prompt_hash_matched = 0usize;
    let mut appended_row_total = 0usize;

    for _ in 0..runs {
        let before_offset = if let Some(path) = &log_file {
            if path.exists() { file_len(path) } else { 0 }
        } else {
            0
        };

        let started = Instant::now();
        let started_epoch = Utc::now().timestamp();
        let code = match run_command_for_bench(command, disable_cx_log, passthru) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("cxrs bench: {e}");
                return 1;
            }
        };
        let ended_epoch = Utc::now().timestamp();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        durations.push(elapsed_ms);
        if code != 0 {
            failures += 1;
        }

        if let Some(path) = &log_file {
            if path.exists() && !disable_cx_log {
                let appended = load_runs_appended(path, before_offset).unwrap_or_default();
                if !appended.is_empty() {
                    let windowed: Vec<RunEntry> = appended
                        .into_iter()
                        .filter(|r| {
                            let Some(ts) = r.ts.as_deref() else {
                                return true;
                            };
                            let Some(epoch) = parse_ts_epoch(ts) else {
                                return true;
                            };
                            epoch >= started_epoch.saturating_sub(1)
                                && epoch <= ended_epoch.saturating_add(1)
                        })
                        .collect();
                    appended_row_total += windowed.len();

                    let mut hash_counts: HashMap<String, usize> = HashMap::new();
                    for r in &windowed {
                        if let Some(h) = r.prompt_sha256.as_deref() {
                            if !h.is_empty() {
                                *hash_counts.entry(h.to_string()).or_insert(0) += 1;
                            }
                        }
                    }

                    let preferred_hash = hash_counts.into_iter().max_by(|a, b| a.1.cmp(&b.1));
                    let correlated: Vec<&RunEntry> = if let Some((h, _)) = preferred_hash {
                        prompt_hash_matched += 1;
                        windowed
                            .iter()
                            .filter(|r| r.prompt_sha256.as_deref() == Some(h.as_str()))
                            .collect()
                    } else {
                        windowed.iter().collect()
                    };

                    if !correlated.is_empty() {
                        let mut eff_sum = 0u64;
                        let mut out_sum = 0u64;
                        let mut any_eff = false;
                        let mut any_out = false;
                        for r in correlated {
                            if let Some(v) = r.effective_input_tokens {
                                eff_sum += v;
                                any_eff = true;
                            }
                            if let Some(v) = r.output_tokens {
                                out_sum += v;
                                any_out = true;
                            }
                        }
                        if any_eff {
                            eff_totals.push(eff_sum);
                        }
                        if any_out {
                            out_totals.push(out_sum);
                        }
                    }
                }
            }
        }
    }

    let min = durations.iter().min().copied().unwrap_or(0);
    let max = durations.iter().max().copied().unwrap_or(0);
    let sum: u64 = durations.iter().sum();
    let avg = if durations.is_empty() {
        0
    } else {
        sum / (durations.len() as u64)
    };

    println!("== cxrs bench ==");
    println!("runs: {runs}");
    println!("command: {}", command.join(" "));
    println!("duration_ms avg/min/max: {avg}/{min}/{max}");
    println!("failures: {failures}");
    if !eff_totals.is_empty() {
        let eff_avg = eff_totals.iter().sum::<u64>() / (eff_totals.len() as u64);
        println!("avg effective_input_tokens: {eff_avg}");
    } else {
        println!("avg effective_input_tokens: n/a");
    }
    if !out_totals.is_empty() {
        let out_avg = out_totals.iter().sum::<u64>() / (out_totals.len() as u64);
        println!("avg output_tokens: {out_avg}");
    } else {
        println!("avg output_tokens: n/a");
    }
    if disable_cx_log {
        println!("cxbench_log: disabled (CXBENCH_LOG=0)");
    } else {
        println!("cxbench_log: enabled");
    }
    println!(
        "cxbench_passthru: {}",
        if passthru { "enabled" } else { "disabled" }
    );
    if !disable_cx_log {
        println!(
            "cxbench_correlation: prompt_hash_matches={}/{} runs, appended_rows={}",
            prompt_hash_matched, runs, appended_row_total
        );
    }

    if failures > 0 { 1 } else { 0 }
}

pub fn cmd_parity() -> i32 {
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cxparity: cannot resolve current executable: {e}");
            return 1;
        }
    };
    let budget_chars = env::var("CX_CONTEXT_BUDGET_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(12000);

    #[derive(Default)]
    struct Row {
        cmd: String,
        rust_ok: bool,
        bash_ok: bool,
        json_ok: bool,
        logs_ok: bool,
        budget_ok: bool,
        checked: bool,
    }

    let mut rows: Vec<Row> = Vec::new();
    let mut pass_all = true;
    let ts = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let temp_repo = env::temp_dir().join(format!("cxparity-{}-{}", std::process::id(), ts));
    if fs::create_dir_all(&temp_repo).is_err() {
        eprintln!(
            "cxparity: failed to create temp repo {}",
            temp_repo.display()
        );
        return 1;
    }
    let init_ok = Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&temp_repo)
        .status()
        .ok()
        .is_some_and(|s| s.success());
    if !init_ok {
        eprintln!("cxparity: git init failed in {}", temp_repo.display());
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }
    let stage_file = temp_repo.join("cxparity_tmp.txt");
    if fs::write(&stage_file, "cx parity staged change\n").is_err() {
        eprintln!("cxparity: failed to write staged file");
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }
    let stage_ok = Command::new("git")
        .arg("add")
        .arg("cxparity_tmp.txt")
        .current_dir(&temp_repo)
        .status()
        .ok()
        .is_some_and(|s| s.success());
    if !stage_ok {
        eprintln!("cxparity: git add failed");
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }
    let temp_log_file = temp_repo.join(".codex").join("cxlogs").join("runs.jsonl");
    let bash_funcs = bash_function_names(&repo);
    let parity_catalog: Vec<(&str, Vec<&str>, Option<Vec<&str>>)> = vec![
        ("cxo", vec!["echo", "hi"], None),
        (
            "cxcommitjson",
            vec![],
            Some(vec!["subject", "body", "breaking", "tests"]),
        ),
    ];
    let overlap: Vec<(&str, Vec<&str>, Option<Vec<&str>>)> = parity_catalog
        .into_iter()
        .filter(|(cmd, _, _)| {
            route_handler_for(cmd).is_some() && bash_funcs.iter().any(|f| f == cmd)
        })
        .collect();
    if overlap.is_empty() {
        eprintln!("cxparity: no overlap commands found");
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }

    for (cmd, args, schema_keys) in overlap {
        let mut row = Row {
            cmd: cmd.to_string(),
            ..Row::default()
        };
        row.checked = true;
        let before_rust = file_len(&temp_log_file);
        let rust_out = Command::new(&exe)
            .arg("cx-compat")
            .arg(cmd)
            .args(&args)
            .current_dir(&temp_repo)
            .env("CX_EXECUTION_PATH", "rust:cxparity")
            .output();
        let rust_ok = rust_out.as_ref().is_ok_and(|o| o.status.success());
        row.rust_ok = rust_ok;
        let rust_stdout = rust_out
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let rust_json_ok = if let Some(keys) = schema_keys.as_ref() {
            if let Ok(v) = serde_json::from_str::<Value>(rust_stdout.trim()) {
                keys.iter().all(|k| v.get(*k).is_some())
            } else {
                false
            }
        } else {
            true
        };
        let rust_row = last_appended_json_value(&temp_log_file, before_rust);
        let rust_budget_ok = rust_stdout.chars().count() <= budget_chars
            || rust_row
                .as_ref()
                .and_then(|v| v.get("clipped"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let rust_log_ok = rust_row.as_ref().is_some_and(has_required_log_fields);

        let before_bash = file_len(&temp_log_file);
        let bash_cmd = format!(
            "source '{}' >/dev/null 2>&1; {} {}",
            repo.join("cx.sh").display(),
            cmd,
            args.join(" ")
        );
        let bash_out = Command::new("bash")
            .arg("-lc")
            .arg(bash_cmd)
            .current_dir(&temp_repo)
            .env("CX_EXECUTION_PATH", "bash:cxparity")
            .output();
        let bash_ok = bash_out.as_ref().is_ok_and(|o| o.status.success());
        row.bash_ok = bash_ok;
        let bash_stdout = bash_out
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let bash_json_ok = if let Some(keys) = schema_keys.as_ref() {
            if let Ok(v) = serde_json::from_str::<Value>(bash_stdout.trim()) {
                keys.iter().all(|k| v.get(*k).is_some())
            } else {
                false
            }
        } else {
            true
        };
        let bash_row = last_appended_json_value(&temp_log_file, before_bash);
        let bash_budget_ok = bash_stdout.chars().count() <= budget_chars
            || bash_row
                .as_ref()
                .and_then(|v| v.get("clipped"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let bash_log_ok = bash_row.as_ref().is_some_and(has_required_log_fields);

        row.json_ok = rust_json_ok && bash_json_ok;
        row.logs_ok = rust_log_ok && bash_log_ok;
        row.budget_ok = rust_budget_ok && bash_budget_ok;

        let row_pass = row.rust_ok && row.bash_ok && row.json_ok && row.logs_ok && row.budget_ok;
        if !row_pass {
            pass_all = false;
            eprintln!(
                "cxparity: FAIL {} rust_ok={} bash_ok={} json_ok={} logs_ok={} budget_ok={}",
                row.cmd, row.rust_ok, row.bash_ok, row.json_ok, row.logs_ok, row.budget_ok
            );
        }
        rows.push(row);
    }
    let _ = fs::remove_dir_all(&temp_repo);

    println!("cmd | rust | bash | json | logs | budget | result");
    println!("--- | --- | --- | --- | --- | --- | ---");
    for row in rows {
        let result = if row.checked
            && row.rust_ok
            && row.bash_ok
            && row.json_ok
            && row.logs_ok
            && row.budget_ok
        {
            "PASS"
        } else {
            "FAIL"
        };
        println!(
            "{} | {} | {} | {} | {} | {} | {}",
            row.cmd, row.rust_ok, row.bash_ok, row.json_ok, row.logs_ok, row.budget_ok, result
        );
    }
    if pass_all { 0 } else { 1 }
}

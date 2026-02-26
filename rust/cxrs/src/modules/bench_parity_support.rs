use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::analytics::parse_ts_epoch;
use crate::diagnostics::{has_required_log_fields, last_appended_json_value};
use crate::logs::load_runs_appended;
use crate::types::RunEntry;

#[derive(Default)]
pub struct BenchStats {
    pub durations: Vec<u64>,
    pub eff_totals: Vec<u64>,
    pub out_totals: Vec<u64>,
    pub failures: usize,
    pub prompt_hash_matched: usize,
    pub appended_row_total: usize,
}

#[derive(Default)]
pub struct ParityRow {
    pub cmd: String,
    pub rust_ok: bool,
    pub bash_ok: bool,
    pub json_ok: bool,
    pub logs_ok: bool,
    pub budget_ok: bool,
    pub checked: bool,
}

fn correlated_rows(windowed: &[RunEntry]) -> (Vec<&RunEntry>, bool) {
    let mut hash_counts: HashMap<String, usize> = HashMap::new();
    for r in windowed {
        if let Some(h) = r.prompt_sha256.as_deref()
            && !h.is_empty()
        {
            *hash_counts.entry(h.to_string()).or_insert(0) += 1;
        }
    }
    let preferred_hash = hash_counts.into_iter().max_by(|a, b| a.1.cmp(&b.1));
    if let Some((h, _)) = preferred_hash {
        let rows = windowed
            .iter()
            .filter(|r| r.prompt_sha256.as_deref() == Some(h.as_str()))
            .collect();
        return (rows, true);
    }
    (windowed.iter().collect(), false)
}

pub fn maybe_collect_tokens(
    stats: &mut BenchStats,
    log_file: &Option<PathBuf>,
    before_offset: u64,
    started_epoch: i64,
    ended_epoch: i64,
    disable_cx_log: bool,
) {
    let Some(path) = log_file else { return };
    if !path.exists() || disable_cx_log {
        return;
    }
    let appended = load_runs_appended(path, before_offset).unwrap_or_default();
    if appended.is_empty() {
        return;
    }
    let windowed: Vec<RunEntry> = appended
        .into_iter()
        .filter(|r| {
            let Some(ts) = r.ts.as_deref() else {
                return true;
            };
            let Some(epoch) = parse_ts_epoch(ts) else {
                return true;
            };
            epoch >= started_epoch.saturating_sub(1) && epoch <= ended_epoch.saturating_add(1)
        })
        .collect();
    stats.appended_row_total += windowed.len();
    let (correlated, hash_matched) = correlated_rows(&windowed);
    if hash_matched {
        stats.prompt_hash_matched += 1;
    }
    accumulate_token_sums(stats, correlated);
}

fn accumulate_token_sums(stats: &mut BenchStats, correlated: Vec<&RunEntry>) {
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
        stats.eff_totals.push(eff_sum);
    }
    if any_out {
        stats.out_totals.push(out_sum);
    }
}

fn avg_opt(values: &[u64]) -> Option<u64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<u64>() / (values.len() as u64))
    }
}

pub fn print_bench_summary(
    runs: usize,
    command: &[String],
    disable_cx_log: bool,
    passthru: bool,
    stats: &BenchStats,
) {
    let min = stats.durations.iter().min().copied().unwrap_or(0);
    let max = stats.durations.iter().max().copied().unwrap_or(0);
    let sum: u64 = stats.durations.iter().sum();
    let avg = if stats.durations.is_empty() {
        0
    } else {
        sum / (stats.durations.len() as u64)
    };
    println!("== cxrs bench ==");
    println!("runs: {runs}");
    println!("command: {}", command.join(" "));
    println!("duration_ms avg/min/max: {avg}/{min}/{max}");
    println!("failures: {}", stats.failures);
    if let Some(eff_avg) = avg_opt(&stats.eff_totals) {
        println!("avg effective_input_tokens: {eff_avg}");
    } else {
        println!("avg effective_input_tokens: n/a");
    }
    if let Some(out_avg) = avg_opt(&stats.out_totals) {
        println!("avg output_tokens: {out_avg}");
    } else {
        println!("avg output_tokens: n/a");
    }
    println!(
        "cxbench_log: {}",
        if disable_cx_log {
            "disabled (CXBENCH_LOG=0)"
        } else {
            "enabled"
        }
    );
    println!(
        "cxbench_passthru: {}",
        if passthru { "enabled" } else { "disabled" }
    );
    if !disable_cx_log {
        println!(
            "cxbench_correlation: prompt_hash_matches={}/{} runs, appended_rows={}",
            stats.prompt_hash_matched, runs, stats.appended_row_total
        );
    }
}

pub fn setup_temp_repo() -> Result<PathBuf, String> {
    let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let temp_repo = std::env::temp_dir().join(format!("cxparity-{}-{}", std::process::id(), ts));
    fs::create_dir_all(&temp_repo).map_err(|_| {
        format!(
            "cxparity: failed to create temp repo {}",
            temp_repo.display()
        )
    })?;
    let init_ok = Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&temp_repo)
        .status()
        .ok()
        .is_some_and(|s| s.success());
    if !init_ok {
        return Err(format!(
            "cxparity: git init failed in {}",
            temp_repo.display()
        ));
    }
    let stage_file = temp_repo.join("cxparity_tmp.txt");
    fs::write(&stage_file, "cx parity staged change\n")
        .map_err(|_| "cxparity: failed to write staged file".to_string())?;
    let stage_ok = Command::new("git")
        .arg("add")
        .arg("cxparity_tmp.txt")
        .current_dir(&temp_repo)
        .status()
        .ok()
        .is_some_and(|s| s.success());
    if !stage_ok {
        return Err("cxparity: git add failed".to_string());
    }
    Ok(temp_repo)
}

fn json_keys_ok(stdout: &str, schema_keys: &Option<Vec<&str>>) -> bool {
    let Some(keys) = schema_keys.as_ref() else {
        return true;
    };
    let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) else {
        return false;
    };
    keys.iter().all(|k| v.get(*k).is_some())
}

pub fn run_parity_path(
    out: std::process::Output,
    temp_log_file: &Path,
    before_offset: u64,
    budget_chars: usize,
    schema_keys: &Option<Vec<&str>>,
) -> (bool, bool, bool, bool) {
    let ok = out.status.success();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let row = last_appended_json_value(temp_log_file, before_offset);
    let json_ok = json_keys_ok(&stdout, schema_keys);
    let budget_ok = stdout.chars().count() <= budget_chars
        || row
            .as_ref()
            .and_then(|v| v.get("clipped"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let logs_ok = row.as_ref().is_some_and(has_required_log_fields);
    (ok, json_ok, logs_ok, budget_ok)
}

pub fn print_parity_table(rows: &[ParityRow]) {
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
}

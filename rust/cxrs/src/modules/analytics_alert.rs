use std::path::Path;

use crate::types::RunEntry;

use super::analytics_shared::{env_u64, load_runs_for};

fn print_alert_empty(n: usize, log_file: &Path) {
    println!("== cxrs alert (last {n} runs) ==");
    println!("Runs: 0");
    println!("Slow threshold violations: 0");
    println!("Token threshold violations: 0");
    println!("Avg cache hit rate: n/a");
    println!("Top 5 slowest: n/a");
    println!("Top 5 heaviest: n/a");
    println!("log_file: {}", log_file.display());
}

fn top_slowest(runs: &[RunEntry]) -> Vec<(u64, String, String)> {
    let mut slowest: Vec<(u64, String, String)> = runs
        .iter()
        .filter_map(|r| {
            r.duration_ms.map(|d| {
                (
                    d,
                    r.tool.clone().unwrap_or_else(|| "unknown".to_string()),
                    r.ts.clone().unwrap_or_else(|| "n/a".to_string()),
                )
            })
        })
        .collect();
    slowest.sort_by(|a, b| b.0.cmp(&a.0));
    slowest.truncate(5);
    slowest
}

fn print_top_runs(label: &str, empty_label: &str, rows: Vec<(u64, String, String)>, unit: &str) {
    if rows.is_empty() {
        println!("{empty_label}");
        return;
    }
    println!("{label}");
    for (value, tool, ts) in rows {
        println!("- {value}{unit} | {tool} | {ts}");
    }
}

fn top_heaviest(runs: &[RunEntry]) -> Vec<(u64, String, String)> {
    let mut heaviest: Vec<(u64, String, String)> = runs
        .iter()
        .filter_map(|r| {
            r.effective_input_tokens.map(|e| {
                (
                    e,
                    r.tool.clone().unwrap_or_else(|| "unknown".to_string()),
                    r.ts.clone().unwrap_or_else(|| "n/a".to_string()),
                )
            })
        })
        .collect();
    heaviest.sort_by(|a, b| b.0.cmp(&a.0));
    heaviest.truncate(5);
    heaviest
}

struct AlertHeaderStats {
    n: usize,
    runs_len: usize,
    max_ms: u64,
    max_eff: u64,
    slow_violations: usize,
    token_violations: usize,
    sum_in: u64,
    sum_cached: u64,
}

fn print_alert_header(s: &AlertHeaderStats) {
    println!("== cxrs alert (last {} runs) ==", s.n);
    println!("Runs: {}", s.runs_len);
    println!("Thresholds: max_ms={}, max_eff_in={}", s.max_ms, s.max_eff);
    println!("Slow threshold violations: {}", s.slow_violations);
    println!("Token threshold violations: {}", s.token_violations);
    match (s.sum_in > 0).then_some((s.sum_cached as f64 / s.sum_in as f64) * 100.0) {
        Some(v) => println!("Avg cache hit rate: {}%", v.round() as i64),
        None => println!("Avg cache hit rate: n/a"),
    }
}

fn collect_alert_stats(runs: &[RunEntry], max_ms: u64, max_eff: u64) -> (usize, usize, u64, u64) {
    let slow_violations = runs
        .iter()
        .filter(|r| r.duration_ms.unwrap_or(0) > max_ms)
        .count();
    let token_violations = runs
        .iter()
        .filter(|r| r.effective_input_tokens.unwrap_or(0) > max_eff)
        .count();
    let sum_in: u64 = runs.iter().map(|r| r.input_tokens.unwrap_or(0)).sum();
    let sum_cached: u64 = runs
        .iter()
        .map(|r| r.cached_input_tokens.unwrap_or(0))
        .sum();
    (slow_violations, token_violations, sum_in, sum_cached)
}

pub fn print_alert(n: usize) -> i32 {
    let (log_file, runs) = match load_runs_for("alert", n) {
        Ok(v) => v,
        Err(code) => return code,
    };
    if runs.is_empty() {
        print_alert_empty(n, &log_file);
        return 0;
    }

    let max_ms = env_u64("CXALERT_MAX_MS", 12000);
    let max_eff = env_u64("CXALERT_MAX_EFF_IN", 8000);
    let (slow_violations, token_violations, sum_in, sum_cached) =
        collect_alert_stats(&runs, max_ms, max_eff);

    let header = AlertHeaderStats {
        n,
        runs_len: runs.len(),
        max_ms,
        max_eff,
        slow_violations,
        token_violations,
        sum_in,
        sum_cached,
    };
    print_alert_header(&header);

    print_top_runs(
        "Top 5 slowest:",
        "Top 5 slowest: n/a",
        top_slowest(&runs),
        "ms",
    );
    print_top_runs(
        "Top 5 heaviest:",
        "Top 5 heaviest: n/a",
        top_heaviest(&runs),
        " effective tokens",
    );
    println!("log_file: {}", log_file.display());
    0
}

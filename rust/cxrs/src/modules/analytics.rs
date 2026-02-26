use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;

pub use crate::analytics_trace::print_trace;
pub use crate::analytics_worklog::print_worklog;
use crate::logs::load_runs;
use crate::paths::resolve_log_file;
use crate::types::RunEntry;

pub fn parse_ts_epoch(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp())
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(default)
}

fn print_json_value(prefix: &str, v: &Value) -> i32 {
    match serde_json::to_string_pretty(v) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            eprintln!("{prefix}: failed to render JSON: {e}");
            1
        }
    }
}

fn load_runs_for(command: &str, n: usize) -> Result<(std::path::PathBuf, Vec<RunEntry>), i32> {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return Err(1);
    };
    if !log_file.exists() {
        return Ok((log_file, Vec::new()));
    }
    match load_runs(&log_file, n) {
        Ok(v) => Ok((log_file, v)),
        Err(e) => {
            eprintln!("cxrs {command}: {e}");
            Err(1)
        }
    }
}

fn print_profile_empty(n: usize, log_file: &Path) {
    println!("== cxrs profile (last {n} runs) ==");
    println!("Runs: 0");
    println!("Avg duration: 0ms");
    println!("Avg effective tokens: 0");
    println!("Cache hit rate: n/a");
    println!("Output/input ratio: n/a");
    println!("Slowest run: n/a");
    println!("Heaviest context: n/a");
    println!("log_file: {}", log_file.display());
}

fn max_duration_tool(runs: &[RunEntry]) -> Option<(u64, String)> {
    runs.iter()
        .filter_map(|r| {
            r.duration_ms
                .map(|d| (d, r.tool.clone().unwrap_or_else(|| "unknown".to_string())))
        })
        .max_by_key(|(d, _)| *d)
}

fn max_eff_tool(runs: &[RunEntry]) -> Option<(u64, String)> {
    runs.iter()
        .filter_map(|r| {
            r.effective_input_tokens
                .map(|e| (e, r.tool.clone().unwrap_or_else(|| "unknown".to_string())))
        })
        .max_by_key(|(e, _)| *e)
}

pub fn print_profile(n: usize) -> i32 {
    let (log_file, runs) = match load_runs_for("profile", n) {
        Ok(v) => v,
        Err(code) => return code,
    };
    if runs.is_empty() {
        print_profile_empty(n, &log_file);
        return 0;
    }

    let total = runs.len() as u64;
    let sum_dur: u64 = runs.iter().map(|r| r.duration_ms.unwrap_or(0)).sum();
    let sum_eff: u64 = runs
        .iter()
        .map(|r| r.effective_input_tokens.unwrap_or(0))
        .sum();
    let sum_in: u64 = runs.iter().map(|r| r.input_tokens.unwrap_or(0)).sum();
    let sum_cached: u64 = runs
        .iter()
        .map(|r| r.cached_input_tokens.unwrap_or(0))
        .sum();
    let sum_out: u64 = runs.iter().map(|r| r.output_tokens.unwrap_or(0)).sum();

    println!("== cxrs profile (last {n} runs) ==");
    println!("Runs: {}", runs.len());
    println!("Avg duration: {}ms", sum_dur / total);
    println!("Avg effective tokens: {}", sum_eff / total);
    match (sum_in > 0).then_some(sum_cached as f64 / sum_in as f64) {
        Some(v) => println!("Cache hit rate: {}%", (v * 100.0).round() as i64),
        None => println!("Cache hit rate: n/a"),
    }
    match (sum_eff > 0).then_some(sum_out as f64 / sum_eff as f64) {
        Some(v) => println!("Output/input ratio: {:.2}", v),
        None => println!("Output/input ratio: n/a"),
    }
    match max_duration_tool(&runs) {
        Some((d, t)) => println!("Slowest run: {d}ms ({t})"),
        None => println!("Slowest run: n/a"),
    }
    match max_eff_tool(&runs) {
        Some((e, t)) => println!("Heaviest context: {e} effective tokens ({t})"),
        None => println!("Heaviest context: n/a"),
    }
    println!("log_file: {}", log_file.display());
    0
}

fn metrics_empty_json(log_file: &Path) -> Value {
    json!({
        "log_file": log_file.display().to_string(),
        "runs": 0,
        "avg_duration_ms": 0.0,
        "avg_input_tokens": 0.0,
        "avg_cached_input_tokens": 0.0,
        "avg_effective_input_tokens": 0.0,
        "avg_output_tokens": 0.0,
        "by_tool": []
    })
}

fn group_metrics_by_tool(runs: &[RunEntry]) -> Vec<Value> {
    let mut grouped: HashMap<String, Vec<&RunEntry>> = HashMap::new();
    for r in runs {
        grouped
            .entry(r.tool.clone().unwrap_or_else(|| "unknown".to_string()))
            .or_default()
            .push(r);
    }

    let mut by_tool: Vec<Value> = grouped
        .into_iter()
        .map(|(tool, entries)| {
            let c = entries.len() as f64;
            let d: f64 = entries
                .iter()
                .map(|r| r.duration_ms.unwrap_or(0) as f64)
                .sum();
            let e: f64 = entries
                .iter()
                .map(|r| r.effective_input_tokens.unwrap_or(0) as f64)
                .sum();
            let o: f64 = entries
                .iter()
                .map(|r| r.output_tokens.unwrap_or(0) as f64)
                .sum();
            json!({
                "tool": tool,
                "runs": entries.len(),
                "avg_duration_ms": if c == 0.0 { 0.0 } else { d / c },
                "avg_effective_input_tokens": if c == 0.0 { 0.0 } else { e / c },
                "avg_output_tokens": if c == 0.0 { 0.0 } else { o / c }
            })
        })
        .collect();

    by_tool.sort_by(|a, b| {
        b.get("runs")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .cmp(&a.get("runs").and_then(Value::as_u64).unwrap_or(0))
    });
    by_tool
}

pub fn print_metrics(n: usize) -> i32 {
    let (log_file, runs) = match load_runs_for("metrics", n) {
        Ok(v) => v,
        Err(code) => return code,
    };
    if runs.is_empty() {
        return print_json_value("cxrs metrics", &metrics_empty_json(&log_file));
    }

    let total = runs.len() as f64;
    let sum_dur: f64 = runs.iter().map(|r| r.duration_ms.unwrap_or(0) as f64).sum();
    let sum_in: f64 = runs
        .iter()
        .map(|r| r.input_tokens.unwrap_or(0) as f64)
        .sum();
    let sum_cached: f64 = runs
        .iter()
        .map(|r| r.cached_input_tokens.unwrap_or(0) as f64)
        .sum();
    let sum_eff: f64 = runs
        .iter()
        .map(|r| r.effective_input_tokens.unwrap_or(0) as f64)
        .sum();
    let sum_out: f64 = runs
        .iter()
        .map(|r| r.output_tokens.unwrap_or(0) as f64)
        .sum();

    let out = json!({
      "log_file": log_file.display().to_string(),
      "runs": runs.len(),
      "avg_duration_ms": sum_dur / total,
      "avg_input_tokens": sum_in / total,
      "avg_cached_input_tokens": sum_cached / total,
      "avg_effective_input_tokens": sum_eff / total,
      "avg_output_tokens": sum_out / total,
      "by_tool": group_metrics_by_tool(&runs)
    });
    print_json_value("cxrs metrics", &out)
}

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

fn print_alert_header(
    n: usize,
    runs_len: usize,
    max_ms: u64,
    max_eff: u64,
    slow_violations: usize,
    token_violations: usize,
    sum_in: u64,
    sum_cached: u64,
) {
    println!("== cxrs alert (last {n} runs) ==");
    println!("Runs: {runs_len}");
    println!("Thresholds: max_ms={max_ms}, max_eff_in={max_eff}");
    println!("Slow threshold violations: {slow_violations}");
    println!("Token threshold violations: {token_violations}");
    match (sum_in > 0).then_some((sum_cached as f64 / sum_in as f64) * 100.0) {
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

    print_alert_header(
        n,
        runs.len(),
        max_ms,
        max_eff,
        slow_violations,
        token_violations,
        sum_in,
        sum_cached,
    );

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

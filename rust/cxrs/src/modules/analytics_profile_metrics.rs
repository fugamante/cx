use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;

use crate::types::RunEntry;

use super::analytics_shared::{load_runs_for, print_json_value};

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

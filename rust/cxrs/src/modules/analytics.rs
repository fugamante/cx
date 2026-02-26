use serde_json::{Value, json};
use std::collections::HashMap;

use crate::logs::load_runs;
use crate::paths::resolve_log_file;
use crate::types::RunEntry;

pub fn parse_ts_epoch(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp())
}

fn show_field<T: ToString>(label: &str, value: Option<T>) {
    match value {
        Some(v) => println!("{label}: {}", v.to_string()),
        None => println!("{label}: n/a"),
    }
}

pub fn print_profile(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("== cxrs profile (last {n} runs) ==");
        println!("Runs: 0");
        println!("Avg duration: 0ms");
        println!("Avg effective tokens: 0");
        println!("Cache hit rate: n/a");
        println!("Output/input ratio: n/a");
        println!("Slowest run: n/a");
        println!("Heaviest context: n/a");
        println!("log_file: {}", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs profile: {e}");
            return 1;
        }
    };
    let total = runs.len();
    if total == 0 {
        println!("== cxrs profile (last {n} runs) ==");
        println!("Runs: 0");
        println!("Avg duration: 0ms");
        println!("Avg effective tokens: 0");
        println!("Cache hit rate: n/a");
        println!("Output/input ratio: n/a");
        println!("Slowest run: n/a");
        println!("Heaviest context: n/a");
        println!("log_file: {}", log_file.display());
        return 0;
    }

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

    let avg_dur = sum_dur / (total as u64);
    let avg_eff = sum_eff / (total as u64);
    let cache_hit_rate = if sum_in == 0 {
        None
    } else {
        Some((sum_cached as f64) / (sum_in as f64))
    };
    let out_in_ratio = if sum_eff == 0 {
        None
    } else {
        Some((sum_out as f64) / (sum_eff as f64))
    };

    let slowest = runs
        .iter()
        .filter_map(|r| {
            r.duration_ms
                .map(|d| (d, r.tool.clone().unwrap_or_else(|| "unknown".to_string())))
        })
        .max_by_key(|(d, _)| *d);
    let heaviest = runs
        .iter()
        .filter_map(|r| {
            r.effective_input_tokens
                .map(|e| (e, r.tool.clone().unwrap_or_else(|| "unknown".to_string())))
        })
        .max_by_key(|(e, _)| *e);

    println!("== cxrs profile (last {n} runs) ==");
    println!("Runs: {total}");
    println!("Avg duration: {avg_dur}ms");
    println!("Avg effective tokens: {avg_eff}");
    if let Some(v) = cache_hit_rate {
        println!("Cache hit rate: {}%", (v * 100.0).round() as i64);
    } else {
        println!("Cache hit rate: n/a");
    }
    if let Some(v) = out_in_ratio {
        println!("Output/input ratio: {:.2}", v);
    } else {
        println!("Output/input ratio: n/a");
    }
    if let Some((d, t)) = slowest {
        println!("Slowest run: {d}ms ({t})");
    } else {
        println!("Slowest run: n/a");
    }
    if let Some((e, t)) = heaviest {
        println!("Heaviest context: {e} effective tokens ({t})");
    } else {
        println!("Heaviest context: n/a");
    }
    println!("log_file: {}", log_file.display());
    0
}

pub fn print_metrics(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        let out = json!({
            "log_file": log_file.display().to_string(),
            "runs": 0,
            "avg_duration_ms": 0.0,
            "avg_input_tokens": 0.0,
            "avg_cached_input_tokens": 0.0,
            "avg_effective_input_tokens": 0.0,
            "avg_output_tokens": 0.0,
            "by_tool": []
        });
        match serde_json::to_string_pretty(&out) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("cxrs metrics: failed to render JSON: {e}");
                return 1;
            }
        }
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs metrics: {e}");
            return 1;
        }
    };

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

    let mut grouped: HashMap<String, Vec<&RunEntry>> = HashMap::new();
    for r in &runs {
        let key = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        grouped.entry(key).or_default().push(r);
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

    let out = json!({
      "log_file": log_file.display().to_string(),
      "runs": runs.len(),
      "avg_duration_ms": if total == 0.0 { 0.0 } else { sum_dur / total },
      "avg_input_tokens": if total == 0.0 { 0.0 } else { sum_in / total },
      "avg_cached_input_tokens": if total == 0.0 { 0.0 } else { sum_cached / total },
      "avg_effective_input_tokens": if total == 0.0 { 0.0 } else { sum_eff / total },
      "avg_output_tokens": if total == 0.0 { 0.0 } else { sum_out / total },
      "by_tool": by_tool
    });
    match serde_json::to_string_pretty(&out) {
        Ok(s) => println!("{s}"),
        Err(e) => {
            eprintln!("cxrs metrics: failed to render JSON: {e}");
            return 1;
        }
    }
    0
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(default)
}

pub fn print_alert(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("== cxrs alert (last {n} runs) ==");
        println!("Runs: 0");
        println!("Slow threshold violations: 0");
        println!("Token threshold violations: 0");
        println!("Avg cache hit rate: n/a");
        println!("Top 5 slowest: n/a");
        println!("Top 5 heaviest: n/a");
        println!("log_file: {}", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs alert: {e}");
            return 1;
        }
    };
    let total = runs.len();
    let max_ms = env_u64("CXALERT_MAX_MS", 12000);
    let max_eff = env_u64("CXALERT_MAX_EFF_IN", 8000);

    let mut slow_violations = 0usize;
    let mut token_violations = 0usize;
    let mut sum_in: u64 = 0;
    let mut sum_cached: u64 = 0;

    for run in &runs {
        let d = run.duration_ms.unwrap_or(0);
        let eff = run.effective_input_tokens.unwrap_or(0);
        if d > max_ms {
            slow_violations += 1;
        }
        if eff > max_eff {
            token_violations += 1;
        }
        sum_in += run.input_tokens.unwrap_or(0);
        sum_cached += run.cached_input_tokens.unwrap_or(0);
    }

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

    let cache_hit = if sum_in == 0 {
        None
    } else {
        Some((sum_cached as f64 / sum_in as f64) * 100.0)
    };

    println!("== cxrs alert (last {n} runs) ==");
    println!("Runs: {total}");
    println!("Thresholds: max_ms={max_ms}, max_eff_in={max_eff}");
    println!("Slow threshold violations: {slow_violations}");
    println!("Token threshold violations: {token_violations}");
    match cache_hit {
        Some(v) => println!("Avg cache hit rate: {}%", v.round() as i64),
        None => println!("Avg cache hit rate: n/a"),
    }

    if slowest.is_empty() {
        println!("Top 5 slowest: n/a");
    } else {
        println!("Top 5 slowest:");
        for (d, tool, ts) in slowest {
            println!("- {d}ms | {tool} | {ts}");
        }
    }

    if heaviest.is_empty() {
        println!("Top 5 heaviest: n/a");
    } else {
        println!("Top 5 heaviest:");
        for (e, tool, ts) in heaviest {
            println!("- {e} effective tokens | {tool} | {ts}");
        }
    }
    println!("log_file: {}", log_file.display());
    0
}

pub fn print_worklog(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("# cxrs Worklog");
        println!();
        println!("Window: last {n} runs");
        println!();
        println!("No runs found.");
        println!();
        println!("_log_file: {}_", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs worklog: {e}");
            return 1;
        }
    };

    println!("# cxrs Worklog");
    println!();
    println!("Window: last {n} runs");
    println!();

    let mut by_tool: HashMap<String, (u64, u64, u64)> = HashMap::new();
    for r in &runs {
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let entry = by_tool.entry(tool).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 += r.duration_ms.unwrap_or(0);
        entry.2 += r.effective_input_tokens.unwrap_or(0);
    }

    let mut grouped: Vec<(String, u64, u64, u64)> = by_tool
        .into_iter()
        .map(|(tool, (count, sum_dur, sum_eff))| {
            let avg_dur = if count == 0 { 0 } else { sum_dur / count };
            let avg_eff = if count == 0 { 0 } else { sum_eff / count };
            (tool, count, avg_dur, avg_eff)
        })
        .collect();
    grouped.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));

    println!("## By Tool");
    println!();
    println!("| Tool | Runs | Avg Duration (ms) | Avg Effective Tokens |");
    println!("|---|---:|---:|---:|");
    for (tool, count, avg_dur, avg_eff) in grouped {
        println!("| {tool} | {count} | {avg_dur} | {avg_eff} |");
    }
    println!();

    println!("## Chronological Runs");
    println!();
    for r in &runs {
        let ts = r.ts.clone().unwrap_or_else(|| "n/a".to_string());
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let dur = r.duration_ms.unwrap_or(0);
        let eff = r.effective_input_tokens.unwrap_or(0);
        println!("- {ts} | {tool} | {dur}ms | {eff} effective tokens");
    }
    println!();
    println!("_log_file: {}_", log_file.display());
    0
}

pub fn print_trace(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        eprintln!("cxrs trace: no log file at {}", log_file.display());
        return 1;
    }

    let runs = match load_runs(&log_file, usize::MAX) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs trace: {e}");
            return 1;
        }
    };
    if runs.is_empty() {
        eprintln!("cxrs trace: no runs in {}", log_file.display());
        return 1;
    }
    if n == 0 || n > runs.len() {
        eprintln!(
            "cxrs trace: run index out of range (requested {}, available {})",
            n,
            runs.len()
        );
        return 2;
    }
    let idx = runs.len() - n;
    let run = runs.get(idx).cloned().unwrap_or_default();

    println!("== cxrs trace (run #{n} most recent) ==");
    show_field("ts", run.ts);
    show_field("tool", run.tool);
    show_field("cwd", run.cwd);
    show_field("duration_ms", run.duration_ms);
    show_field("input_tokens", run.input_tokens);
    show_field("cached_input_tokens", run.cached_input_tokens);
    show_field("effective_input_tokens", run.effective_input_tokens);
    show_field("output_tokens", run.output_tokens);
    show_field("scope", run.scope);
    show_field("repo_root", run.repo_root);
    show_field("llm_backend", run.llm_backend);
    show_field("llm_model", run.llm_model);
    show_field("prompt_sha256", run.prompt_sha256);
    show_field("prompt_preview", run.prompt_preview);
    println!("log_file: {}", log_file.display());
    0
}

use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::logs::load_values;
use crate::paths::resolve_log_file;

fn parse_args(args: &[String]) -> Result<(usize, bool), String> {
    let mut n = 200usize;
    let mut as_json = false;
    for a in args {
        if a == "--json" {
            as_json = true;
            continue;
        }
        let parsed = a
            .parse::<usize>()
            .map_err(|_| format!("prompt-stats: invalid argument '{a}'"))?;
        if parsed == 0 {
            return Err("prompt-stats: N must be >= 1".to_string());
        }
        n = parsed;
    }
    Ok((n, as_json))
}

fn summarize_by_tool(rows: &[Value]) -> Vec<Value> {
    let mut agg: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    for row in rows {
        let Some(raw) = row.get("prompt_len_raw").and_then(Value::as_u64) else {
            continue;
        };
        let filtered = row
            .get("prompt_len_filtered")
            .and_then(Value::as_u64)
            .unwrap_or(raw);
        let tool = row
            .get("tool")
            .and_then(Value::as_str)
            .or_else(|| row.get("command").and_then(Value::as_str))
            .unwrap_or("unknown")
            .to_string();
        let entry = agg.entry(tool).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 += raw;
        entry.2 += filtered;
    }
    let mut out: Vec<Value> = agg
        .into_iter()
        .map(|(tool, (runs, raw, filtered))| {
            let saved = raw.saturating_sub(filtered);
            json!({
                "tool": tool,
                "runs": runs,
                "raw_chars": raw,
                "filtered_chars": filtered,
                "saved_chars": saved,
                "saved_pct": if raw == 0 { Value::Null } else { json!((saved as f64) / (raw as f64)) }
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.get("saved_chars")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .cmp(&a.get("saved_chars").and_then(Value::as_u64).unwrap_or(0))
    });
    out.truncate(10);
    out
}

pub fn cmd_prompt_stats(args: &[String]) -> i32 {
    let (n, as_json) = match parse_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            crate::cx_eprintln!("Usage: prompt-stats [N] [--json]");
            return 2;
        }
    };
    let Some(log_file) = resolve_log_file() else {
        crate::cx_eprintln!("cxrs prompt-stats: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        if as_json {
            println!(
                "{}",
                json!({
                    "window": n,
                    "runs": 0,
                    "rows_with_prompt_lengths": 0,
                    "prompt_filter_applied_runs": 0,
                    "raw_chars_total": 0,
                    "filtered_chars_total": 0,
                    "saved_chars_total": 0,
                    "saved_pct": Value::Null,
                    "by_tool": [],
                    "log_file": log_file.display().to_string()
                })
            );
        } else {
            println!("== cx prompt-stats (last {n} runs) ==");
            println!("runs: 0");
            println!("rows_with_prompt_lengths: 0");
            println!("prompt_filter_applied_runs: 0");
            println!("saved_chars_total: 0");
            println!("saved_pct: n/a");
            println!("log_file: {}", log_file.display());
        }
        return 0;
    }

    let rows = match load_values(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("cxrs prompt-stats: {e}");
            return 1;
        }
    };
    let mut rows_with_prompt_lengths = 0u64;
    let mut applied = 0u64;
    let mut raw_total = 0u64;
    let mut filtered_total = 0u64;
    for row in &rows {
        let Some(raw) = row.get("prompt_len_raw").and_then(Value::as_u64) else {
            continue;
        };
        rows_with_prompt_lengths += 1;
        let filtered = row
            .get("prompt_len_filtered")
            .and_then(Value::as_u64)
            .unwrap_or(raw);
        raw_total += raw;
        filtered_total += filtered;
        if row
            .get("prompt_filter_applied")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            applied += 1;
        }
    }
    let saved_total = raw_total.saturating_sub(filtered_total);
    let by_tool = summarize_by_tool(&rows);
    let payload = json!({
        "window": n,
        "runs": rows.len(),
        "rows_with_prompt_lengths": rows_with_prompt_lengths,
        "prompt_filter_applied_runs": applied,
        "raw_chars_total": raw_total,
        "filtered_chars_total": filtered_total,
        "saved_chars_total": saved_total,
        "saved_pct": if raw_total == 0 { Value::Null } else { json!((saved_total as f64) / (raw_total as f64)) },
        "by_tool": by_tool,
        "log_file": log_file.display().to_string()
    });

    if as_json {
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("cxrs prompt-stats: failed to render json: {e}");
                return 1;
            }
        }
        return 0;
    }

    println!("== cx prompt-stats (last {n} runs) ==");
    println!("runs: {}", rows.len());
    println!("rows_with_prompt_lengths: {rows_with_prompt_lengths}");
    println!("prompt_filter_applied_runs: {applied}");
    println!("raw_chars_total: {raw_total}");
    println!("filtered_chars_total: {filtered_total}");
    println!("saved_chars_total: {saved_total}");
    println!(
        "saved_pct: {}",
        if raw_total == 0 {
            "n/a".to_string()
        } else {
            format!(
                "{}%",
                ((saved_total as f64 / raw_total as f64) * 100.0).round() as i64
            )
        }
    );
    println!("by_tool:");
    if let Some(arr) = payload.get("by_tool").and_then(Value::as_array) {
        for row in arr {
            println!(
                "- {}: saved_chars={} saved_pct={} runs={}",
                row.get("tool").and_then(Value::as_str).unwrap_or("unknown"),
                row.get("saved_chars").and_then(Value::as_u64).unwrap_or(0),
                row.get("saved_pct")
                    .and_then(Value::as_f64)
                    .map(|v| format!("{}%", (v * 100.0).round() as i64))
                    .unwrap_or_else(|| "n/a".to_string()),
                row.get("runs").and_then(Value::as_u64).unwrap_or(0)
            );
        }
    }
    println!("log_file: {}", log_file.display());
    0
}

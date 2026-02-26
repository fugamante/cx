use serde_json::{Value, json};

use crate::optimize_report::optimize_report;

pub fn print_optimize(n: usize, json_out: bool) -> i32 {
    let report = match optimize_report(n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs optimize: {e}");
            return 1;
        }
    };
    if json_out {
        println!("{report}");
        return 0;
    }

    println!("== cxrs optimize (last {n} runs) ==");
    println!("Section A: Scoreboard");
    let sb = report
        .get("scoreboard")
        .cloned()
        .unwrap_or_else(|| json!({}));
    println!(
        "runs: {}",
        sb.get("runs").and_then(Value::as_u64).unwrap_or(0)
    );
    println!(
        "alerts: {}",
        sb.get("alerts").and_then(Value::as_u64).unwrap_or(0)
    );
    if let Some(v) = sb.get("cache_hit_rate").and_then(Value::as_f64) {
        println!("cache_hit_rate: {}%", (v * 100.0).round() as i64);
    } else {
        println!("cache_hit_rate: n/a");
    }
    if let Some(tr) = sb.get("cache_hit_trend") {
        let a = tr.get("first_half").and_then(Value::as_f64);
        let b = tr.get("second_half").and_then(Value::as_f64);
        match (a, b) {
            (Some(x), Some(y)) => println!(
                "cache_trend: first_half={}%, second_half={}%, delta={}pp",
                (x * 100.0).round() as i64,
                (y * 100.0).round() as i64,
                ((y - x) * 100.0).round() as i64
            ),
            _ => println!("cache_trend: n/a"),
        }
    }
    println!("top_by_avg_duration_ms:");
    if let Some(arr) = sb.get("top_avg_duration_ms").and_then(Value::as_array) {
        for row in arr {
            if let Some(pair) = row.as_array()
                && pair.len() == 2
            {
                println!(
                    "- {}: {}ms",
                    pair[0].as_str().unwrap_or("unknown"),
                    pair[1].as_u64().unwrap_or(0)
                );
            }
        }
    }
    println!("top_by_avg_effective_tokens:");
    if let Some(arr) = sb
        .get("top_avg_effective_input_tokens")
        .and_then(Value::as_array)
    {
        for row in arr {
            if let Some(pair) = row.as_array()
                && pair.len() == 2
            {
                println!(
                    "- {}: {}",
                    pair[0].as_str().unwrap_or("unknown"),
                    pair[1].as_u64().unwrap_or(0)
                );
            }
        }
    }
    if let Some(c) = sb.get("budget_clipping_frequency") {
        let rate = c.get("rate").and_then(Value::as_f64);
        match rate {
            Some(r) => println!("budget_clipping_frequency: {}%", (r * 100.0).round() as i64),
            None => println!("budget_clipping_frequency: n/a"),
        }
    }
    println!("capture_provider_compression:");
    if let Some(arr) = sb
        .get("capture_provider_compression")
        .and_then(Value::as_array)
    {
        if arr.is_empty() {
            println!("- n/a");
        } else {
            for row in arr {
                let provider = row
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let ratio = row
                    .get("processed_over_raw")
                    .and_then(Value::as_f64)
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "n/a".to_string());
                println!("- {provider}: processed/raw={ratio}");
            }
        }
    }

    println!();
    println!("Section B: Anomaly Alerts");
    if let Some(arr) = report.get("anomalies").and_then(Value::as_array) {
        if arr.is_empty() {
            println!("- none");
        } else {
            for a in arr {
                println!("- {}", a.as_str().unwrap_or(""));
            }
        }
    }

    println!();
    println!("Section C: Actionable Recommendations");
    if let Some(arr) = report.get("recommendations").and_then(Value::as_array) {
        for r in arr {
            println!("- {}", r.as_str().unwrap_or(""));
        }
    }
    println!(
        "log_file: {}",
        report
            .get("log_file")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>")
    );
    0
}

use serde_json::{Value, json};

use crate::optimize_report::optimize_report;

fn print_tool_pairs(label: &str, arr: Option<&Vec<Value>>, suffix: &str) {
    println!("{label}");
    if let Some(rows) = arr {
        for row in rows {
            if let Some(pair) = row.as_array()
                && pair.len() == 2
            {
                println!(
                    "- {}: {}{}",
                    pair[0].as_str().unwrap_or("unknown"),
                    pair[1].as_u64().unwrap_or(0),
                    suffix
                );
            }
        }
    }
}

fn print_capture_compression(sb: &Value) {
    println!("capture_provider_compression:");
    let Some(arr) = sb
        .get("capture_provider_compression")
        .and_then(Value::as_array)
    else {
        return;
    };
    if arr.is_empty() {
        println!("- n/a");
        return;
    }
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

fn print_retry_health(sb: &Value) {
    let Some(rh) = sb.get("retry_health") else {
        println!("retry_health: n/a");
        return;
    };
    let rows_after_retry = rh
        .get("rows_after_retry")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let rows_after_retry_success = rh
        .get("rows_after_retry_success")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let rows_after_retry_rate = rh
        .get("rows_after_retry_rate")
        .and_then(Value::as_f64)
        .map(|v| format!("{}%", (v * 100.0).round() as i64))
        .unwrap_or_else(|| "n/a".to_string());
    let rows_after_retry_success_rate = rh
        .get("rows_after_retry_success_rate")
        .and_then(Value::as_f64)
        .map(|v| format!("{}%", (v * 100.0).round() as i64))
        .unwrap_or_else(|| "n/a".to_string());
    let tasks_with_timeout = rh
        .get("tasks_with_timeout")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let tasks_recovered = rh
        .get("tasks_recovered")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let tasks_recovery_rate = rh
        .get("tasks_recovery_rate")
        .and_then(Value::as_f64)
        .map(|v| format!("{}%", (v * 100.0).round() as i64))
        .unwrap_or_else(|| "n/a".to_string());

    println!(
        "retry_health: rows_after_retry={} ({rows_after_retry_rate}), success={} ({rows_after_retry_success_rate}), tasks_recovered={}/{} ({tasks_recovery_rate})",
        rows_after_retry, rows_after_retry_success, tasks_recovered, tasks_with_timeout
    );
    let hist = rh
        .get("attempt_histogram")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|row| {
                    row.as_array().and_then(|pair| {
                        if pair.len() == 2 {
                            Some(format!(
                                "{}:{}",
                                pair[0].as_u64().unwrap_or(0),
                                pair[1].as_u64().unwrap_or(0)
                            ))
                        } else {
                            None
                        }
                    })
                })
                .collect::<Vec<String>>()
                .join(",")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "<none>".to_string());
    println!("retry_attempt_histogram: {hist}");
}

fn print_timeout_frequency(sb: &Value) {
    let Some(tf) = sb.get("timeout_frequency") else {
        println!("timeout_frequency: n/a");
        return;
    };
    let timeout_runs = tf.get("timeout_runs").and_then(Value::as_u64).unwrap_or(0);
    let rate = tf
        .get("rate")
        .and_then(Value::as_f64)
        .map(|v| format!("{}%", (v * 100.0).round() as i64))
        .unwrap_or_else(|| "n/a".to_string());
    println!("timeout_frequency: {rate} ({timeout_runs} runs)");
    let labels = tf.get("top_labels").and_then(Value::as_array);
    if let Some(rows) = labels
        && !rows.is_empty()
    {
        println!("timeout_top_labels:");
        for row in rows {
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
}

fn print_scoreboard(sb: &Value) {
    println!("Section A: Scoreboard");
    println!(
        "runs: {}",
        sb.get("runs").and_then(Value::as_u64).unwrap_or(0)
    );
    println!(
        "alerts: {}",
        sb.get("alerts").and_then(Value::as_u64).unwrap_or(0)
    );
    match sb.get("cache_hit_rate").and_then(Value::as_f64) {
        Some(v) => println!("cache_hit_rate: {}%", (v * 100.0).round() as i64),
        None => println!("cache_hit_rate: n/a"),
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

    print_tool_pairs(
        "top_by_avg_duration_ms:",
        sb.get("top_avg_duration_ms").and_then(Value::as_array),
        "ms",
    );
    print_tool_pairs(
        "top_by_avg_effective_tokens:",
        sb.get("top_avg_effective_input_tokens")
            .and_then(Value::as_array),
        "",
    );

    if let Some(c) = sb.get("budget_clipping_frequency") {
        match c.get("rate").and_then(Value::as_f64) {
            Some(r) => println!("budget_clipping_frequency: {}%", (r * 100.0).round() as i64),
            None => println!("budget_clipping_frequency: n/a"),
        }
    }
    print_timeout_frequency(sb);
    print_retry_health(sb);
    print_capture_compression(sb);
}

fn print_list_section(title: &str, arr: Option<&Vec<Value>>, empty: &str) {
    println!();
    println!("{title}");
    match arr {
        Some(a) if !a.is_empty() => {
            for v in a {
                println!("- {}", v.as_str().unwrap_or(""));
            }
        }
        _ => println!("- {empty}"),
    }
}

pub fn print_optimize(n: usize, json_out: bool) -> i32 {
    let report = match optimize_report(n) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("cxrs optimize: {e}");
            return 1;
        }
    };
    if json_out {
        println!("{report}");
        return 0;
    }

    println!("== cxrs optimize (last {n} runs) ==");
    let sb = report
        .get("scoreboard")
        .cloned()
        .unwrap_or_else(|| json!({}));
    print_scoreboard(&sb);
    print_list_section(
        "Section B: Anomaly Alerts",
        report.get("anomalies").and_then(Value::as_array),
        "none",
    );
    print_list_section(
        "Section C: Actionable Recommendations",
        report.get("recommendations").and_then(Value::as_array),
        "none",
    );
    println!(
        "log_file: {}",
        report
            .get("log_file")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>")
    );
    0
}

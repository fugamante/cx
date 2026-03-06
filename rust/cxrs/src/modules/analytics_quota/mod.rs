use serde_json::{Value, json};

mod catalog;
mod guard;
mod resolution;
mod shared;

use catalog::cmd_quota_catalog;
use guard::{cmd_quota_guard, cmd_quota_set, cmd_quota_unset};
use resolution::quota_probe_payload;
use shared::{daily_burn, read_window_rows, top_commands};

fn parse_args(args: &[String]) -> Result<(usize, bool, bool), String> {
    let mut days = 30usize;
    let mut as_json = false;
    let mut probe = false;
    for a in args {
        if a == "probe" || a == "--probe" {
            probe = true;
            continue;
        }
        if a == "--json" {
            as_json = true;
            continue;
        }
        let parsed = a
            .parse::<usize>()
            .map_err(|_| format!("quota: invalid argument '{a}'"))?;
        if parsed == 0 {
            return Err("quota: days must be >= 1".to_string());
        }
        days = parsed;
    }
    Ok((days, as_json, probe))
}

pub fn quota_probe_for_backend_days(
    days: usize,
    backend: &str,
    model: Option<&str>,
) -> Result<Value, String> {
    let (log_file, rows) = read_window_rows(days)?;
    Ok(quota_probe_payload(
        days,
        &log_file,
        &rows,
        Some(backend),
        model,
    ))
}

pub fn cmd_quota(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("catalog") {
        return cmd_quota_catalog(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("set") {
        return cmd_quota_set(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("unset") {
        return cmd_quota_unset(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("guard") {
        return cmd_quota_guard(&args[1..]);
    }

    let (days, as_json, probe) = match parse_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            crate::cx_eprintln!(
                "Usage: quota [probe] [days] [--json] | quota catalog <show|refresh [--if-stale --max-age-hours N] [--json]|auto <show|on|off>>"
            );
            return 2;
        }
    };
    let (log_file, rows) = match read_window_rows(days) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("cxrs {e}");
            return 1;
        }
    };

    let runs = rows.len() as u64;
    let total_input: u64 = rows
        .iter()
        .map(|r| r.get("input_tokens").and_then(Value::as_u64).unwrap_or(0))
        .sum();
    let total_cached: u64 = rows
        .iter()
        .map(|r| {
            r.get("cached_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        })
        .sum();
    let total_effective: u64 = rows
        .iter()
        .map(|r| {
            r.get("effective_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| r.get("input_tokens").and_then(Value::as_u64).unwrap_or(0))
        })
        .sum();
    let total_output: u64 = rows
        .iter()
        .map(|r| r.get("output_tokens").and_then(Value::as_u64).unwrap_or(0))
        .sum();

    let per_day = if days == 0 {
        0
    } else {
        total_effective / days as u64
    };
    let monthly_projection = per_day * 30;
    let payload = json!({
        "window_days": days,
        "log_file": log_file.display().to_string(),
        "runs": runs,
        "tokens": {
            "input": total_input,
            "cached_input": total_cached,
            "effective_input": total_effective,
            "output": total_output,
            "cache_hit_rate": if total_input == 0 { Value::Null } else { json!((total_cached as f64) / (total_input as f64)) }
        },
        "daily_effective_tokens_avg": per_day,
        "monthly_effective_projection": monthly_projection,
        "daily_burn": daily_burn(&rows),
        "top_commands": top_commands(&rows),
        "recommendations": vec![
            "Set a provider quota total with CX_QUOTA_<BACKEND>_TOTAL_TOKENS or CX_QUOTA_TOTAL_TOKENS.",
            "Use quota catalog for official-source references: cx quota catalog refresh && cx quota catalog show.",
            "Use --actions + --strict gates to avoid broad retries.",
            "Prefer lean mode and tighter context budgets on token-heavy commands."
        ]
    });

    if as_json {
        let out = if probe {
            quota_probe_payload(days, &log_file, &rows, None, None)
        } else {
            payload
        };
        match serde_json::to_string_pretty(&out) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("cxrs quota: failed to render json: {e}");
                return 1;
            }
        }
        return 0;
    }

    if probe {
        let probe_payload = quota_probe_payload(days, &log_file, &rows, None, None);
        println!("== cx quota probe (last {days} days) ==");
        println!(
            "backend: {}",
            probe_payload
                .get("backend")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        println!(
            "model: {}",
            probe_payload
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("<unset>")
        );
        println!(
            "service_kind: {}",
            probe_payload
                .get("service_kind")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        println!(
            "quota_tier: {}",
            probe_payload
                .get("quota_tier")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        println!(
            "quota_limit_type: {}",
            probe_payload
                .get("quota_limit_type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        println!(
            "quota_source: {}",
            probe_payload
                .get("quota_source")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        println!(
            "quota_source_url: {}",
            probe_payload
                .get("quota_source_url")
                .and_then(Value::as_str)
                .unwrap_or("n/a")
        );
        println!(
            "quota_total_tokens: {}",
            probe_payload
                .get("quota_total_tokens")
                .and_then(Value::as_u64)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        println!(
            "quota_used_tokens_window: {}",
            probe_payload
                .get("quota_used_tokens_window")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "quota_remaining_tokens: {}",
            probe_payload
                .get("quota_remaining_tokens")
                .and_then(Value::as_u64)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        println!(
            "quota_remaining_pct: {}",
            probe_payload
                .get("quota_remaining_pct")
                .and_then(Value::as_f64)
                .map(|v| format!("{}%", (v * 100.0).round() as i64))
                .unwrap_or_else(|| "unknown".to_string())
        );
        println!("log_file: {}", log_file.display());
        return 0;
    }

    println!("== cx quota (last {days} days) ==");
    println!("runs: {runs}");
    println!("effective_input_tokens: {total_effective}");
    println!("output_tokens: {total_output}");
    println!(
        "cache_hit_rate: {}",
        if total_input == 0 {
            "n/a".to_string()
        } else {
            format!(
                "{}%",
                ((total_cached as f64 / total_input as f64) * 100.0).round() as i64
            )
        }
    );
    println!("avg_daily_effective_tokens: {per_day}");
    println!("projected_monthly_effective_tokens: {monthly_projection}");
    println!("top_commands_by_avg_effective_tokens:");
    if let Some(arr) = payload.get("top_commands").and_then(Value::as_array) {
        for row in arr {
            println!(
                "- {}: avg_effective_tokens={} avg_duration_ms={} runs={}",
                row.get("command")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                row.get("avg_effective_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                row.get("avg_duration_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                row.get("runs").and_then(Value::as_u64).unwrap_or(0)
            );
        }
    }
    println!("recommendations:");
    if let Some(arr) = payload.get("recommendations").and_then(Value::as_array) {
        for rec in arr {
            println!("- {}", rec.as_str().unwrap_or(""));
        }
    }
    println!("log_file: {}", log_file.display());
    0
}

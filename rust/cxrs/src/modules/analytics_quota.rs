use chrono::Utc;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::logs::load_values;
use crate::paths::resolve_log_file;
use crate::runtime::{llm_backend, llm_model};
use crate::state::{read_state_value, value_at_path};

fn parse_ts_epoch(v: &Value) -> Option<i64> {
    let ts = v
        .get("timestamp")
        .and_then(Value::as_str)
        .or_else(|| v.get("ts").and_then(Value::as_str))?;
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp())
}

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

fn configured_quota_total(backend: &str) -> (Option<u64>, String) {
    let env_backend = format!("CX_QUOTA_{}_TOTAL_TOKENS", backend.to_uppercase());
    if let Ok(v) = std::env::var(&env_backend)
        && let Ok(parsed) = v.trim().parse::<u64>()
    {
        return (Some(parsed), format!("env:{env_backend}"));
    }
    if let Ok(v) = std::env::var("CX_QUOTA_TOTAL_TOKENS")
        && let Ok(parsed) = v.trim().parse::<u64>()
    {
        return (Some(parsed), "env:CX_QUOTA_TOTAL_TOKENS".to_string());
    }
    if let Some(state) = read_state_value() {
        let key = format!("preferences.quota.{}_total_tokens", backend);
        if let Some(v) = value_at_path(&state, &key)
            && let Some(parsed) = v.as_u64()
        {
            return (Some(parsed), format!("state:{key}"));
        }
    }
    (None, "unknown".to_string())
}

fn quota_probe_payload(days: usize, log_file: &std::path::Path, rows: &[Value]) -> Value {
    let backend = llm_backend();
    let model = llm_model();
    let used_effective: u64 = rows
        .iter()
        .map(|r| {
            r.get("effective_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| r.get("input_tokens").and_then(Value::as_u64).unwrap_or(0))
        })
        .sum();

    let (total_opt, mut source) = configured_quota_total(&backend);
    let (service_kind, total, remaining, remaining_pct) = if backend == "ollama" {
        source = "service:local_unmetered".to_string();
        ("local_unmetered", Value::Null, Value::Null, Value::Null)
    } else if let Some(total_tokens) = total_opt {
        let rem = total_tokens.saturating_sub(used_effective);
        let pct = if total_tokens == 0 {
            Value::Null
        } else {
            json!(rem as f64 / total_tokens as f64)
        };
        ("remote_metered", json!(total_tokens), json!(rem), pct)
    } else {
        ("remote_metered", Value::Null, Value::Null, Value::Null)
    };

    json!({
        "window_days": days,
        "log_file": log_file.display().to_string(),
        "backend": backend,
        "model": if model.is_empty() { Value::Null } else { json!(model) },
        "service_kind": service_kind,
        "quota_source": source,
        "quota_total_tokens": total,
        "quota_used_tokens_window": used_effective,
        "quota_remaining_tokens": remaining,
        "quota_remaining_pct": remaining_pct
    })
}

fn read_window_rows(days: usize) -> Result<(std::path::PathBuf, Vec<Value>), String> {
    let Some(log_file) = resolve_log_file() else {
        return Err("quota: unable to resolve log file".to_string());
    };
    if !log_file.exists() {
        return Ok((log_file, Vec::new()));
    }
    let rows = load_values(&log_file, 10_000)?;
    let now = Utc::now().timestamp();
    let cutoff = now - (days as i64 * 86_400);
    let filtered = rows
        .into_iter()
        .filter(|row| parse_ts_epoch(row).is_some_and(|t| t >= cutoff))
        .collect::<Vec<Value>>();
    Ok((log_file, filtered))
}

fn top_commands(rows: &[Value]) -> Vec<Value> {
    let mut map: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    for row in rows {
        let cmd = row
            .get("command")
            .and_then(Value::as_str)
            .or_else(|| row.get("tool").and_then(Value::as_str))
            .unwrap_or("unknown")
            .to_string();
        let in_tok = row
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let eff_tok = row
            .get("effective_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let dur = row
            .get("duration_ms")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let entry = map.entry(cmd).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 += eff_tok.max(in_tok);
        entry.2 += dur;
    }
    let mut out: Vec<Value> = map
        .into_iter()
        .map(|(cmd, (runs, tokens, duration_ms))| {
            json!({
                "command": cmd,
                "runs": runs,
                "avg_effective_tokens": if runs == 0 { 0 } else { tokens / runs },
                "avg_duration_ms": if runs == 0 { 0 } else { duration_ms / runs }
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.get("avg_effective_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .cmp(
                &a.get("avg_effective_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            )
    });
    out.truncate(5);
    out
}

fn daily_burn(rows: &[Value]) -> Vec<Value> {
    let mut day_map: BTreeMap<String, u64> = BTreeMap::new();
    for row in rows {
        let Some(ts) = parse_ts_epoch(row) else {
            continue;
        };
        let Some(day) = chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.date_naive()) else {
            continue;
        };
        let eff = row
            .get("effective_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| row.get("input_tokens").and_then(Value::as_u64).unwrap_or(0));
        *day_map.entry(day.to_string()).or_insert(0) += eff;
    }
    day_map
        .into_iter()
        .map(|(day, effective_tokens)| json!({ "day": day, "effective_tokens": effective_tokens }))
        .collect()
}

pub fn cmd_quota(args: &[String]) -> i32 {
    let (days, as_json, probe) = match parse_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            crate::cx_eprintln!("Usage: quota [probe] [days] [--json]");
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
    let top = top_commands(&rows);
    let recommendations = vec![
        "Set a provider quota total with CX_QUOTA_<BACKEND>_TOTAL_TOKENS or CX_QUOTA_TOTAL_TOKENS."
            .to_string(),
        "Use --actions + --strict gates to avoid broad retries.".to_string(),
        "Prefer lean mode and tighter context budgets on token-heavy commands.".to_string(),
    ];
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
        "top_commands": top,
        "recommendations": recommendations
    });

    if as_json {
        let out = if probe {
            quota_probe_payload(days, &log_file, &rows)
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
        let probe_payload = quota_probe_payload(days, &log_file, &rows);
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
            "quota_source: {}",
            probe_payload
                .get("quota_source")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
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

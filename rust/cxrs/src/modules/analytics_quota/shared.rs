use chrono::Utc;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::logs::load_values;
use crate::paths::resolve_log_file;

fn parse_ts_epoch(v: &Value) -> Option<i64> {
    let ts = v
        .get("timestamp")
        .and_then(Value::as_str)
        .or_else(|| v.get("ts").and_then(Value::as_str))?;
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp())
}

pub(super) fn read_window_rows(days: usize) -> Result<(std::path::PathBuf, Vec<Value>), String> {
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

pub(super) fn top_commands(rows: &[Value]) -> Vec<Value> {
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

pub(super) fn daily_burn(rows: &[Value]) -> Vec<Value> {
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

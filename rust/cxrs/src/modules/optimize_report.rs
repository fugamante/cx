use serde_json::{Value, json};
use std::collections::HashMap;

use crate::logs::load_runs;
use crate::paths::resolve_log_file;

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

pub fn parse_optimize_args(args: &[String], default_n: usize) -> Result<(usize, bool), String> {
    let mut n = default_n;
    let mut json_out = false;
    for a in args {
        if a == "--json" {
            json_out = true;
            continue;
        }
        if let Ok(v) = a.parse::<usize>()
            && v > 0
        {
            n = v;
            continue;
        }
        return Err(format!("invalid argument: {a}"));
    }
    Ok((n, json_out))
}

pub fn optimize_report(n: usize) -> Result<Value, String> {
    let Some(log_file) = resolve_log_file() else {
        return Err("unable to resolve log file".to_string());
    };
    if !log_file.exists() {
        return Ok(json!({
            "window": n,
            "runs": 0,
            "scoreboard": {"runs": 0},
            "anomalies": [],
            "recommendations": ["No runs available in log window."],
            "log_file": log_file.display().to_string()
        }));
    }
    let runs = load_runs(&log_file, n)?;
    if runs.is_empty() {
        return Ok(json!({
            "window": n,
            "runs": 0,
            "scoreboard": {"runs": 0},
            "anomalies": [],
            "recommendations": ["No runs available in log window."],
            "log_file": log_file.display().to_string()
        }));
    }

    let max_ms = env_u64("CXALERT_MAX_MS", 12000);
    let max_eff = env_u64("CXALERT_MAX_EFF_IN", 8000);
    let total = runs.len() as u64;

    let mut tool_eff: HashMap<String, (u64, u64)> = HashMap::new();
    let mut tool_dur: HashMap<String, (u64, u64)> = HashMap::new();
    let mut alerts = 0u64;
    let mut schema_fails = 0u64;
    let mut schema_total = 0u64;
    let mut clipped_count = 0u64;
    let mut clipped_total = 0u64;
    let mut provider_stats: HashMap<String, (u64, u64, u64, u64)> = HashMap::new();

    let mut sum_in: u64 = 0;
    let mut sum_cached: u64 = 0;

    for r in &runs {
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let eff = r.effective_input_tokens.unwrap_or(0);
        let dur = r.duration_ms.unwrap_or(0);

        let eff_entry = tool_eff.entry(tool.clone()).or_insert((0, 0));
        eff_entry.0 += eff;
        eff_entry.1 += 1;

        let dur_entry = tool_dur.entry(tool).or_insert((0, 0));
        dur_entry.0 += dur;
        dur_entry.1 += 1;

        if dur > max_ms || eff > max_eff {
            alerts += 1;
        }
        if r.schema_enforced.unwrap_or(false) {
            schema_total += 1;
            if r.schema_valid == Some(false) {
                schema_fails += 1;
            }
        }
        if r.clipped.is_some() {
            clipped_total += 1;
            if r.clipped == Some(true) {
                clipped_count += 1;
            }
        }
        if let Some(provider) = r.capture_provider.as_ref() {
            let entry = provider_stats
                .entry(provider.clone())
                .or_insert((0, 0, 0, 0));
            entry.0 += r.system_output_len_raw.unwrap_or(0);
            entry.1 += r.system_output_len_processed.unwrap_or(0);
            entry.2 += r.system_output_len_clipped.unwrap_or(0);
            entry.3 += 1;
        }

        sum_in += r.input_tokens.unwrap_or(0);
        sum_cached += r.cached_input_tokens.unwrap_or(0);
    }

    let mut top_eff: Vec<(String, u64)> = tool_eff
        .into_iter()
        .map(|(tool, (sum, count))| (tool, if count == 0 { 0 } else { sum / count }))
        .collect();
    top_eff.sort_by(|a, b| b.1.cmp(&a.1));
    top_eff.truncate(5);

    let mut top_dur: Vec<(String, u64)> = tool_dur
        .into_iter()
        .map(|(tool, (sum, count))| (tool, if count == 0 { 0 } else { sum / count }))
        .collect();
    top_dur.sort_by(|a, b| b.1.cmp(&a.1));
    top_dur.truncate(5);

    let cache_all = if sum_in == 0 {
        None
    } else {
        Some(sum_cached as f64 / sum_in as f64)
    };

    let mid = runs.len() / 2;
    let (first, second) = runs.split_at(mid.max(1).min(runs.len()));
    let first_in: u64 = first.iter().map(|r| r.input_tokens.unwrap_or(0)).sum();
    let first_cached: u64 = first
        .iter()
        .map(|r| r.cached_input_tokens.unwrap_or(0))
        .sum();
    let second_in: u64 = second.iter().map(|r| r.input_tokens.unwrap_or(0)).sum();
    let second_cached: u64 = second
        .iter()
        .map(|r| r.cached_input_tokens.unwrap_or(0))
        .sum();
    let first_cache = if first_in == 0 {
        None
    } else {
        Some(first_cached as f64 / first_in as f64)
    };
    let second_cache = if second_in == 0 {
        None
    } else {
        Some(second_cached as f64 / second_in as f64)
    };
    let clip_freq = if clipped_total == 0 {
        None
    } else {
        Some(clipped_count as f64 / clipped_total as f64)
    };
    let schema_fail_freq = if schema_total == 0 {
        None
    } else {
        Some(schema_fails as f64 / schema_total as f64)
    };
    let mut compression_rows: Vec<Value> = provider_stats
        .into_iter()
        .map(|(provider, (raw, processed, clipped, count))| {
            let processed_ratio = if raw == 0 {
                Value::Null
            } else {
                json!((processed as f64) / (raw as f64))
            };
            let clipped_ratio = if raw == 0 {
                Value::Null
            } else {
                json!((clipped as f64) / (raw as f64))
            };
            json!({
                "provider": provider,
                "runs": count,
                "raw_sum": raw,
                "processed_sum": processed,
                "clipped_sum": clipped,
                "processed_over_raw": processed_ratio,
                "clipped_over_raw": clipped_ratio
            })
        })
        .collect();
    compression_rows.sort_by(|a, b| {
        let ar = a
            .get("processed_over_raw")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        let br = b
            .get("processed_over_raw")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        ar.partial_cmp(&br).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut anomalies: Vec<String> = Vec::new();
    if let Some((tool, avg)) = top_dur.first()
        && *avg > max_ms / 2
    {
        anomalies.push(format!(
            "High latency concentration: {tool} avg_duration_ms={avg}"
        ));
    }
    if let Some((tool, avg)) = top_eff.first()
        && *avg > max_eff / 2
    {
        anomalies.push(format!(
            "High token load concentration: {tool} avg_effective_input_tokens={avg}"
        ));
    }
    if let (Some(a), Some(b)) = (first_cache, second_cache)
        && b + 0.05 < a
    {
        anomalies.push(format!(
            "Cache hit degraded: first_half={}%, second_half={}%",
            (a * 100.0).round() as i64,
            (b * 100.0).round() as i64
        ));
    }
    if let Some(freq) = schema_fail_freq
        && freq > 0.05
    {
        anomalies.push(format!(
            "Schema failure frequency elevated: {}%",
            (freq * 100.0).round() as i64
        ));
    }
    if let Some(freq) = clip_freq
        && freq > 0.30
    {
        anomalies.push(format!(
            "Budget clipping frequent: {}% of captured runs",
            (freq * 100.0).round() as i64
        ));
    }

    let mut recommendations: Vec<String> = Vec::new();
    if let Some((tool, avg_eff)) = top_eff.first() {
        recommendations.push(format!(
            "{tool} exceeds average token threshold ({avg_eff}); recommend lean mode."
        ));
    }
    if let (Some(a), Some(b)) = (first_cache, second_cache)
        && b + 0.05 < a
    {
        recommendations.push("Cache hit rate degraded; inspect prompt drift.".to_string());
    }
    if schema_fails > 0 {
        let tool = top_eff
            .first()
            .map(|v| v.0.clone())
            .unwrap_or_else(|| "schema command".to_string());
        recommendations.push(format!(
            "Schema failures detected for {tool}; enforce deterministic mode."
        ));
    }
    if recommendations.is_empty() {
        recommendations.push("No significant anomalies in this window.".to_string());
    }

    Ok(json!({
        "window": n,
        "runs": total,
        "scoreboard": {
            "runs": total,
            "alerts": alerts,
            "alerts_pct": if total == 0 { 0.0 } else { (alerts as f64 / total as f64) * 100.0 },
            "top_avg_duration_ms": top_dur,
            "top_avg_effective_input_tokens": top_eff,
            "cache_hit_rate": cache_all,
            "cache_hit_trend": {
                "first_half": first_cache,
                "second_half": second_cache,
                "delta": match (first_cache, second_cache) {
                    (Some(a), Some(b)) => Some(b - a),
                    _ => None
                }
            },
            "schema_failure_frequency": {
                "schema_runs": schema_total,
                "schema_failures": schema_fails,
                "rate": schema_fail_freq
            },
            "capture_provider_compression": compression_rows,
            "budget_clipping_frequency": {
                "captured_runs": clipped_total,
                "clipped_runs": clipped_count,
                "rate": clip_freq
            }
        },
        "anomalies": anomalies,
        "recommendations": recommendations,
        "log_file": log_file.display().to_string()
    }))
}

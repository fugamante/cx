use serde_json::{Value, json};
use std::collections::HashMap;

use crate::logs::load_runs;
use crate::optimize_rules::{
    build_recommendations, push_cache_anomaly, push_clip_anomaly, push_latency_anomaly,
    push_schema_anomaly, push_timeout_anomaly, push_token_anomaly,
};
use crate::paths::resolve_log_file;
use crate::types::RunEntry;

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

fn empty_report(n: usize, log_file: &std::path::Path) -> Value {
    json!({
        "window": n,
        "runs": 0,
        "scoreboard": {"runs": 0},
        "anomalies": [],
        "recommendations": ["No runs available in log window."],
        "log_file": log_file.display().to_string()
    })
}

#[derive(Default)]
struct Agg {
    tool_eff: HashMap<String, (u64, u64)>,
    tool_dur: HashMap<String, (u64, u64)>,
    timeout_labels: HashMap<String, u64>,
    provider_stats: HashMap<String, (u64, u64, u64, u64)>,
    alerts: u64,
    schema_fails: u64,
    schema_total: u64,
    clipped_count: u64,
    clipped_total: u64,
    timeout_count: u64,
    sum_in: u64,
    sum_cached: u64,
}

impl Agg {
    fn ingest(&mut self, r: &RunEntry, max_ms: u64, max_eff: u64) {
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let eff = r.effective_input_tokens.unwrap_or(0);
        let dur = r.duration_ms.unwrap_or(0);
        let eff_entry = self.tool_eff.entry(tool.clone()).or_insert((0, 0));
        eff_entry.0 += eff;
        eff_entry.1 += 1;
        let dur_entry = self.tool_dur.entry(tool).or_insert((0, 0));
        dur_entry.0 += dur;
        dur_entry.1 += 1;
        if dur > max_ms || eff > max_eff {
            self.alerts += 1;
        }
        if r.schema_enforced.unwrap_or(false) {
            self.schema_total += 1;
            if r.schema_valid == Some(false) {
                self.schema_fails += 1;
            }
        }
        if r.timed_out.unwrap_or(false) {
            self.timeout_count += 1;
            let label = r
                .command_label
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            *self.timeout_labels.entry(label).or_insert(0) += 1;
        }
        if r.clipped.is_some() {
            self.clipped_total += 1;
            if r.clipped == Some(true) {
                self.clipped_count += 1;
            }
        }
        if let Some(provider) = r.capture_provider.as_ref() {
            let entry = self
                .provider_stats
                .entry(provider.clone())
                .or_insert((0, 0, 0, 0));
            entry.0 += r.system_output_len_raw.unwrap_or(0);
            entry.1 += r.system_output_len_processed.unwrap_or(0);
            entry.2 += r.system_output_len_clipped.unwrap_or(0);
            entry.3 += 1;
        }
        self.sum_in += r.input_tokens.unwrap_or(0);
        self.sum_cached += r.cached_input_tokens.unwrap_or(0);
    }
}

fn top_avg(map: HashMap<String, (u64, u64)>) -> Vec<(String, u64)> {
    let mut rows: Vec<(String, u64)> = map
        .into_iter()
        .map(|(tool, (sum, count))| (tool, if count == 0 { 0 } else { sum / count }))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    rows.truncate(5);
    rows
}

fn cache_halves(runs: &[RunEntry]) -> (Option<f64>, Option<f64>) {
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
    (
        (first_in > 0).then_some(first_cached as f64 / first_in as f64),
        (second_in > 0).then_some(second_cached as f64 / second_in as f64),
    )
}

fn compression_rows(provider_stats: HashMap<String, (u64, u64, u64, u64)>) -> Vec<Value> {
    let mut rows: Vec<Value> = provider_stats
        .into_iter()
        .map(|(provider, (raw, processed, clipped, count))| {
            json!({
                "provider": provider,
                "runs": count,
                "raw_sum": raw,
                "processed_sum": processed,
                "clipped_sum": clipped,
                "processed_over_raw": if raw == 0 { Value::Null } else { json!((processed as f64) / (raw as f64)) },
                "clipped_over_raw": if raw == 0 { Value::Null } else { json!((clipped as f64) / (raw as f64)) }
            })
        })
        .collect();
    rows.sort_by(|a, b| {
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
    rows
}

fn build_anomalies(
    top_dur: &[(String, u64)],
    top_eff: &[(String, u64)],
    max_ms: u64,
    max_eff: u64,
    first_cache: Option<f64>,
    second_cache: Option<f64>,
    schema_fail_freq: Option<f64>,
    clip_freq: Option<f64>,
    timeout_freq: Option<f64>,
) -> Vec<String> {
    let mut anomalies: Vec<String> = Vec::new();
    push_latency_anomaly(&mut anomalies, top_dur, max_ms);
    push_token_anomaly(&mut anomalies, top_eff, max_eff);
    push_cache_anomaly(&mut anomalies, first_cache, second_cache);
    push_schema_anomaly(&mut anomalies, schema_fail_freq);
    push_clip_anomaly(&mut anomalies, clip_freq);
    push_timeout_anomaly(&mut anomalies, timeout_freq);
    anomalies
}

struct Derived {
    top_eff: Vec<(String, u64)>,
    top_dur: Vec<(String, u64)>,
    top_timeout_labels: Vec<(String, u64)>,
    cache_all: Option<f64>,
    first_cache: Option<f64>,
    second_cache: Option<f64>,
    clip_freq: Option<f64>,
    schema_fail_freq: Option<f64>,
    timeout_freq: Option<f64>,
    compression: Vec<Value>,
}

fn derive_metrics(runs: &[RunEntry], agg: Agg) -> (Agg, Derived) {
    let top_eff = top_avg(agg.tool_eff.clone());
    let top_dur = top_avg(agg.tool_dur.clone());
    let mut top_timeout_labels: Vec<(String, u64)> =
        agg.timeout_labels.clone().into_iter().collect();
    top_timeout_labels.sort_by(|a, b| b.1.cmp(&a.1));
    top_timeout_labels.truncate(5);
    let cache_all = (agg.sum_in > 0).then_some(agg.sum_cached as f64 / agg.sum_in as f64);
    let (first_cache, second_cache) = cache_halves(runs);
    let clip_freq =
        (agg.clipped_total > 0).then_some(agg.clipped_count as f64 / agg.clipped_total as f64);
    let schema_fail_freq =
        (agg.schema_total > 0).then_some(agg.schema_fails as f64 / agg.schema_total as f64);
    let timeout_freq = (!runs.is_empty()).then_some(agg.timeout_count as f64 / runs.len() as f64);
    let compression = compression_rows(agg.provider_stats.clone());
    (
        agg,
        Derived {
            top_eff,
            top_dur,
            top_timeout_labels,
            cache_all,
            first_cache,
            second_cache,
            clip_freq,
            schema_fail_freq,
            timeout_freq,
            compression,
        },
    )
}

fn analyze_runs(runs: &[RunEntry], max_ms: u64, max_eff: u64) -> (Agg, Derived) {
    let mut agg = Agg::default();
    for r in runs {
        agg.ingest(r, max_ms, max_eff);
    }
    derive_metrics(runs, agg)
}

fn build_scoreboard(total: u64, agg: &Agg, d: &Derived) -> Value {
    json!({
        "runs": total,
        "alerts": agg.alerts,
        "alerts_pct": if total == 0 { 0.0 } else { (agg.alerts as f64 / total as f64) * 100.0 },
        "top_avg_duration_ms": d.top_dur,
        "top_avg_effective_input_tokens": d.top_eff,
        "cache_hit_rate": d.cache_all,
        "cache_hit_trend": {
            "first_half": d.first_cache,
            "second_half": d.second_cache,
            "delta": match (d.first_cache, d.second_cache) {
                (Some(a), Some(b)) => Some(b - a),
                _ => None
            }
        },
        "schema_failure_frequency": {
            "schema_runs": agg.schema_total,
            "schema_failures": agg.schema_fails,
            "rate": d.schema_fail_freq
        },
        "timeout_frequency": {
            "timeout_runs": agg.timeout_count,
            "rate": d.timeout_freq,
            "top_labels": d.top_timeout_labels
        },
        "capture_provider_compression": d.compression,
        "budget_clipping_frequency": {
            "captured_runs": agg.clipped_total,
            "clipped_runs": agg.clipped_count,
            "rate": d.clip_freq
        }
    })
}

fn build_full_report(
    n: usize,
    total: u64,
    scoreboard: Value,
    anomalies: Vec<String>,
    recommendations: Vec<String>,
    log_file: &std::path::Path,
) -> Value {
    json!({
        "window": n,
        "runs": total,
        "scoreboard": scoreboard,
        "anomalies": anomalies,
        "recommendations": recommendations,
        "log_file": log_file.display().to_string()
    })
}

pub fn optimize_report(n: usize) -> Result<Value, String> {
    let Some(log_file) = resolve_log_file() else {
        return Err("unable to resolve log file".to_string());
    };
    if !log_file.exists() {
        return Ok(empty_report(n, &log_file));
    }
    let runs = load_runs(&log_file, n)?;
    if runs.is_empty() {
        return Ok(empty_report(n, &log_file));
    }

    let max_ms = env_u64("CXALERT_MAX_MS", 12000);
    let max_eff = env_u64("CXALERT_MAX_EFF_IN", 8000);
    let (agg, d) = analyze_runs(&runs, max_ms, max_eff);

    let anomalies = build_anomalies(
        &d.top_dur,
        &d.top_eff,
        max_ms,
        max_eff,
        d.first_cache,
        d.second_cache,
        d.schema_fail_freq,
        d.clip_freq,
        d.timeout_freq,
    );
    let recommendations = build_recommendations(
        &d.top_eff,
        d.first_cache,
        d.second_cache,
        agg.schema_fails,
        agg.timeout_count,
        &d.top_timeout_labels,
    );

    let total = runs.len() as u64;
    let scoreboard = build_scoreboard(total, &agg, &d);
    Ok(build_full_report(
        n,
        total,
        scoreboard,
        anomalies,
        recommendations,
        &log_file,
    ))
}

use serde_json::{Value, json};
use std::collections::HashMap;

use crate::contract_versions::OPTIMIZE_JSON_CONTRACT_VERSION;
use crate::diagnostics::sort_actions_by_phase7;
use crate::doctor::{exec_action_value, exec_diag_value, latest_run_all_sum, latest_wave_sum};
use crate::json_mode::resolve_json_mode;
use crate::logs::load_runs;
use crate::optimize_rules::{
    RecommendationInput, build_recommendations, push_cache_anomaly, push_clip_anomaly,
    push_latency_anomaly, push_retry_anomaly, push_schema_anomaly, push_timeout_anomaly,
    push_token_anomaly,
};
use crate::paths::resolve_log_file;
use crate::provider_adapter::selected_tq_caps;
use crate::types::RunEntry;

pub type OptimizeArgs = (usize, bool, bool, bool, Option<String>);

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn parse_severity_floor(raw: &str) -> Option<&'static str> {
    match raw {
        "warn" | "warning" => Some("warning"),
        "critical" => Some("critical"),
        _ => None,
    }
}

fn severity_rank(level: &str) -> i32 {
    match level {
        "critical" => 2,
        "warning" => 1,
        _ => 0,
    }
}

pub fn parse_optimize_args(args: &[String], default_n: usize) -> Result<OptimizeArgs, String> {
    let mut n = default_n;
    let mut json_out: Option<bool> = None;
    let mut actions = false;
    let mut strict = false;
    let mut severity_floor: Option<String> = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_out = Some(true);
                i += 1;
            }
            "--text" => {
                json_out = Some(false);
                i += 1;
            }
            "--actions" => {
                actions = true;
                i += 1;
            }
            "--strict" => {
                strict = true;
                i += 1;
            }
            "--severity" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    return Err("optimize: --severity requires a value".to_string());
                };
                let Some(normalized) = parse_severity_floor(v) else {
                    return Err("optimize: --severity must be warning|critical".to_string());
                };
                severity_floor = Some(normalized.to_string());
                i += 2;
            }
            a => {
                if let Ok(v) = a.parse::<usize>()
                    && v > 0
                {
                    n = v;
                    i += 1;
                    continue;
                }
                return Err(format!("invalid argument: {a}"));
            }
        }
    }
    Ok((
        n,
        resolve_json_mode(json_out, false),
        actions,
        strict,
        severity_floor,
    ))
}

fn empty_report(n: usize, log_file: &std::path::Path) -> Value {
    json!({
        "contract_version": OPTIMIZE_JSON_CONTRACT_VERSION,
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
    retry_rows_after_retry: u64,
    retry_rows_after_retry_success: u64,
    retry_task_timeout_seen: HashMap<String, bool>,
    retry_task_recovered: HashMap<String, bool>,
    retry_attempt_histogram: HashMap<u64, u64>,
    timing_task_rows: u64,
    timing_with_retry_attempt: u64,
    timing_with_queue_started_at: u64,
    timing_with_task_started_at: u64,
    timing_with_task_finished_at: u64,
    capture_prompt_rows_with_profile: u64,
    capture_prompt_shadow_narrow_configured: u64,
    capture_prompt_shadow_narrow_applied: u64,
    capture_prompt_shadow_narrow_fallback: u64,
    capture_prompt_applied_reducer_kinds: HashMap<String, u64>,
    capture_prompt_fallback_reasons: HashMap<String, u64>,
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
        let is_task_row = r
            .task_id
            .as_ref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        if is_task_row {
            self.timing_task_rows += 1;
            if r.retry_attempt.is_some() {
                self.timing_with_retry_attempt += 1;
            }
            if r.queue_started_at
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            {
                self.timing_with_queue_started_at += 1;
            }
            if r.task_started_at
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            {
                self.timing_with_task_started_at += 1;
            }
            if r.task_finished_at
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            {
                self.timing_with_task_finished_at += 1;
            }
        }
        if let Some(attempt) = r.retry_attempt.map(u64::from) {
            *self.retry_attempt_histogram.entry(attempt).or_insert(0) += 1;
            if attempt > 1 {
                self.retry_rows_after_retry += 1;
                if !r.timed_out.unwrap_or(false)
                    && r.policy_blocked != Some(true)
                    && r.schema_valid != Some(false)
                {
                    self.retry_rows_after_retry_success += 1;
                }
            }
            if let Some(task_id) = r.task_id.as_ref()
                && !task_id.trim().is_empty()
            {
                self.retry_task_timeout_seen
                    .entry(task_id.clone())
                    .or_insert(false);
                self.retry_task_recovered
                    .entry(task_id.clone())
                    .or_insert(false);
                if r.timed_out.unwrap_or(false) {
                    self.retry_task_timeout_seen.insert(task_id.clone(), true);
                } else if attempt > 1 {
                    self.retry_task_recovered.insert(task_id.clone(), true);
                }
            }
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
        if let Some(profile) = r
            .capture_prompt_profile
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            self.capture_prompt_rows_with_profile += 1;
            if profile == "shadow_narrow" {
                self.capture_prompt_shadow_narrow_configured += 1;
                if r.capture_prompt_profile_applied == Some(true) {
                    self.capture_prompt_shadow_narrow_applied += 1;
                    if let Some(kind) = r
                        .capture_prompt_reducer_kind
                        .as_ref()
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                    {
                        *self
                            .capture_prompt_applied_reducer_kinds
                            .entry(kind.to_string())
                            .or_insert(0) += 1;
                    }
                } else {
                    self.capture_prompt_shadow_narrow_fallback += 1;
                    if let Some(reason) = r
                        .capture_prompt_fallback_reason
                        .as_ref()
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                    {
                        *self
                            .capture_prompt_fallback_reasons
                            .entry(reason.to_string())
                            .or_insert(0) += 1;
                    }
                }
            }
        }
        self.sum_in += r.input_tokens.unwrap_or(0);
        self.sum_cached += r.cached_input_tokens.unwrap_or(0);
    }
}

fn top_avg(map: HashMap<String, (u64, u64)>) -> Vec<(String, u64)> {
    let mut rows: Vec<(String, u64)> = map
        .into_iter()
        .map(|(tool, (sum, count))| (tool, sum.checked_div(count).unwrap_or(0)))
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.1));
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

struct AnomalyInput<'a> {
    top_dur: &'a [(String, u64)],
    top_eff: &'a [(String, u64)],
    max_ms: u64,
    max_eff: u64,
    first_cache: Option<f64>,
    second_cache: Option<f64>,
    schema_fail_freq: Option<f64>,
    clip_freq: Option<f64>,
    timeout_freq: Option<f64>,
    retry_rows_rate: Option<f64>,
    retry_recovery_rate: Option<f64>,
    timing_coverage_min: Option<f64>,
}

fn build_anomalies(input: AnomalyInput<'_>) -> Vec<String> {
    let AnomalyInput {
        top_dur,
        top_eff,
        max_ms,
        max_eff,
        first_cache,
        second_cache,
        schema_fail_freq,
        clip_freq,
        timeout_freq,
        retry_rows_rate,
        retry_recovery_rate,
        timing_coverage_min,
    } = input;
    let mut anomalies: Vec<String> = Vec::new();
    push_latency_anomaly(&mut anomalies, top_dur, max_ms);
    push_token_anomaly(&mut anomalies, top_eff, max_eff);
    push_cache_anomaly(&mut anomalies, first_cache, second_cache);
    push_schema_anomaly(&mut anomalies, schema_fail_freq);
    push_clip_anomaly(&mut anomalies, clip_freq);
    push_timeout_anomaly(&mut anomalies, timeout_freq);
    push_retry_anomaly(&mut anomalies, retry_rows_rate, retry_recovery_rate);
    if let Some(min_cov) = timing_coverage_min
        && min_cov < 0.80
    {
        anomalies.push(format!(
            "Timing attribution coverage is low: min_field_coverage={}%",
            (min_cov * 100.0).round() as i64
        ));
    }
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
    retry_rows_rate: Option<f64>,
    retry_rows_success_rate: Option<f64>,
    retry_tasks_recovery_rate: Option<f64>,
    retry_tasks_with_timeout: u64,
    retry_tasks_recovered: u64,
    retry_attempt_histogram: Vec<(u64, u64)>,
    timing_retry_attempt_rate: Option<f64>,
    timing_queue_started_at_rate: Option<f64>,
    timing_task_started_at_rate: Option<f64>,
    timing_task_finished_at_rate: Option<f64>,
    timing_min_coverage_rate: Option<f64>,
    capture_prompt_applied_reducer_kinds: Vec<(String, u64)>,
    capture_prompt_fallback_reasons: Vec<(String, u64)>,
    capture_prompt_shadow_narrow_applied_rate: Option<f64>,
    capture_prompt_shadow_narrow_fallback_rate: Option<f64>,
}

fn derive_metrics(runs: &[RunEntry], agg: Agg) -> (Agg, Derived) {
    let top_eff = top_avg(agg.tool_eff.clone());
    let top_dur = top_avg(agg.tool_dur.clone());
    let mut top_timeout_labels: Vec<(String, u64)> =
        agg.timeout_labels.clone().into_iter().collect();
    top_timeout_labels.sort_by_key(|row| std::cmp::Reverse(row.1));
    top_timeout_labels.truncate(5);
    let cache_all = (agg.sum_in > 0).then_some(agg.sum_cached as f64 / agg.sum_in as f64);
    let (first_cache, second_cache) = cache_halves(runs);
    let clip_freq =
        (agg.clipped_total > 0).then_some(agg.clipped_count as f64 / agg.clipped_total as f64);
    let schema_fail_freq =
        (agg.schema_total > 0).then_some(agg.schema_fails as f64 / agg.schema_total as f64);
    let timeout_freq = (!runs.is_empty()).then_some(agg.timeout_count as f64 / runs.len() as f64);
    let retry_rows_rate =
        (!runs.is_empty()).then_some(agg.retry_rows_after_retry as f64 / runs.len() as f64);
    let retry_rows_success_rate = (agg.retry_rows_after_retry > 0)
        .then_some(agg.retry_rows_after_retry_success as f64 / agg.retry_rows_after_retry as f64);
    let retry_tasks_with_timeout = agg
        .retry_task_timeout_seen
        .iter()
        .filter(|(_, saw)| **saw)
        .count() as u64;
    let retry_tasks_recovered = agg
        .retry_task_timeout_seen
        .iter()
        .filter(|(task, saw)| **saw && agg.retry_task_recovered.get(*task) == Some(&true))
        .count() as u64;
    let retry_tasks_recovery_rate = (retry_tasks_with_timeout > 0)
        .then_some(retry_tasks_recovered as f64 / retry_tasks_with_timeout as f64);
    let mut retry_attempt_histogram: Vec<(u64, u64)> =
        agg.retry_attempt_histogram.clone().into_iter().collect();
    retry_attempt_histogram.sort_by_key(|row| row.0);
    let timing_retry_attempt_rate = (agg.timing_task_rows > 0)
        .then_some(agg.timing_with_retry_attempt as f64 / agg.timing_task_rows as f64);
    let timing_queue_started_at_rate = (agg.timing_task_rows > 0)
        .then_some(agg.timing_with_queue_started_at as f64 / agg.timing_task_rows as f64);
    let timing_task_started_at_rate = (agg.timing_task_rows > 0)
        .then_some(agg.timing_with_task_started_at as f64 / agg.timing_task_rows as f64);
    let timing_task_finished_at_rate = (agg.timing_task_rows > 0)
        .then_some(agg.timing_with_task_finished_at as f64 / agg.timing_task_rows as f64);
    let timing_min_coverage_rate = [
        timing_retry_attempt_rate,
        timing_queue_started_at_rate,
        timing_task_started_at_rate,
        timing_task_finished_at_rate,
    ]
    .into_iter()
    .flatten()
    .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let compression = compression_rows(agg.provider_stats.clone());
    let mut capture_prompt_applied_reducer_kinds: Vec<(String, u64)> = agg
        .capture_prompt_applied_reducer_kinds
        .clone()
        .into_iter()
        .collect();
    capture_prompt_applied_reducer_kinds.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut capture_prompt_fallback_reasons: Vec<(String, u64)> = agg
        .capture_prompt_fallback_reasons
        .clone()
        .into_iter()
        .collect();
    capture_prompt_fallback_reasons.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let capture_prompt_shadow_narrow_applied_rate =
        (agg.capture_prompt_shadow_narrow_configured > 0).then_some(
            agg.capture_prompt_shadow_narrow_applied as f64
                / agg.capture_prompt_shadow_narrow_configured as f64,
        );
    let capture_prompt_shadow_narrow_fallback_rate =
        (agg.capture_prompt_shadow_narrow_configured > 0).then_some(
            agg.capture_prompt_shadow_narrow_fallback as f64
                / agg.capture_prompt_shadow_narrow_configured as f64,
        );
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
            retry_rows_rate,
            retry_rows_success_rate,
            retry_tasks_recovery_rate,
            retry_tasks_with_timeout,
            retry_tasks_recovered,
            retry_attempt_histogram,
            timing_retry_attempt_rate,
            timing_queue_started_at_rate,
            timing_task_started_at_rate,
            timing_task_finished_at_rate,
            timing_min_coverage_rate,
            capture_prompt_applied_reducer_kinds,
            capture_prompt_fallback_reasons,
            capture_prompt_shadow_narrow_applied_rate,
            capture_prompt_shadow_narrow_fallback_rate,
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
        "retry_health": {
            "rows_after_retry": agg.retry_rows_after_retry,
            "rows_after_retry_success": agg.retry_rows_after_retry_success,
            "rows_after_retry_rate": d.retry_rows_rate,
            "rows_after_retry_success_rate": d.retry_rows_success_rate,
            "tasks_with_timeout": d.retry_tasks_with_timeout,
            "tasks_recovered": d.retry_tasks_recovered,
            "tasks_recovery_rate": d.retry_tasks_recovery_rate,
            "attempt_histogram": d.retry_attempt_histogram
        },
        "timing_attribution_coverage": {
            "task_rows": agg.timing_task_rows,
            "rows_with_retry_attempt": agg.timing_with_retry_attempt,
            "rows_with_queue_started_at": agg.timing_with_queue_started_at,
            "rows_with_task_started_at": agg.timing_with_task_started_at,
            "rows_with_task_finished_at": agg.timing_with_task_finished_at,
            "retry_attempt_rate": d.timing_retry_attempt_rate,
            "queue_started_at_rate": d.timing_queue_started_at_rate,
            "task_started_at_rate": d.timing_task_started_at_rate,
            "task_finished_at_rate": d.timing_task_finished_at_rate,
            "min_coverage_rate": d.timing_min_coverage_rate
        },
        "capture_provider_compression": d.compression,
        "capture_prompt_profile_rollout": {
            "rows_with_explicit_profile": agg.capture_prompt_rows_with_profile,
            "shadow_narrow_configured_runs": agg.capture_prompt_shadow_narrow_configured,
            "shadow_narrow_applied_runs": agg.capture_prompt_shadow_narrow_applied,
            "shadow_narrow_fallback_runs": agg.capture_prompt_shadow_narrow_fallback,
            "shadow_narrow_applied_rate": d.capture_prompt_shadow_narrow_applied_rate,
            "shadow_narrow_fallback_rate": d.capture_prompt_shadow_narrow_fallback_rate,
            "applied_reducer_kinds": d.capture_prompt_applied_reducer_kinds,
            "fallback_reasons": d.capture_prompt_fallback_reasons
        },
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
    let experiment_caps = selected_tq_caps();
    json!({
        "contract_version": OPTIMIZE_JSON_CONTRACT_VERSION,
        "window": n,
        "runs": total,
        "backend_capabilities": {
            "turboquant": {
                "cx_runtime_support": experiment_caps.turboquant_runtime_support,
                "selected_backend_role": experiment_caps.turboquant_backend_role,
                "memory_metric_kind": experiment_caps.turboquant_metric_kind,
            }
        },
        "scoreboard": scoreboard,
        "anomalies": anomalies,
        "recommendations": recommendations,
        "log_file": log_file.display().to_string()
    })
}

pub fn build_optimize_actions(report: &Value) -> Vec<Value> {
    let mut actions: Vec<Value> = Vec::new();
    let latest_run = latest_run_all_sum();
    let latest_wave = latest_wave_sum();
    let task_execution = exec_diag_value(latest_run.as_ref(), latest_wave.as_ref());
    if let Some(exec_action) = exec_action_value(&task_execution) {
        actions.push(exec_action);
    }
    let Some(scoreboard) = report.get("scoreboard") else {
        return actions;
    };
    if let Some(top_dur) = scoreboard
        .get("top_avg_duration_ms")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(Value::as_array)
        && top_dur.len() == 2
    {
        let tool = top_dur[0].as_str().unwrap_or("unknown");
        let dur = top_dur[1].as_u64().unwrap_or(0);
        if dur > 0 {
            actions.push(json!({
                "id": "latency_hotspot",
                "severity": "warning",
                "rationale": format!("Highest average duration is concentrated on {tool} ({dur}ms)."),
                "command": crate::config::command_with_cli("optimize 200 --json --actions")
            }));
        }
    }
    let schema_rate = scoreboard
        .get("schema_failure_frequency")
        .and_then(|v| v.get("rate"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if schema_rate > 0.0 {
        actions.push(json!({
            "id": "schema_failure_frequency",
            "severity": if schema_rate > 0.05 { "critical" } else { "warning" },
            "rationale": format!("Schema failure rate detected at {}%.", (schema_rate * 100.0).round() as i64),
            "command": crate::config::command_with_cli("diag --json --strict --actions")
        }));
    }
    let timeout_rate = scoreboard
        .get("timeout_frequency")
        .and_then(|v| v.get("rate"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if timeout_rate > 0.03 {
        actions.push(json!({
            "id": "timeout_frequency",
            "severity": "warning",
            "rationale": format!("Timeout frequency is elevated at {}% of runs.", (timeout_rate * 100.0).round() as i64),
            "command": crate::config::command_with_cli("logs stats 200 --json --strict")
        }));
    }
    let clip_rate = scoreboard
        .get("budget_clipping_frequency")
        .and_then(|v| v.get("rate"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if clip_rate > 0.30 {
        actions.push(json!({
            "id": "budget_clipping_frequency",
            "severity": "warning",
            "rationale": format!("Budget clipping occurs in {}% of captured runs.", (clip_rate * 100.0).round() as i64),
            "command": crate::config::command_with_cli("budget")
        }));
    }
    let timing_cov = scoreboard
        .get("timing_attribution_coverage")
        .and_then(|v| v.get("min_coverage_rate"))
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    if timing_cov < 0.80 {
        actions.push(json!({
            "id": "timing_attribution_coverage_low",
            "severity": "warning",
            "rationale": format!("Task timing attribution coverage is low at {}% minimum field population.", (timing_cov * 100.0).round() as i64),
            "command": crate::config::command_with_cli("scheduler --json --window 200")
        }));
    }
    let cache_delta = scoreboard
        .get("cache_hit_trend")
        .and_then(|v| v.get("delta"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if cache_delta < -0.05 {
        actions.push(json!({
            "id": "cache_hit_degradation",
            "severity": "warning",
            "rationale": format!("Cache hit trend regressed by {} percentage points.", (cache_delta * 100.0).round() as i64),
            "command": crate::config::command_with_cli("promptlint 200")
        }));
    }
    let capture_prompt_rollout = scoreboard.get("capture_prompt_profile_rollout");
    let capture_prompt_fallback_rate = capture_prompt_rollout
        .and_then(|v| v.get("shadow_narrow_fallback_rate"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let capture_prompt_configured_runs = capture_prompt_rollout
        .and_then(|v| v.get("shadow_narrow_configured_runs"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let capture_prompt_applied_runs = capture_prompt_rollout
        .and_then(|v| v.get("shadow_narrow_applied_runs"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if capture_prompt_configured_runs > 0
        && (capture_prompt_fallback_rate > 0.0 || capture_prompt_applied_runs == 0)
    {
        let rationale = if capture_prompt_applied_runs == 0 {
            format!(
                "shadow_narrow was configured for {} runs but never applied; review reducer eligibility and fallback reasons.",
                capture_prompt_configured_runs
            )
        } else {
            format!(
                "shadow_narrow fell back in {}% of configured runs; inspect reducer mix and fallback reasons.",
                (capture_prompt_fallback_rate * 100.0).round() as i64
            )
        };
        actions.push(json!({
            "id": "capture_prompt_rollout_followup",
            "severity": "warning",
            "rationale": rationale,
            "command": crate::config::command_with_cli("telemetry 200 --json")
        }));
    }
    if actions.len() > 1 {
        sort_actions_by_phase7(&mut actions[1..]);
    }
    actions
}

pub fn action_gate_severity(actions: &[Value]) -> &'static str {
    let mut max_level = "ok";
    for action in actions {
        let level = action
            .get("severity")
            .and_then(Value::as_str)
            .unwrap_or("ok");
        if severity_rank(level) > severity_rank(max_level) {
            max_level = if level == "critical" {
                "critical"
            } else {
                "warning"
            };
        }
    }
    max_level
}

pub fn should_fail_strict(strict: bool, severity_floor: Option<&str>, actions: &[Value]) -> bool {
    if !strict {
        return false;
    }
    let threshold = severity_floor.unwrap_or("warning");
    severity_rank(action_gate_severity(actions)) >= severity_rank(threshold)
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

    let anomalies = build_anomalies(AnomalyInput {
        top_dur: &d.top_dur,
        top_eff: &d.top_eff,
        max_ms,
        max_eff,
        first_cache: d.first_cache,
        second_cache: d.second_cache,
        schema_fail_freq: d.schema_fail_freq,
        clip_freq: d.clip_freq,
        timeout_freq: d.timeout_freq,
        retry_rows_rate: d.retry_rows_rate,
        retry_recovery_rate: d.retry_tasks_recovery_rate,
        timing_coverage_min: d.timing_min_coverage_rate,
    });
    let recommendations = build_recommendations(RecommendationInput {
        top_eff: &d.top_eff,
        first_cache: d.first_cache,
        second_cache: d.second_cache,
        schema_fails: agg.schema_fails,
        timeout_count: agg.timeout_count,
        top_timeout_labels: &d.top_timeout_labels,
        retry_rows_rate: d.retry_rows_rate,
        retry_recovery_rate: d.retry_tasks_recovery_rate,
        timing_coverage_min: d.timing_min_coverage_rate,
    });
    let mut recommendations = recommendations;
    if agg.capture_prompt_shadow_narrow_configured > 0 {
        if agg.capture_prompt_shadow_narrow_applied == 0 {
            recommendations.push(format!(
                "shadow_narrow is configured but never applied across {} runs; inspect capture_prompt_telemetry before widening rollout.",
                agg.capture_prompt_shadow_narrow_configured
            ));
        } else if agg.capture_prompt_shadow_narrow_fallback > 0 {
            recommendations.push(format!(
                "shadow_narrow fallback observed in {} of {} configured runs; review fallback reasons and reducer eligibility in capture_prompt_telemetry.",
                agg.capture_prompt_shadow_narrow_fallback,
                agg.capture_prompt_shadow_narrow_configured
            ));
        } else {
            recommendations.push(format!(
                "shadow_narrow applied cleanly across {} configured runs; use capture_prompt_profile_rollout to decide whether more reducer classes need fixture coverage.",
                agg.capture_prompt_shadow_narrow_configured
            ));
        }
    }

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

#[cfg(test)]
mod tests {
    use super::build_optimize_actions;
    use serde_json::Value;
    use serde_json::json;

    #[test]
    fn optimize_order_cov() {
        let report = json!({
            "scoreboard": {
                "top_avg_duration_ms": [["cxo", 4200]],
                "schema_failure_frequency": { "rate": 0.02 },
                "timeout_frequency": { "rate": 0.06 },
                "budget_clipping_frequency": { "rate": 0.0 },
                "timing_attribution_coverage": { "min_coverage_rate": 1.0 },
                "cache_hit_trend": { "delta": 0.0 }
            }
        });
        let actions = build_optimize_actions(&report);
        let diag_idx = actions
            .iter()
            .position(|action| {
                action.get("command").and_then(Value::as_str)
                    == Some(
                        crate::config::command_with_cli("diag --json --strict --actions").as_str(),
                    )
            })
            .expect("diag action");
        let logs_idx = actions
            .iter()
            .position(|action| {
                action.get("command").and_then(Value::as_str)
                    == Some(
                        crate::config::command_with_cli("logs stats 200 --json --strict").as_str(),
                    )
            })
            .expect("logs action");
        let optimize_idx = actions
            .iter()
            .position(|action| {
                action.get("command").and_then(Value::as_str)
                    == Some(
                        crate::config::command_with_cli("optimize 200 --json --actions").as_str(),
                    )
            })
            .expect("optimize action");
        assert!(diag_idx < logs_idx, "{actions:?}");
        assert!(diag_idx < optimize_idx, "{actions:?}");
    }
}

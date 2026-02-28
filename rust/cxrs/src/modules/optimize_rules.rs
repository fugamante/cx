pub struct RecommendationInput<'a> {
    pub top_eff: &'a [(String, u64)],
    pub first_cache: Option<f64>,
    pub second_cache: Option<f64>,
    pub schema_fails: u64,
    pub timeout_count: u64,
    pub top_timeout_labels: &'a [(String, u64)],
    pub retry_rows_rate: Option<f64>,
    pub retry_recovery_rate: Option<f64>,
}

pub fn push_latency_anomaly(anomalies: &mut Vec<String>, top_dur: &[(String, u64)], max_ms: u64) {
    if let Some((tool, avg)) = top_dur.first()
        && *avg > max_ms / 2
    {
        anomalies.push(format!(
            "High latency concentration: {tool} avg_duration_ms={avg}"
        ));
    }
}

pub fn push_token_anomaly(anomalies: &mut Vec<String>, top_eff: &[(String, u64)], max_eff: u64) {
    if let Some((tool, avg)) = top_eff.first()
        && *avg > max_eff / 2
    {
        anomalies.push(format!(
            "High token load concentration: {tool} avg_effective_input_tokens={avg}"
        ));
    }
}

pub fn push_cache_anomaly(
    anomalies: &mut Vec<String>,
    first_cache: Option<f64>,
    second_cache: Option<f64>,
) {
    if let (Some(a), Some(b)) = (first_cache, second_cache)
        && b + 0.05 < a
    {
        anomalies.push(format!(
            "Cache hit degraded: first_half={}%, second_half={}%,",
            (a * 100.0).round() as i64,
            (b * 100.0).round() as i64
        ));
    }
}

pub fn push_schema_anomaly(anomalies: &mut Vec<String>, schema_fail_freq: Option<f64>) {
    if let Some(freq) = schema_fail_freq
        && freq > 0.05
    {
        anomalies.push(format!(
            "Schema failure frequency elevated: {}%",
            (freq * 100.0).round() as i64
        ));
    }
}

pub fn push_clip_anomaly(anomalies: &mut Vec<String>, clip_freq: Option<f64>) {
    if let Some(freq) = clip_freq
        && freq > 0.30
    {
        anomalies.push(format!(
            "Budget clipping frequent: {}% of captured runs",
            (freq * 100.0).round() as i64
        ));
    }
}

pub fn push_timeout_anomaly(anomalies: &mut Vec<String>, timeout_freq: Option<f64>) {
    if let Some(freq) = timeout_freq
        && freq > 0.03
    {
        anomalies.push(format!(
            "Timeout frequency elevated: {}% of runs",
            (freq * 100.0).round() as i64
        ));
    }
}

pub fn push_retry_anomaly(
    anomalies: &mut Vec<String>,
    retry_rows_rate: Option<f64>,
    retry_recovery_rate: Option<f64>,
) {
    if let Some(rate) = retry_rows_rate
        && rate > 0.10
    {
        anomalies.push(format!(
            "Retry pressure elevated: {}% of runs are attempt>1",
            (rate * 100.0).round() as i64
        ));
    }
    if let Some(rate) = retry_recovery_rate
        && rate < 0.70
    {
        anomalies.push(format!(
            "Retry recovery weak: only {}% of timed-out tasks recover",
            (rate * 100.0).round() as i64
        ));
    }
}

pub fn build_recommendations(input: RecommendationInput<'_>) -> Vec<String> {
    let RecommendationInput {
        top_eff,
        first_cache,
        second_cache,
        schema_fails,
        timeout_count,
        top_timeout_labels,
        retry_rows_rate,
        retry_recovery_rate,
    } = input;
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
    if timeout_count > 0 {
        let label = top_timeout_labels
            .first()
            .map(|row| row.0.as_str())
            .unwrap_or("long-running command");
        recommendations.push(format!(
            "Timeouts detected around '{label}'; increase CX_CMD_TIMEOUT_SECS/CX_TIMEOUT_* or reduce prompt/capture scope."
        ));
    }
    if let Some(rate) = retry_rows_rate
        && rate > 0.10
    {
        recommendations.push(format!(
            "Retry attempt volume is high ({}% rows with attempt>1); reduce flaky commands and narrow captured context per task.",
            (rate * 100.0).round() as i64
        ));
    }
    if let Some(rate) = retry_recovery_rate
        && rate < 0.70
    {
        recommendations.push(format!(
            "Retry recovery is low ({}%); tune timeout overrides per command label and split heavy objectives into smaller tasks.",
            (rate * 100.0).round() as i64
        ));
    }
    if recommendations.is_empty() {
        recommendations.push("No significant anomalies in this window.".to_string());
    }
    recommendations
}

use serde_json::{Value, json};

use super::catalog::maybe_auto_refresh_quota_catalog;
use crate::runtime::{llm_backend, llm_model};
use crate::state::{read_state_value, value_at_path};

fn quota_tier_for_backend(backend: &str) -> String {
    let env_backend = format!("CX_QUOTA_{}_TIER", backend.to_uppercase());
    if let Ok(v) = std::env::var(&env_backend) {
        let tier = v.trim().to_lowercase();
        if !tier.is_empty() {
            return tier;
        }
    }
    if let Ok(v) = std::env::var("CX_QUOTA_TIER") {
        let tier = v.trim().to_lowercase();
        if !tier.is_empty() {
            return tier;
        }
    }
    if let Some(state) = read_state_value() {
        let key = format!("preferences.quota_tier.{backend}");
        if let Some(v) = value_at_path(&state, &key).and_then(Value::as_str) {
            let tier = v.trim().to_lowercase();
            if !tier.is_empty() {
                return tier;
            }
        }
        if let Some(v) =
            value_at_path(&state, "preferences.quota_tier.default").and_then(Value::as_str)
        {
            let tier = v.trim().to_lowercase();
            if !tier.is_empty() {
                return tier;
            }
        }
    }
    "free".to_string()
}

struct QuotaResolution {
    total_tokens: Option<u64>,
    source: String,
    tier: String,
    limit_type: String,
    source_url: Option<String>,
}

fn configured_quota_total(backend: &str) -> QuotaResolution {
    let tier = quota_tier_for_backend(backend);
    let env_backend = format!("CX_QUOTA_{}_TOTAL_TOKENS", backend.to_uppercase());
    if let Ok(v) = std::env::var(&env_backend)
        && let Ok(parsed) = v.trim().parse::<u64>()
    {
        return QuotaResolution {
            total_tokens: Some(parsed),
            source: format!("env:{env_backend}"),
            tier,
            limit_type: "hard".to_string(),
            source_url: None,
        };
    }
    if let Ok(v) = std::env::var("CX_QUOTA_TOTAL_TOKENS")
        && let Ok(parsed) = v.trim().parse::<u64>()
    {
        return QuotaResolution {
            total_tokens: Some(parsed),
            source: "env:CX_QUOTA_TOTAL_TOKENS".to_string(),
            tier,
            limit_type: "hard".to_string(),
            source_url: None,
        };
    }
    if let Some(state) = read_state_value() {
        let key = format!("preferences.quota.{}_total_tokens", backend);
        if let Some(v) = value_at_path(&state, &key)
            && let Some(parsed) = v.as_u64()
        {
            return QuotaResolution {
                total_tokens: Some(parsed),
                source: format!("state:{key}"),
                tier,
                limit_type: "hard".to_string(),
                source_url: None,
            };
        }
        let key_default = "preferences.quota.default_total_tokens";
        if let Some(v) = value_at_path(&state, key_default)
            && let Some(parsed) = v.as_u64()
        {
            return QuotaResolution {
                total_tokens: Some(parsed),
                source: format!("state:{key_default}"),
                tier,
                limit_type: "hard".to_string(),
                source_url: None,
            };
        }
    }

    if let Some(catalog) = maybe_auto_refresh_quota_catalog()
        && let Some(entries) = catalog.get("entries").and_then(Value::as_array)
    {
        let mut matched: Option<&Value> = None;
        for row in entries {
            if row.get("backend").and_then(Value::as_str) == Some(backend)
                && row.get("tier").and_then(Value::as_str) == Some(tier.as_str())
            {
                matched = Some(row);
                break;
            }
        }
        if matched.is_none() {
            for row in entries {
                if row.get("backend").and_then(Value::as_str) == Some(backend)
                    && row.get("tier").and_then(Value::as_str) == Some("default")
                {
                    matched = Some(row);
                    break;
                }
            }
        }
        if let Some(row) = matched {
            let total = row.get("quota_total_tokens").and_then(Value::as_u64);
            let limit_type = row
                .get("limit_type")
                .and_then(Value::as_str)
                .unwrap_or("dynamic")
                .to_string();
            let source_url = row
                .get("source_url")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            return QuotaResolution {
                total_tokens: total,
                source: format!("catalog:{backend}:{tier}"),
                tier,
                limit_type,
                source_url,
            };
        }
    }
    QuotaResolution {
        total_tokens: None,
        source: "unknown".to_string(),
        tier,
        limit_type: "unknown".to_string(),
        source_url: None,
    }
}

pub(super) fn quota_probe_payload(
    days: usize,
    log_file: &std::path::Path,
    rows: &[Value],
    backend_override: Option<&str>,
    model_override: Option<&str>,
) -> Value {
    let backend = backend_override
        .map(|v| v.to_string())
        .unwrap_or_else(llm_backend);
    let model = model_override
        .map(|v| v.to_string())
        .unwrap_or_else(llm_model);
    let used_effective: u64 = rows
        .iter()
        .map(|r| {
            r.get("effective_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| r.get("input_tokens").and_then(Value::as_u64).unwrap_or(0))
        })
        .sum();

    let mut resolved = configured_quota_total(&backend);
    let (service_kind, total, remaining, remaining_pct) = if backend == "ollama" {
        resolved.source = "service:local_unmetered".to_string();
        resolved.limit_type = "unmetered".to_string();
        ("local_unmetered", Value::Null, Value::Null, Value::Null)
    } else if let Some(total_tokens) = resolved.total_tokens {
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
        "quota_tier": resolved.tier,
        "service_kind": service_kind,
        "quota_source": resolved.source,
        "quota_limit_type": resolved.limit_type,
        "quota_source_url": resolved.source_url,
        "quota_total_tokens": total,
        "quota_used_tokens_window": used_effective,
        "quota_remaining_tokens": remaining,
        "quota_remaining_pct": remaining_pct
    })
}

use chrono::Utc;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

use crate::paths::resolve_quota_catalog_file;
use crate::state::{read_state_value, set_state_path, value_at_path, write_json_atomic};

pub(super) fn quota_catalog_path() -> Option<PathBuf> {
    resolve_quota_catalog_file()
}

fn embedded_quota_catalog() -> Value {
    let now = Utc::now().to_rfc3339();
    json!({
        "version": 1,
        "updated_at": now,
        "notes": "Catalog of provider-published quota/rate-limit statements. Values may be dynamic; verify against source URLs.",
        "sources": [
            { "provider":"openai", "url":"https://help.openai.com/en/articles/11909943-gpt-53-and-52-in-chatgpt", "kind":"help_center" },
            { "provider":"openai", "url":"https://help.openai.com/en/articles/12003714-chatgpt-business-models-limits", "kind":"help_center" },
            { "provider":"openai", "url":"https://help.openai.com/en/articles/9793128/", "kind":"help_center" },
            { "provider":"anthropic", "url":"https://support.anthropic.com/en/articles/8324991-about-claude-s-pro-plan-usage", "kind":"support" },
            { "provider":"google", "url":"https://gemini.google/us/subscriptions/?hl=en", "kind":"marketing" },
            { "provider":"perplexity", "url":"https://docs.perplexity.ai/docs/admin/rate-limits-usage-tiers", "kind":"docs" }
        ],
        "entries": [
            { "provider":"openai", "backend":"codex", "tier":"free", "service_kind":"remote_metered", "limit_type":"dynamic", "quota_total_tokens": null, "window_days": null, "source_url":"https://help.openai.com/en/articles/11909943-gpt-53-and-52-in-chatgpt", "retrieved_at": now, "notes":"Public ChatGPT limits are product/tier-dependent and can change." },
            { "provider":"openai", "backend":"codex", "tier":"plus", "service_kind":"remote_metered", "limit_type":"dynamic", "quota_total_tokens": null, "window_days": null, "source_url":"https://help.openai.com/en/articles/11909943-gpt-53-and-52-in-chatgpt", "retrieved_at": now, "notes":"Plus limits may vary by model and rolling window." },
            { "provider":"openai", "backend":"codex", "tier":"pro", "service_kind":"remote_metered", "limit_type":"dynamic", "quota_total_tokens": null, "window_days": null, "source_url":"https://help.openai.com/en/articles/9793128/", "retrieved_at": now, "notes":"Pro may be described with dynamic or guardrail-based usage." },
            { "provider":"openai", "backend":"codex", "tier":"business", "service_kind":"remote_metered", "limit_type":"dynamic", "quota_total_tokens": null, "window_days": null, "source_url":"https://help.openai.com/en/articles/12003714-chatgpt-business-models-limits", "retrieved_at": now, "notes":"Business limits are model-specific and may be updated." },
            { "provider":"anthropic", "backend":"anthropic", "tier":"free", "service_kind":"remote_metered", "limit_type":"dynamic", "quota_total_tokens": null, "window_days": null, "source_url":"https://support.anthropic.com/en/articles/8324991-about-claude-s-pro-plan-usage", "retrieved_at": now, "notes":"Anthropic usage limits are dynamic." },
            { "provider":"anthropic", "backend":"anthropic", "tier":"pro", "service_kind":"remote_metered", "limit_type":"dynamic", "quota_total_tokens": null, "window_days": null, "source_url":"https://support.anthropic.com/en/articles/8324991-about-claude-s-pro-plan-usage", "retrieved_at": now, "notes":"Anthropic Pro usage is dynamic." },
            { "provider":"google", "backend":"gemini", "tier":"free", "service_kind":"remote_metered", "limit_type":"dynamic", "quota_total_tokens": null, "window_days": null, "source_url":"https://gemini.google/us/subscriptions/?hl=en", "retrieved_at": now, "notes":"Gemini subscription pages publish feature limits; quotas may change." },
            { "provider":"google", "backend":"gemini", "tier":"advanced", "service_kind":"remote_metered", "limit_type":"dynamic", "quota_total_tokens": null, "window_days": null, "source_url":"https://gemini.google/us/subscriptions/?hl=en", "retrieved_at": now, "notes":"Gemini Advanced limits may vary over time." },
            { "provider":"perplexity", "backend":"perplexity", "tier":"free", "service_kind":"remote_metered", "limit_type":"rate_limited", "quota_total_tokens": null, "window_days": null, "source_url":"https://docs.perplexity.ai/docs/admin/rate-limits-usage-tiers", "retrieved_at": now, "notes":"Perplexity documents request/rate tiers rather than token budgets." },
            { "provider":"ollama", "backend":"ollama", "tier":"local", "service_kind":"local_unmetered", "limit_type":"unmetered", "quota_total_tokens": null, "window_days": null, "source_url":"https://ollama.com", "retrieved_at": now, "notes":"Local inference; no provider-enforced quota." }
        ]
    })
}

fn load_quota_catalog() -> Option<Value> {
    let path = quota_catalog_path()?;
    if !path.exists() {
        return None;
    }
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str::<Value>(&text).ok()
}

#[derive(Debug, Clone)]
struct CatalogAutoConfig {
    enabled: bool,
    interval_hours: u64,
}

fn catalog_auto_config_from_state() -> CatalogAutoConfig {
    let mut cfg = CatalogAutoConfig {
        enabled: false,
        interval_hours: 168,
    };
    if let Some(state) = read_state_value() {
        if let Some(v) =
            value_at_path(&state, "preferences.quota_catalog.auto.enabled").and_then(Value::as_bool)
        {
            cfg.enabled = v;
        }
        if let Some(v) = value_at_path(&state, "preferences.quota_catalog.auto.interval_hours")
            .and_then(Value::as_u64)
        {
            cfg.interval_hours = v.clamp(1, 24 * 365);
        }
    }
    cfg
}

fn catalog_age_hours(catalog: &Value) -> Option<u64> {
    let ts = catalog.get("updated_at")?.as_str()?;
    let dt = chrono::DateTime::parse_from_rfc3339(ts).ok()?;
    let now = Utc::now();
    let age = now.signed_duration_since(dt.with_timezone(&Utc));
    if age.num_hours() < 0 {
        Some(0)
    } else {
        Some(age.num_hours() as u64)
    }
}

fn quota_catalog_refresh_now() -> Result<Value, String> {
    let Some(path) = quota_catalog_path() else {
        return Err("quota catalog: unable to resolve catalog path".to_string());
    };
    let catalog = embedded_quota_catalog();
    write_json_atomic(&path, &catalog)?;
    Ok(catalog)
}

pub(super) fn maybe_auto_refresh_quota_catalog() -> Option<Value> {
    let cfg = catalog_auto_config_from_state();
    if !cfg.enabled {
        return load_quota_catalog();
    }
    if let Some(current) = load_quota_catalog()
        && let Some(age_h) = catalog_age_hours(&current)
        && age_h < cfg.interval_hours
    {
        return Some(current);
    }
    match quota_catalog_refresh_now() {
        Ok(v) => Some(v),
        Err(e) => {
            crate::cx_eprintln!("quota catalog auto-refresh: {e}");
            load_quota_catalog()
        }
    }
}

pub(super) fn cmd_quota_catalog(args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("show");
    let as_json = args.iter().any(|a| a == "--json");
    match sub {
        "refresh" => {
            let mut if_stale = false;
            let mut max_age_hours: u64 = 168;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => i += 1,
                    "--if-stale" => {
                        if_stale = true;
                        i += 1;
                    }
                    "--max-age-hours" => {
                        let Some(v) = args.get(i + 1) else {
                            crate::cx_eprintln!(
                                "quota catalog refresh: --max-age-hours requires a value"
                            );
                            return 2;
                        };
                        match v.trim().parse::<u64>() {
                            Ok(parsed) if parsed > 0 => max_age_hours = parsed,
                            _ => {
                                crate::cx_eprintln!(
                                    "quota catalog refresh: --max-age-hours must be >= 1"
                                );
                                return 2;
                            }
                        }
                        i += 2;
                    }
                    other => {
                        crate::cx_eprintln!("quota catalog refresh: unknown arg '{other}'");
                        return 2;
                    }
                }
            }
            let Some(path) = quota_catalog_path() else {
                crate::cx_eprintln!("quota catalog: unable to resolve catalog path");
                return 1;
            };
            let mut refreshed = true;
            let catalog = if if_stale {
                if let Some(current) = load_quota_catalog() {
                    if let Some(age_h) = catalog_age_hours(&current)
                        && age_h < max_age_hours
                    {
                        refreshed = false;
                        current
                    } else {
                        match quota_catalog_refresh_now() {
                            Ok(v) => v,
                            Err(e) => {
                                crate::cx_eprintln!("quota catalog refresh: {e}");
                                return 1;
                            }
                        }
                    }
                } else {
                    match quota_catalog_refresh_now() {
                        Ok(v) => v,
                        Err(e) => {
                            crate::cx_eprintln!("quota catalog refresh: {e}");
                            return 1;
                        }
                    }
                }
            } else {
                match quota_catalog_refresh_now() {
                    Ok(v) => v,
                    Err(e) => {
                        crate::cx_eprintln!("quota catalog refresh: {e}");
                        return 1;
                    }
                }
            };
            if as_json {
                let payload = json!({
                    "refreshed": refreshed,
                    "catalog": catalog
                });
                match serde_json::to_string_pretty(&payload) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        crate::cx_eprintln!("quota catalog refresh: failed to render json: {e}");
                        return 1;
                    }
                }
            } else {
                let entries = catalog
                    .get("entries")
                    .and_then(Value::as_array)
                    .map(|a| a.len())
                    .unwrap_or(0);
                println!("quota_catalog_refreshed: {}", path.display());
                println!("refreshed: {}", if refreshed { "true" } else { "false" });
                println!("entries: {entries}");
            }
            0
        }
        "show" => {
            let Some(path) = quota_catalog_path() else {
                crate::cx_eprintln!("quota catalog: unable to resolve catalog path");
                return 1;
            };
            let catalog = maybe_auto_refresh_quota_catalog().unwrap_or_else(|| {
                json!({
                    "version": 1,
                    "updated_at": Value::Null,
                    "entries": [],
                    "sources": [],
                    "notes": "Catalog missing. Run: cx quota catalog refresh"
                })
            });
            if as_json {
                match serde_json::to_string_pretty(&catalog) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        crate::cx_eprintln!("quota catalog show: failed to render json: {e}");
                        return 1;
                    }
                }
                return 0;
            }
            let entries = catalog
                .get("entries")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            println!("== cx quota catalog ==");
            println!("path: {}", path.display());
            println!(
                "updated_at: {}",
                catalog
                    .get("updated_at")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            );
            println!("entries: {entries}");
            if let Some(arr) = catalog.get("entries").and_then(Value::as_array) {
                for row in arr {
                    let backend = row
                        .get("backend")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let tier = row.get("tier").and_then(Value::as_str).unwrap_or("unknown");
                    let kind = row
                        .get("limit_type")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let total = row
                        .get("quota_total_tokens")
                        .and_then(Value::as_u64)
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "n/a".to_string());
                    println!("- {backend}:{tier} limit_type={kind} total_tokens={total}");
                }
            }
            0
        }
        "auto" => {
            let sub2 = args.get(1).map(String::as_str).unwrap_or("show");
            match sub2 {
                "show" => {
                    let cfg = catalog_auto_config_from_state();
                    println!("== cx quota catalog auto ==");
                    println!("enabled: {}", if cfg.enabled { "true" } else { "false" });
                    println!("interval_hours: {}", cfg.interval_hours);
                    0
                }
                "on" => {
                    let mut interval_hours = 168u64;
                    let mut i = 2usize;
                    while i < args.len() {
                        match args[i].as_str() {
                            "--interval-hours" => {
                                let Some(v) = args.get(i + 1) else {
                                    crate::cx_eprintln!(
                                        "quota catalog auto on: --interval-hours requires a value"
                                    );
                                    return 2;
                                };
                                match v.trim().parse::<u64>() {
                                    Ok(parsed) if parsed > 0 => interval_hours = parsed,
                                    _ => {
                                        crate::cx_eprintln!(
                                            "quota catalog auto on: --interval-hours must be >= 1"
                                        );
                                        return 2;
                                    }
                                }
                                i += 2;
                            }
                            "--json" => i += 1,
                            other => {
                                crate::cx_eprintln!("quota catalog auto on: unknown arg '{other}'");
                                return 2;
                            }
                        }
                    }
                    if let Err(e) =
                        set_state_path("preferences.quota_catalog.auto.enabled", Value::Bool(true))
                    {
                        crate::cx_eprintln!("quota catalog auto on: {e}");
                        return 1;
                    }
                    if let Err(e) = set_state_path(
                        "preferences.quota_catalog.auto.interval_hours",
                        json!(interval_hours),
                    ) {
                        crate::cx_eprintln!("quota catalog auto on: {e}");
                        return 1;
                    }
                    println!("quota_catalog_auto: enabled");
                    println!("interval_hours: {}", interval_hours);
                    0
                }
                "off" => {
                    if let Err(e) =
                        set_state_path("preferences.quota_catalog.auto.enabled", Value::Bool(false))
                    {
                        crate::cx_eprintln!("quota catalog auto off: {e}");
                        return 1;
                    }
                    println!("quota_catalog_auto: disabled");
                    0
                }
                _ => {
                    crate::cx_eprintln!(
                        "Usage: quota catalog auto <show|on [--interval-hours N]|off>"
                    );
                    2
                }
            }
        }
        _ => {
            crate::cx_eprintln!(
                "Usage: quota catalog <show|refresh [--if-stale --max-age-hours N] [--json]|auto <show|on|off>>"
            );
            2
        }
    }
}

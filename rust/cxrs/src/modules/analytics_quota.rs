use chrono::Utc;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::logs::load_values;
use crate::paths::{resolve_log_file, resolve_quota_catalog_file};
use crate::runtime::{llm_backend, llm_model};
use crate::state::{read_state_value, set_state_path, value_at_path, write_json_atomic};

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

#[derive(Debug, Clone)]
struct GuardConfig {
    enabled: bool,
    warn_pct: f64,
    critical_pct: f64,
    auto_action: String,
}

fn guard_config_from_state() -> GuardConfig {
    let mut cfg = GuardConfig {
        enabled: false,
        warn_pct: 0.25,
        critical_pct: 0.10,
        auto_action: "none".to_string(),
    };
    if let Some(state) = read_state_value() {
        if let Some(v) =
            value_at_path(&state, "preferences.quota_guard.enabled").and_then(Value::as_bool)
        {
            cfg.enabled = v;
        }
        if let Some(v) =
            value_at_path(&state, "preferences.quota_guard.warn_pct").and_then(Value::as_f64)
        {
            cfg.warn_pct = v.clamp(0.0, 1.0);
        }
        if let Some(v) =
            value_at_path(&state, "preferences.quota_guard.critical_pct").and_then(Value::as_f64)
        {
            cfg.critical_pct = v.clamp(0.0, 1.0);
        }
        if let Some(v) =
            value_at_path(&state, "preferences.quota_guard.auto_action").and_then(Value::as_str)
        {
            let action = v.trim().to_lowercase();
            if matches!(action.as_str(), "none" | "quota_saver") {
                cfg.auto_action = action;
            }
        }
    }
    if cfg.warn_pct < cfg.critical_pct {
        cfg.warn_pct = cfg.critical_pct;
    }
    cfg
}

fn parse_pct(input: &str) -> Result<f64, String> {
    let raw = input
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("invalid percentage '{}'", input))?;
    if (0.0..=100.0).contains(&raw) {
        Ok(raw / 100.0)
    } else {
        Err(format!(
            "percentage must be between 0 and 100, got {}",
            input
        ))
    }
}

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

fn quota_catalog_path() -> Option<PathBuf> {
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
            {
                "provider":"openai",
                "backend":"codex",
                "tier":"free",
                "service_kind":"remote_metered",
                "limit_type":"dynamic",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://help.openai.com/en/articles/11909943-gpt-53-and-52-in-chatgpt",
                "retrieved_at": now,
                "notes":"Public ChatGPT limits are product/tier-dependent and can change."
            },
            {
                "provider":"openai",
                "backend":"codex",
                "tier":"plus",
                "service_kind":"remote_metered",
                "limit_type":"dynamic",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://help.openai.com/en/articles/11909943-gpt-53-and-52-in-chatgpt",
                "retrieved_at": now,
                "notes":"Plus limits may vary by model and rolling window."
            },
            {
                "provider":"openai",
                "backend":"codex",
                "tier":"pro",
                "service_kind":"remote_metered",
                "limit_type":"dynamic",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://help.openai.com/en/articles/9793128/",
                "retrieved_at": now,
                "notes":"Pro may be described with dynamic or guardrail-based usage."
            },
            {
                "provider":"openai",
                "backend":"codex",
                "tier":"business",
                "service_kind":"remote_metered",
                "limit_type":"dynamic",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://help.openai.com/en/articles/12003714-chatgpt-business-models-limits",
                "retrieved_at": now,
                "notes":"Business limits are model-specific and may be updated."
            },
            {
                "provider":"anthropic",
                "backend":"anthropic",
                "tier":"free",
                "service_kind":"remote_metered",
                "limit_type":"dynamic",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://support.anthropic.com/en/articles/8324991-about-claude-s-pro-plan-usage",
                "retrieved_at": now,
                "notes":"Anthropic usage limits are dynamic."
            },
            {
                "provider":"anthropic",
                "backend":"anthropic",
                "tier":"pro",
                "service_kind":"remote_metered",
                "limit_type":"dynamic",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://support.anthropic.com/en/articles/8324991-about-claude-s-pro-plan-usage",
                "retrieved_at": now,
                "notes":"Anthropic Pro usage is dynamic."
            },
            {
                "provider":"google",
                "backend":"gemini",
                "tier":"free",
                "service_kind":"remote_metered",
                "limit_type":"dynamic",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://gemini.google/us/subscriptions/?hl=en",
                "retrieved_at": now,
                "notes":"Gemini subscription pages publish feature limits; quotas may change."
            },
            {
                "provider":"google",
                "backend":"gemini",
                "tier":"advanced",
                "service_kind":"remote_metered",
                "limit_type":"dynamic",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://gemini.google/us/subscriptions/?hl=en",
                "retrieved_at": now,
                "notes":"Gemini Advanced limits may vary over time."
            },
            {
                "provider":"perplexity",
                "backend":"perplexity",
                "tier":"free",
                "service_kind":"remote_metered",
                "limit_type":"rate_limited",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://docs.perplexity.ai/docs/admin/rate-limits-usage-tiers",
                "retrieved_at": now,
                "notes":"Perplexity documents request/rate tiers rather than token budgets."
            },
            {
                "provider":"ollama",
                "backend":"ollama",
                "tier":"local",
                "service_kind":"local_unmetered",
                "limit_type":"unmetered",
                "quota_total_tokens": null,
                "window_days": null,
                "source_url":"https://ollama.com",
                "retrieved_at": now,
                "notes":"Local inference; no provider-enforced quota."
            }
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

fn cmd_quota_catalog(args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("show");
    let as_json = args.iter().any(|a| a == "--json");
    match sub {
        "refresh" => {
            let Some(path) = quota_catalog_path() else {
                crate::cx_eprintln!("quota catalog: unable to resolve catalog path");
                return 1;
            };
            let catalog = embedded_quota_catalog();
            if let Err(e) = write_json_atomic(&path, &catalog) {
                crate::cx_eprintln!("quota catalog refresh: {e}");
                return 1;
            }
            if as_json {
                match serde_json::to_string_pretty(&catalog) {
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
                println!("entries: {entries}");
            }
            0
        }
        "show" => {
            let Some(path) = quota_catalog_path() else {
                crate::cx_eprintln!("quota catalog: unable to resolve catalog path");
                return 1;
            };
            let catalog = load_quota_catalog().unwrap_or_else(|| {
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
        _ => {
            crate::cx_eprintln!("Usage: quota catalog <show|refresh> [--json]");
            2
        }
    }
}

fn cmd_quota_guard(args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("show");
    match sub {
        "show" => {
            let cfg = guard_config_from_state();
            println!("== cx quota guard ==");
            println!("enabled: {}", if cfg.enabled { "true" } else { "false" });
            println!("warn_pct: {}%", (cfg.warn_pct * 100.0).round() as i64);
            println!(
                "critical_pct: {}%",
                (cfg.critical_pct * 100.0).round() as i64
            );
            println!("auto_action: {}", cfg.auto_action);
            0
        }
        "on" => {
            let mut cfg = guard_config_from_state();
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--warn-pct" => {
                        let Some(v) = args.get(i + 1) else {
                            crate::cx_eprintln!("quota guard on: --warn-pct requires a value");
                            return 2;
                        };
                        match parse_pct(v) {
                            Ok(p) => cfg.warn_pct = p,
                            Err(e) => {
                                crate::cx_eprintln!("quota guard on: {e}");
                                return 2;
                            }
                        }
                        i += 2;
                    }
                    "--critical-pct" => {
                        let Some(v) = args.get(i + 1) else {
                            crate::cx_eprintln!("quota guard on: --critical-pct requires a value");
                            return 2;
                        };
                        match parse_pct(v) {
                            Ok(p) => cfg.critical_pct = p,
                            Err(e) => {
                                crate::cx_eprintln!("quota guard on: {e}");
                                return 2;
                            }
                        }
                        i += 2;
                    }
                    "--auto-action" => {
                        let Some(v) = args.get(i + 1) else {
                            crate::cx_eprintln!("quota guard on: --auto-action requires a value");
                            return 2;
                        };
                        let action = v.trim().to_lowercase();
                        if !matches!(action.as_str(), "none" | "quota_saver") {
                            crate::cx_eprintln!(
                                "quota guard on: --auto-action expects none|quota_saver"
                            );
                            return 2;
                        }
                        cfg.auto_action = action;
                        i += 2;
                    }
                    other => {
                        crate::cx_eprintln!("quota guard on: unknown arg '{other}'");
                        return 2;
                    }
                }
            }
            if cfg.warn_pct < cfg.critical_pct {
                cfg.warn_pct = cfg.critical_pct;
            }
            cfg.enabled = true;
            if let Err(e) = set_state_path("preferences.quota_guard.enabled", Value::Bool(true)) {
                crate::cx_eprintln!("quota guard on: {e}");
                return 1;
            }
            if let Err(e) = set_state_path("preferences.quota_guard.warn_pct", json!(cfg.warn_pct))
            {
                crate::cx_eprintln!("quota guard on: {e}");
                return 1;
            }
            if let Err(e) = set_state_path(
                "preferences.quota_guard.critical_pct",
                json!(cfg.critical_pct),
            ) {
                crate::cx_eprintln!("quota guard on: {e}");
                return 1;
            }
            if let Err(e) = set_state_path(
                "preferences.quota_guard.auto_action",
                Value::String(cfg.auto_action.clone()),
            ) {
                crate::cx_eprintln!("quota guard on: {e}");
                return 1;
            }
            println!("quota_guard: enabled");
            println!("warn_pct: {}%", (cfg.warn_pct * 100.0).round() as i64);
            println!(
                "critical_pct: {}%",
                (cfg.critical_pct * 100.0).round() as i64
            );
            println!("auto_action: {}", cfg.auto_action);
            0
        }
        "off" => {
            if let Err(e) = set_state_path("preferences.quota_guard.enabled", Value::Bool(false)) {
                crate::cx_eprintln!("quota guard off: {e}");
                return 1;
            }
            println!("quota_guard: disabled");
            0
        }
        "check" => cmd_quota_guard_check(&args[1..]),
        _ => {
            crate::cx_eprintln!(
                "Usage: quota guard <show|on [--warn-pct N --critical-pct N --auto-action none|quota_saver]|off|check [days] [--json] [--apply] [--strict]>"
            );
            2
        }
    }
}

fn cmd_quota_set(args: &[String]) -> i32 {
    let backend = args.first().map(String::as_str).unwrap_or_default();
    let total_raw = args.get(1).map(String::as_str).unwrap_or_default();
    if backend.is_empty() || total_raw.is_empty() {
        crate::cx_eprintln!("Usage: quota set <backend|default> <total_tokens>");
        return 2;
    }
    let backend_norm = backend.trim().to_lowercase();
    if !matches!(backend_norm.as_str(), "codex" | "ollama" | "default") {
        crate::cx_eprintln!("quota set: backend must be codex|ollama|default");
        return 2;
    }
    let total = match total_raw.trim().parse::<u64>() {
        Ok(v) if v > 0 => v,
        _ => {
            crate::cx_eprintln!("quota set: total_tokens must be a positive integer");
            return 2;
        }
    };
    let key = if backend_norm == "default" {
        "preferences.quota.default_total_tokens".to_string()
    } else {
        format!("preferences.quota.{}_total_tokens", backend_norm)
    };
    if let Err(e) = set_state_path(&key, json!(total)) {
        crate::cx_eprintln!("quota set: {e}");
        return 1;
    }
    println!(
        "quota_total_set: backend={} total_tokens={}",
        backend_norm, total
    );
    0
}

fn cmd_quota_unset(args: &[String]) -> i32 {
    let backend = args.first().map(String::as_str).unwrap_or_default();
    if backend.is_empty() {
        crate::cx_eprintln!("Usage: quota unset <backend|default|all>");
        return 2;
    }
    let backend_norm = backend.trim().to_lowercase();
    let mut keys: Vec<String> = Vec::new();
    match backend_norm.as_str() {
        "codex" | "ollama" => keys.push(format!("preferences.quota.{}_total_tokens", backend_norm)),
        "default" => keys.push("preferences.quota.default_total_tokens".to_string()),
        "all" => {
            keys.push("preferences.quota.codex_total_tokens".to_string());
            keys.push("preferences.quota.ollama_total_tokens".to_string());
            keys.push("preferences.quota.default_total_tokens".to_string());
        }
        _ => {
            crate::cx_eprintln!("quota unset: backend must be codex|ollama|default|all");
            return 2;
        }
    }
    for key in keys {
        if let Err(e) = set_state_path(&key, Value::Null) {
            crate::cx_eprintln!("quota unset: {e}");
            return 1;
        }
    }
    println!("quota_total_unset: {backend_norm}");
    0
}

fn evaluate_quota_guard(
    probe: &Value,
    cfg: &GuardConfig,
    apply: bool,
    strict: bool,
) -> (Value, i32) {
    let remaining_pct = probe.get("quota_remaining_pct").and_then(Value::as_f64);
    let quota_source = probe
        .get("quota_source")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut level = "ok".to_string();
    let mut reason = "quota healthy".to_string();

    if !cfg.enabled {
        level = "disabled".to_string();
        reason = "quota guard disabled".to_string();
    } else if probe
        .get("service_kind")
        .and_then(Value::as_str)
        .unwrap_or("")
        == "local_unmetered"
    {
        level = "unmetered".to_string();
        reason = "local provider reports unmetered service".to_string();
    } else if remaining_pct.is_none() {
        level = "unknown".to_string();
        reason = format!("cannot compute remaining quota (source={quota_source})");
    } else if let Some(rem) = remaining_pct {
        if rem <= cfg.critical_pct {
            level = "critical".to_string();
            reason = format!(
                "remaining quota {}% is below/equal critical threshold {}%",
                (rem * 100.0).round() as i64,
                (cfg.critical_pct * 100.0).round() as i64
            );
        } else if rem <= cfg.warn_pct {
            level = "warning".to_string();
            reason = format!(
                "remaining quota {}% is below/equal warning threshold {}%",
                (rem * 100.0).round() as i64,
                (cfg.warn_pct * 100.0).round() as i64
            );
        }
    }

    let mut auto_applied = Value::Null;
    if apply
        && cfg.enabled
        && matches!(level.as_str(), "warning" | "critical")
        && cfg.auto_action == "quota_saver"
    {
        match set_state_path(
            "preferences.broker_policy",
            Value::String("quota_saver".to_string()),
        ) {
            Ok(()) => {
                auto_applied = json!("set preferences.broker_policy=quota_saver");
            }
            Err(e) => {
                auto_applied = json!(format!("failed: {e}"));
            }
        }
    }

    let options = json!([
        { "id":"continue", "description":"Continue with current settings." },
        { "id":"quota_saver", "description":"Set broker policy to quota_saver.", "command":"cx broker set --policy quota_saver" },
        { "id":"lean_mode", "description":"Run sensitive tasks with lean mode.", "command":"CX_MODE=lean cx <command>" },
        { "id":"tight_budgets", "description":"Lower context budgets for next run.", "command":"CX_CONTEXT_BUDGET_CHARS=8000 CX_CONTEXT_BUDGET_LINES=200 cx <command>" },
        { "id":"strict_gate", "description":"Fail fast when warning/critical remains.", "command":"cx quota guard check 30 --strict" }
    ]);

    let payload = json!({
        "guard": {
            "enabled": cfg.enabled,
            "warn_pct": cfg.warn_pct,
            "critical_pct": cfg.critical_pct,
            "auto_action": cfg.auto_action
        },
        "probe": probe,
        "status": level,
        "reason": reason,
        "options": options,
        "auto_applied": auto_applied
    });
    let code = if strict && matches!(level.as_str(), "warning" | "critical") {
        1
    } else {
        0
    };
    (payload, code)
}

fn cmd_quota_guard_check(args: &[String]) -> i32 {
    let mut days = 30usize;
    let mut as_json = false;
    let mut apply = false;
    let mut strict = false;
    for a in args {
        match a.as_str() {
            "--json" => as_json = true,
            "--apply" => apply = true,
            "--strict" => strict = true,
            _ => match a.parse::<usize>() {
                Ok(v) if v > 0 => days = v,
                _ => {
                    crate::cx_eprintln!(
                        "Usage: quota guard check [days] [--json] [--apply] [--strict]"
                    );
                    return 2;
                }
            },
        }
    }
    let (log_file, rows) = match read_window_rows(days) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("cxrs {e}");
            return 1;
        }
    };
    let probe = quota_probe_payload(days, &log_file, &rows);
    let cfg = guard_config_from_state();
    let (payload, code) = evaluate_quota_guard(&probe, &cfg, apply, strict);
    if as_json {
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("cxrs quota guard check: failed to render json: {e}");
                return 1;
            }
        }
        return code;
    }

    println!("== cx quota guard check (last {days} days) ==");
    println!(
        "status: {}",
        payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "reason: {}",
        payload.get("reason").and_then(Value::as_str).unwrap_or("")
    );
    if let Some(p) = payload.get("probe") {
        println!(
            "backend: {}",
            p.get("backend")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        println!(
            "quota_tier: {}",
            p.get("quota_tier")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        println!(
            "quota_limit_type: {}",
            p.get("quota_limit_type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        println!(
            "quota_source: {}",
            p.get("quota_source")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
        println!(
            "quota_total_tokens: {}",
            p.get("quota_total_tokens")
                .and_then(Value::as_u64)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        println!(
            "quota_used_tokens_window: {}",
            p.get("quota_used_tokens_window")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "quota_remaining_tokens: {}",
            p.get("quota_remaining_tokens")
                .and_then(Value::as_u64)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
    }
    println!("options:");
    if let Some(opts) = payload.get("options").and_then(Value::as_array) {
        for opt in opts {
            let id = opt.get("id").and_then(Value::as_str).unwrap_or("option");
            let desc = opt
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let cmd = opt.get("command").and_then(Value::as_str).unwrap_or("");
            if cmd.is_empty() {
                println!("- {}: {}", id, desc);
            } else {
                println!("- {}: {} ({})", id, desc, cmd);
            }
        }
    }
    if let Some(applied) = payload.get("auto_applied").and_then(Value::as_str) {
        println!("auto_applied: {applied}");
    }
    code
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

    if let Some(catalog) = load_quota_catalog()
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
                "Usage: quota [probe] [days] [--json] | quota catalog <show|refresh> [--json]"
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
    let top = top_commands(&rows);
    let recommendations = vec![
        "Set a provider quota total with CX_QUOTA_<BACKEND>_TOTAL_TOKENS or CX_QUOTA_TOTAL_TOKENS."
            .to_string(),
        "Use quota catalog for official-source references: cx quota catalog refresh && cx quota catalog show."
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

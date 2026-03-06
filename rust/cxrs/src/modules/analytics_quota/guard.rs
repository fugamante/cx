use serde_json::{Value, json};

use super::resolution::quota_probe_payload;
use super::shared::read_window_rows;
use crate::state::{read_state_value, set_state_path, value_at_path};

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
    let probe = quota_probe_payload(days, &log_file, &rows, None, None);
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

pub(super) fn cmd_quota_guard(args: &[String]) -> i32 {
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

pub(super) fn cmd_quota_set(args: &[String]) -> i32 {
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

pub(super) fn cmd_quota_unset(args: &[String]) -> i32 {
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

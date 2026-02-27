use std::process::Command;

use serde_json::{Value, json};

use crate::config::app_config;
use crate::runtime::{llm_backend, llm_model};
use crate::state::set_state_path;

fn valid_policy(s: &str) -> bool {
    matches!(s, "latency" | "quality" | "cost" | "balanced")
}

fn parse_set_policy(args: &[String]) -> Result<String, String> {
    let mut i = 0usize;
    let mut policy: Option<String> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--policy" => {
                let Some(v) = args.get(i + 1) else {
                    return Err("cxrs broker set: --policy requires a value".to_string());
                };
                policy = Some(v.trim().to_lowercase());
                i += 2;
            }
            other => {
                return Err(format!("cxrs broker set: unknown flag '{other}'"));
            }
        }
    }
    let Some(v) = policy else {
        return Err("cxrs broker set: missing --policy".to_string());
    };
    if !valid_policy(&v) {
        return Err(format!("cxrs broker set: invalid policy '{v}'"));
    }
    Ok(v)
}

fn backend_available(name: &str) -> bool {
    let disabled = match name {
        "codex" => std::env::var("CX_DISABLE_CODEX")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        "ollama" => std::env::var("CX_DISABLE_OLLAMA")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        _ => false,
    };
    if disabled {
        return false;
    }
    Command::new("bash")
        .args(["-lc", &format!("command -v {name} >/dev/null 2>&1")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn cmd_broker(app_name: &str, args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("show");
    match sub {
        "show" => {
            let active_backend = llm_backend();
            let active_model = llm_model();
            let policy = app_config().broker_policy.clone();
            let codex_ok = backend_available("codex");
            let ollama_ok = backend_available("ollama");

            if args.iter().any(|a| a == "--json") {
                let out = json!({
                    "broker_policy": policy,
                    "active_backend": active_backend,
                    "active_model": if active_model.is_empty() { Value::Null } else { json!(active_model) },
                    "availability": {
                        "codex": codex_ok,
                        "ollama": ollama_ok
                    }
                });
                match serde_json::to_string_pretty(&out) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        crate::cx_eprintln!("cxrs broker show: failed to render json: {e}");
                        return 1;
                    }
                }
                return 0;
            }

            println!("== cx broker ==");
            println!("policy: {policy}");
            println!("active_backend: {active_backend}");
            println!(
                "active_model: {}",
                if active_model.is_empty() {
                    "<unset>"
                } else {
                    &active_model
                }
            );
            println!(
                "availability.codex: {}",
                if codex_ok { "yes" } else { "no" }
            );
            println!(
                "availability.ollama: {}",
                if ollama_ok { "yes" } else { "no" }
            );
            0
        }
        "set" => {
            let policy = match parse_set_policy(&args[1..]) {
                Ok(v) => v,
                Err(e) => {
                    crate::cx_eprintln!(
                        "{e}\nUsage: {app_name} broker set --policy latency|quality|cost|balanced"
                    );
                    return 2;
                }
            };
            if let Err(e) =
                set_state_path("preferences.broker_policy", Value::String(policy.clone()))
            {
                crate::cx_eprintln!("cxrs broker set: {e}");
                return 1;
            }
            println!("broker_policy: {policy}");
            0
        }
        other => {
            crate::cx_eprintln!(
                "Usage: {app_name} broker <show [--json] | set --policy latency|quality|cost|balanced>"
            );
            crate::cx_eprintln!("cxrs broker: unknown subcommand '{other}'");
            2
        }
    }
}

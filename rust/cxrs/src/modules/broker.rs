use std::process::Command;

use serde_json::{Value, json};

use crate::config::app_config;
use crate::runtime::{llm_backend, llm_model};

fn backend_available(name: &str) -> bool {
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
            crate::cx_eprintln!(
                "Usage: {app_name} broker show [--json]\nPhase IV note: 'broker set' is planned but not implemented yet."
            );
            2
        }
        other => {
            crate::cx_eprintln!("Usage: {app_name} broker <show [--json]>");
            crate::cx_eprintln!("cxrs broker: unknown subcommand '{other}'");
            2
        }
    }
}

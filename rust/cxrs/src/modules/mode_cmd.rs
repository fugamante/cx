use serde_json::json;

use crate::json_mode::decide_json_mode;

fn parse_bool_word(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "json" | "true" | "1" => Some(true),
        "text" | "false" | "0" => Some(false),
        _ => None,
    }
}

pub fn cmd_mode(app_name: &str, args: &[String]) -> i32 {
    let mut json_out = false;
    let mut cli_override: Option<bool> = None;
    let mut command_default = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "show" | "explain" => i += 1,
            "--json" => {
                json_out = true;
                i += 1;
            }
            "--text" => {
                json_out = false;
                i += 1;
            }
            "--cli" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!(
                        "Usage: {app_name} mode [show|explain] [--json] [--cli json|text] [--command-default json|text]"
                    );
                    return 2;
                };
                let Some(parsed) = parse_bool_word(v) else {
                    crate::cx_eprintln!("{app_name} mode: invalid --cli '{v}' (use json|text)");
                    return 2;
                };
                cli_override = Some(parsed);
                i += 2;
            }
            "--command-default" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!(
                        "Usage: {app_name} mode [show|explain] [--json] [--cli json|text] [--command-default json|text]"
                    );
                    return 2;
                };
                let Some(parsed) = parse_bool_word(v) else {
                    crate::cx_eprintln!(
                        "{app_name} mode: invalid --command-default '{v}' (use json|text)"
                    );
                    return 2;
                };
                command_default = parsed;
                i += 2;
            }
            other => {
                crate::cx_eprintln!("{app_name} mode: unknown argument '{other}'");
                crate::cx_eprintln!(
                    "Usage: {app_name} mode [show|explain] [--json] [--cli json|text] [--command-default json|text]"
                );
                return 2;
            }
        }
    }

    let d = decide_json_mode(cli_override, command_default);
    if json_out {
        println!(
            "{}",
            json!({
                "selected": if d.json_out { "json" } else { "text" },
                "json_out": d.json_out,
                "source": d.source,
                "reason": d.reason,
                "confidence_pct": d.confidence_pct,
                "inputs": {
                    "cli_override": d.cli_override,
                    "env_default": d.env_default,
                    "state_default": d.state_default,
                    "command_default": d.command_default
                },
                "signals": {
                    "stdout_tty": d.signals.stdout_tty,
                    "stdin_tty": d.signals.stdin_tty,
                    "ci": d.signals.ci,
                    "auto_enabled": d.signals.auto_enabled
                }
            })
        );
        return 0;
    }

    println!("selected: {}", if d.json_out { "json" } else { "text" });
    println!("source: {}", d.source);
    println!("reason: {}", d.reason);
    println!("confidence_pct: {}", d.confidence_pct);
    println!(
        "inputs: cli_override={:?} env_default={:?} state_default={:?} command_default={}",
        d.cli_override, d.env_default, d.state_default, d.command_default
    );
    println!(
        "signals: stdout_tty={} stdin_tty={} ci={} auto_enabled={}",
        d.signals.stdout_tty, d.signals.stdin_tty, d.signals.ci, d.signals.auto_enabled
    );
    0
}

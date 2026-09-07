use std::io::IsTerminal;

use crate::state::{read_state_value, value_at_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonSignals {
    pub stdout_tty: bool,
    pub stdin_tty: bool,
    pub ci: bool,
    pub auto_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonDecision {
    pub json_out: bool,
    pub source: &'static str,
    pub reason: String,
    pub confidence_pct: u8,
    pub cli_override: Option<bool>,
    pub env_default: Option<bool>,
    pub state_default: Option<bool>,
    pub command_default: bool,
    pub signals: JsonSignals,
}

fn parse_bool_str(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_bool(name: &str) -> Option<bool> {
    std::env::var(name).ok().and_then(|v| parse_bool_str(&v))
}

fn state_bool(path: &str) -> Option<bool> {
    let state = read_state_value()?;
    let val = value_at_path(&state, path)?;
    if let Some(b) = val.as_bool() {
        return Some(b);
    }
    if let Some(s) = val.as_str() {
        return parse_bool_str(s);
    }
    if let Some(n) = val.as_i64() {
        return Some(n != 0);
    }
    None
}

fn ci_enabled() -> bool {
    if env_bool("CI") == Some(true) || env_bool("GITHUB_ACTIONS") == Some(true) {
        return true;
    }
    let gh = std::env::var("GITHUB_ACTIONS").unwrap_or_default();
    gh.eq_ignore_ascii_case("true")
}

pub fn env_default_json() -> Option<bool> {
    env_bool("CX_JSON_DEFAULT")
}

pub fn state_default_json() -> Option<bool> {
    state_bool("preferences.default_json_output")
}

pub fn json_auto_enabled() -> bool {
    env_bool("CX_JSON_AUTO").unwrap_or(false)
}

pub fn runtime_json_signals() -> JsonSignals {
    JsonSignals {
        stdout_tty: std::io::stdout().is_terminal(),
        stdin_tty: std::io::stdin().is_terminal(),
        ci: ci_enabled(),
        auto_enabled: json_auto_enabled(),
    }
}

fn auto_mode(sig: &JsonSignals) -> Option<(bool, String, u8)> {
    if !sig.auto_enabled {
        return None;
    }
    if sig.ci {
        return Some((true, "ci environment detected".to_string(), 95));
    }
    if !sig.stdout_tty {
        return Some((true, "stdout is not a tty".to_string(), 90));
    }
    if sig.stdout_tty && sig.stdin_tty {
        return Some((false, "interactive terminal detected".to_string(), 85));
    }
    if sig.stdout_tty && !sig.stdin_tty {
        return Some((false, "stdin piped, stdout interactive".to_string(), 65));
    }
    None
}

pub fn decide_json_mode(cli: Option<bool>, command_default: bool) -> JsonDecision {
    let signals = runtime_json_signals();
    let env_default = env_default_json();
    let state_default = state_default_json();

    if let Some(v) = cli {
        return JsonDecision {
            json_out: v,
            source: "cli",
            reason: "explicit command flag override".to_string(),
            confidence_pct: 100,
            cli_override: cli,
            env_default,
            state_default,
            command_default,
            signals,
        };
    }
    if let Some(v) = env_default {
        return JsonDecision {
            json_out: v,
            source: "env",
            reason: "CX_JSON_DEFAULT".to_string(),
            confidence_pct: 95,
            cli_override: cli,
            env_default,
            state_default,
            command_default,
            signals,
        };
    }
    if let Some(v) = state_default {
        return JsonDecision {
            json_out: v,
            source: "state",
            reason: "preferences.default_json_output".to_string(),
            confidence_pct: 90,
            cli_override: cli,
            env_default,
            state_default,
            command_default,
            signals,
        };
    }
    if let Some((json_out, reason, confidence_pct)) = auto_mode(&signals) {
        return JsonDecision {
            json_out,
            source: "auto",
            reason,
            confidence_pct,
            cli_override: cli,
            env_default,
            state_default,
            command_default,
            signals,
        };
    }
    JsonDecision {
        json_out: command_default,
        source: "default",
        reason: "command default".to_string(),
        confidence_pct: 70,
        cli_override: cli,
        env_default,
        state_default,
        command_default,
        signals,
    }
}

pub fn resolve_json_mode(cli: Option<bool>, command_default: bool) -> bool {
    decide_json_mode(cli, command_default).json_out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(sig: JsonSignals) -> Option<(bool, String, u8)> {
        auto_mode(&sig)
    }

    #[test]
    fn ci_json() {
        let sig = JsonSignals {
            stdout_tty: true,
            stdin_tty: true,
            ci: true,
            auto_enabled: true,
        };
        assert_eq!(auto(sig).map(|v| v.0), Some(true));
    }

    #[test]
    fn tty_text() {
        let sig = JsonSignals {
            stdout_tty: true,
            stdin_tty: true,
            ci: false,
            auto_enabled: true,
        };
        assert_eq!(auto(sig).map(|v| v.0), Some(false));
    }

    #[test]
    fn pipe_json() {
        let sig = JsonSignals {
            stdout_tty: false,
            stdin_tty: true,
            ci: false,
            auto_enabled: true,
        };
        assert_eq!(auto(sig).map(|v| v.0), Some(true));
    }

    #[test]
    fn auto_off() {
        let sig = JsonSignals {
            stdout_tty: false,
            stdin_tty: false,
            ci: true,
            auto_enabled: false,
        };
        assert!(auto(sig).is_none());
    }
}

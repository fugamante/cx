use serde_json::Value;
use std::io::{self, IsTerminal, Write};
use std::process::Command;

use crate::config::app_config;
use crate::process::run_command_output_with_timeout;
use crate::state::{read_state_value, set_state_path, value_at_path};

pub fn llm_backend() -> String {
    app_config().llm_backend.clone()
}

pub fn llm_model() -> String {
    if llm_backend() != "ollama" {
        return app_config().codex_model.clone();
    }
    app_config().ollama_model.clone()
}

pub fn logging_enabled() -> bool {
    app_config().cxlog_enabled
}

pub fn ollama_model_preference() -> String {
    if !app_config().ollama_model.is_empty() {
        return app_config().ollama_model.clone();
    }
    read_state_value()
        .and_then(|v| {
            value_at_path(&v, "preferences.ollama_model")
                .and_then(Value::as_str)
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

fn is_interactive_tty() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

fn ollama_list_models() -> Vec<String> {
    let mut cmd = Command::new("ollama");
    cmd.arg("list");
    let output = match run_command_output_with_timeout(cmd, "ollama list") {
        Ok(v) if v.status.success() => v,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut out: Vec<String> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 && line.to_lowercase().contains("name") {
            continue;
        }
        let name = line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn resolve_ollama_model_for_run() -> Result<String, String> {
    let model = llm_model();
    if !model.trim().is_empty() {
        return Ok(model);
    }
    if !is_interactive_tty() {
        return Err(
            "ollama model is unset; set CX_OLLAMA_MODEL or run 'cxrs llm set-model <model>'"
                .to_string(),
        );
    }

    let models = ollama_list_models();
    eprintln!("cxrs: no default Ollama model configured.");
    if models.is_empty() {
        eprintln!("No local models found from 'ollama list'.");
        eprintln!("Pull one first (example: ollama pull llama3.1) then set it.");
        return Err("ollama model selection aborted".to_string());
    }
    eprintln!("Select a default model (persisted to .codex/state.json):");
    for (idx, m) in models.iter().enumerate() {
        eprintln!("  {}. {}", idx + 1, m);
    }
    eprint!("Enter number or model name: ");
    let _ = io::stderr().flush();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("failed reading selection: {e}"))?;
    let selected_raw = input.trim();
    if selected_raw.is_empty() {
        return Err("no model selected".to_string());
    }
    let selected = if let Ok(n) = selected_raw.parse::<usize>() {
        models
            .get(n.saturating_sub(1))
            .cloned()
            .ok_or_else(|| "invalid model index".to_string())?
    } else {
        selected_raw.to_string()
    };
    set_state_path("preferences.ollama_model", Value::String(selected.clone()))?;
    eprintln!("cxrs: default Ollama model set to '{}'.", selected);
    Ok(selected)
}

pub fn llm_bin_name() -> &'static str {
    if llm_backend() == "ollama" {
        "ollama"
    } else {
        "codex"
    }
}

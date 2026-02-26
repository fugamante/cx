use serde_json::Value;

use crate::runtime::{llm_backend, llm_model, ollama_model_preference};
use crate::state::{
    ensure_state_value, parse_cli_value, set_state_path, set_value_at_path, state_cache_clear,
    value_at_path, write_json_atomic,
};

pub fn cmd_state_show() -> i32 {
    let (_state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs state show: {e}");
            return 1;
        }
    };
    match serde_json::to_string_pretty(&state) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            eprintln!("cxrs state show: failed to render JSON: {e}");
            1
        }
    }
}

pub fn cmd_state_get(key: &str) -> i32 {
    let (state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs state get: {e}");
            return 1;
        }
    };
    let Some(v) = value_at_path(&state, key) else {
        eprintln!("cxrs state get: key not found: {key}");
        eprintln!("state_file: {}", state_file.display());
        return 1;
    };
    match v {
        Value::String(s) => println!("{s}"),
        _ => println!("{}", v),
    }
    0
}

pub fn cmd_state_set(key: &str, raw_value: &str) -> i32 {
    let (state_file, mut state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs state set: {e}");
            return 1;
        }
    };
    if let Err(e) = set_value_at_path(&mut state, key, parse_cli_value(raw_value)) {
        eprintln!("cxrs state set: {e}");
        return 1;
    }
    if let Err(e) = write_json_atomic(&state_file, &state) {
        eprintln!("cxrs state set: {e}");
        return 1;
    }
    state_cache_clear();
    println!("ok");
    0
}

pub fn cmd_llm(app_name: &str, args: &[String]) -> i32 {
    let print_usage = || {
        eprintln!(
            "Usage: {app_name} llm <show|use <codex|ollama> [model]|unset <backend|model|all>|set-backend <codex|ollama>|set-model <model>|clear-model>"
        );
    };
    let sub = args.first().map(String::as_str).unwrap_or("show");
    match sub {
        "show" => {
            let backend = llm_backend();
            let model = llm_model();
            let ollama_pref = ollama_model_preference();
            println!("llm_backend: {backend}");
            println!(
                "active_model: {}",
                if model.is_empty() { "<unset>" } else { &model }
            );
            println!(
                "ollama_model: {}",
                if ollama_pref.is_empty() {
                    "<unset>"
                } else {
                    &ollama_pref
                }
            );
            0
        }
        "use" => {
            let Some(target) = args.get(1).map(|s| s.to_lowercase()) else {
                print_usage();
                return 2;
            };
            if target != "codex" && target != "ollama" {
                print_usage();
                return 2;
            }
            if let Err(e) = set_state_path("preferences.llm_backend", Value::String(target.clone()))
            {
                eprintln!("cxrs llm use: {e}");
                return 1;
            }
            if target == "ollama" {
                if let Some(model) = args.get(2) {
                    let m = model.trim();
                    if m.is_empty() {
                        print_usage();
                        return 2;
                    }
                    if let Err(e) =
                        set_state_path("preferences.ollama_model", Value::String(m.to_string()))
                    {
                        eprintln!("cxrs llm use: {e}");
                        return 1;
                    }
                }
                println!("ok");
                println!("llm_backend: ollama");
                let pref = ollama_model_preference();
                println!(
                    "ollama_model: {}",
                    if pref.is_empty() { "<unset>" } else { &pref }
                );
                return 0;
            }
            println!("ok");
            println!("llm_backend: codex");
            0
        }
        "unset" => {
            let target = args.get(1).map(String::as_str).unwrap_or("all");
            match target {
                "backend" => {
                    if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                        eprintln!("cxrs llm unset backend: {e}");
                        return 1;
                    }
                    println!("ok");
                    println!("llm_backend: <unset>");
                    0
                }
                "model" => {
                    if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                        eprintln!("cxrs llm unset model: {e}");
                        return 1;
                    }
                    println!("ok");
                    println!("ollama_model: <unset>");
                    0
                }
                "all" => {
                    if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                        eprintln!("cxrs llm unset all: {e}");
                        return 1;
                    }
                    if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                        eprintln!("cxrs llm unset all: {e}");
                        return 1;
                    }
                    println!("ok");
                    println!("llm_backend: <unset>");
                    println!("ollama_model: <unset>");
                    0
                }
                _ => {
                    print_usage();
                    2
                }
            }
        }
        "set-backend" => {
            let Some(v) = args.get(1).map(|s| s.to_lowercase()) else {
                print_usage();
                return 2;
            };
            if v != "codex" && v != "ollama" {
                print_usage();
                return 2;
            }
            if let Err(e) = set_state_path("preferences.llm_backend", Value::String(v.clone())) {
                eprintln!("cxrs llm set-backend: {e}");
                return 1;
            }
            println!("ok");
            println!("llm_backend: {v}");
            0
        }
        "set-model" => {
            let Some(model) = args.get(1) else {
                print_usage();
                return 2;
            };
            if model.trim().is_empty() {
                print_usage();
                return 2;
            }
            if let Err(e) = set_state_path(
                "preferences.ollama_model",
                Value::String(model.trim().to_string()),
            ) {
                eprintln!("cxrs llm set-model: {e}");
                return 1;
            }
            println!("ok");
            println!("ollama_model: {}", model.trim());
            0
        }
        "clear-model" => {
            if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                eprintln!("cxrs llm clear-model: {e}");
                return 1;
            }
            println!("ok");
            println!("ollama_model: <unset>");
            0
        }
        other => {
            eprintln!("{app_name} llm: unknown subcommand '{other}'");
            print_usage();
            2
        }
    }
}

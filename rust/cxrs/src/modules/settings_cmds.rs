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
            crate::cx_eprintln!("cxrs state show: {e}");
            return 1;
        }
    };
    match serde_json::to_string_pretty(&state) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!("cxrs state show: failed to render JSON: {e}");
            1
        }
    }
}

pub fn cmd_state_get(key: &str) -> i32 {
    let (state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("cxrs state get: {e}");
            return 1;
        }
    };
    let Some(v) = value_at_path(&state, key) else {
        crate::cx_eprintln!("cxrs state get: key not found: {key}");
        crate::cx_eprintln!("state_file: {}", state_file.display());
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
            crate::cx_eprintln!("cxrs state set: {e}");
            return 1;
        }
    };
    if let Err(e) = set_value_at_path(&mut state, key, parse_cli_value(raw_value)) {
        crate::cx_eprintln!("cxrs state set: {e}");
        return 1;
    }
    if let Err(e) = write_json_atomic(&state_file, &state) {
        crate::cx_eprintln!("cxrs state set: {e}");
        return 1;
    }
    state_cache_clear();
    println!("ok");
    0
}

fn print_llm_usage(app_name: &str) {
    crate::cx_eprintln!(
        "Usage: {app_name} llm <show|use <codex|ollama> [model]|unset <backend|model|all>|set-backend <codex|ollama>|set-model <model>|clear-model>"
    );
}

fn llm_show() -> i32 {
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

fn llm_use(app_name: &str, args: &[String]) -> i32 {
    let Some(target) = args.get(1).map(|s| s.to_lowercase()) else {
        print_llm_usage(app_name);
        return 2;
    };
    if target != "codex" && target != "ollama" {
        print_llm_usage(app_name);
        return 2;
    }
    if let Err(e) = set_state_path("preferences.llm_backend", Value::String(target.clone())) {
        crate::cx_eprintln!("cxrs llm use: {e}");
        return 1;
    }
    if target == "ollama" {
        if let Some(model) = args.get(2) {
            let m = model.trim();
            if m.is_empty() {
                print_llm_usage(app_name);
                return 2;
            }
            if let Err(e) = set_state_path("preferences.ollama_model", Value::String(m.to_string()))
            {
                crate::cx_eprintln!("cxrs llm use: {e}");
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

fn llm_unset(app_name: &str, args: &[String]) -> i32 {
    let target = args.get(1).map(String::as_str).unwrap_or("all");
    match target {
        "backend" => {
            if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                crate::cx_eprintln!("cxrs llm unset backend: {e}");
                return 1;
            }
            println!("ok");
            println!("llm_backend: <unset>");
            0
        }
        "model" => {
            if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                crate::cx_eprintln!("cxrs llm unset model: {e}");
                return 1;
            }
            println!("ok");
            println!("ollama_model: <unset>");
            0
        }
        "all" => {
            if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                crate::cx_eprintln!("cxrs llm unset all: {e}");
                return 1;
            }
            if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                crate::cx_eprintln!("cxrs llm unset all: {e}");
                return 1;
            }
            println!("ok");
            println!("llm_backend: <unset>");
            println!("ollama_model: <unset>");
            0
        }
        _ => {
            print_llm_usage(app_name);
            2
        }
    }
}

fn llm_set_backend(app_name: &str, args: &[String]) -> i32 {
    let Some(v) = args.get(1).map(|s| s.to_lowercase()) else {
        print_llm_usage(app_name);
        return 2;
    };
    if v != "codex" && v != "ollama" {
        print_llm_usage(app_name);
        return 2;
    }
    if let Err(e) = set_state_path("preferences.llm_backend", Value::String(v.clone())) {
        crate::cx_eprintln!("cxrs llm set-backend: {e}");
        return 1;
    }
    println!("ok");
    println!("llm_backend: {v}");
    0
}

fn llm_set_model(app_name: &str, args: &[String]) -> i32 {
    let Some(model) = args.get(1) else {
        print_llm_usage(app_name);
        return 2;
    };
    if model.trim().is_empty() {
        print_llm_usage(app_name);
        return 2;
    }
    if let Err(e) = set_state_path(
        "preferences.ollama_model",
        Value::String(model.trim().to_string()),
    ) {
        crate::cx_eprintln!("cxrs llm set-model: {e}");
        return 1;
    }
    println!("ok");
    println!("ollama_model: {}", model.trim());
    0
}

fn llm_clear_model() -> i32 {
    if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
        crate::cx_eprintln!("cxrs llm clear-model: {e}");
        return 1;
    }
    println!("ok");
    println!("ollama_model: <unset>");
    0
}

pub fn cmd_llm(app_name: &str, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("show") {
        "show" => llm_show(),
        "use" => llm_use(app_name, args),
        "unset" => llm_unset(app_name, args),
        "set-backend" => llm_set_backend(app_name, args),
        "set-model" => llm_set_model(app_name, args),
        "clear-model" => llm_clear_model(),
        other => {
            crate::cx_eprintln!("{app_name} llm: unknown subcommand '{other}'");
            print_llm_usage(app_name);
            2
        }
    }
}

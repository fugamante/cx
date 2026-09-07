use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use crate::config::cli_app_name;

use crate::analytics::quota_probe_for_backend_days;
use crate::contract_versions::{
    LLM_RESIDENT_JSON_CONTRACT_VERSION, LLM_VERIFY_JSON_CONTRACT_VERSION,
};
use crate::execmeta::utc_now_iso;
use crate::llm::run_mlx_plain;
use crate::local_models::{
    find_record_for_backend, resolve_model_for_backend, selector_preferred_args, touch_model_record,
};
use crate::paths::repo_root_hint;
use crate::process::run_command_output_with_timeout;
use crate::provider_adapter::{
    http_profile_opt, probe_http_models_v1, resolve_provider_adapter, selected_adapter_name,
    selected_provider_transport, selected_resident_json, selected_runtime_caps,
};
use crate::runtime::{
    llama_cpp_model_preference, llm_backend, llm_model, mlx_model_preference,
    ollama_model_preference,
};
use crate::state::{
    ensure_state_value, parse_cli_value, set_state_path, set_value_at_path, state_cache_clear,
    value_at_path, write_json_atomic,
};

#[path = "settings_models.rs"]
mod llm_models;

pub fn cmd_state_show() -> i32 {
    let (_state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} state show: {e}", cli_app_name());
            return 1;
        }
    };
    match serde_json::to_string_pretty(&state) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!("{} state show: failed to render JSON: {e}", cli_app_name());
            1
        }
    }
}

pub fn cmd_state_get(key: &str) -> i32 {
    let (state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} state get: {e}", cli_app_name());
            return 1;
        }
    };
    let Some(v) = value_at_path(&state, key) else {
        crate::cx_eprintln!("{} state get: key not found: {key}", cli_app_name());
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
            crate::cx_eprintln!("{} state set: {e}", cli_app_name());
            return 1;
        }
    };
    if let Err(e) = set_value_at_path(&mut state, key, parse_cli_value(raw_value)) {
        crate::cx_eprintln!("{} state set: {e}", cli_app_name());
        return 1;
    }
    if let Err(e) = write_json_atomic(&state_file, &state) {
        crate::cx_eprintln!("{} state set: {e}", cli_app_name());
        return 1;
    }
    state_cache_clear();
    println!("ok");
    0
}

fn print_llm_usage(app_name: &str) {
    crate::cx_eprintln!(
        "Usage: {app_name} llm <show|check [backend]|smoke [prompt]|verify [mlx] [--profile smoke|benchmark] [--ctx N] [--prompt <text>] [--json]|resident [show|probe-models] [--json]|models <list|add|inspect|remove>|use <primary|ollama|llamacpp|mlx> [model]|unset <backend|model|all>|set-backend <primary|ollama|llamacpp|mlx>|set-model <model>|clear-model>"
    );
}

fn normalize_llm_backend(raw: &str) -> Option<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "primary" => Some("primary".to_string()),
        "ollama" => Some("ollama".to_string()),
        "llamacpp" | "llama.cpp" | "llama_cpp" => Some("llamacpp".to_string()),
        "mlx" => Some("mlx".to_string()),
        _ => None,
    }
}

fn emit_model_resolution_line(backend: &str, model: &str, label: &str) {
    if model.trim().is_empty() {
        return;
    }
    if let Ok(Some(record)) = find_record_for_backend(model, backend) {
        println!("{label}_alias: {}", record.alias);
        println!("{label}_resolved_model: {}", record.resolved_model);
    }
}

fn resolve_model_for_use(backend: &str, model: &str) -> Result<String, String> {
    resolve_model_for_backend(backend, model).map(|r| r.resolved_model)
}

fn llm_show() -> i32 {
    let backend = llm_backend();
    let model = llm_model();
    let ollama_pref = ollama_model_preference();
    let llama_cpp_pref = llama_cpp_model_preference();
    let mlx_pref = mlx_model_preference();
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
    println!(
        "llama_cpp_model: {}",
        if llama_cpp_pref.is_empty() {
            "<unset>"
        } else {
            &llama_cpp_pref
        }
    );
    println!(
        "mlx_model: {}",
        if mlx_pref.is_empty() {
            "<unset>"
        } else {
            &mlx_pref
        }
    );
    emit_model_resolution_line("ollama", &ollama_pref, "ollama_model");
    emit_model_resolution_line("llamacpp", &llama_cpp_pref, "llama_cpp_model");
    emit_model_resolution_line("mlx", &mlx_pref, "mlx_model");
    emit_model_resolution_line(&backend, &model, "active_model");
    0
}

fn emit_quota_probe_notice(backend: &str, model: Option<&str>) {
    let Ok(payload) = quota_probe_for_backend_days(30, backend, model) else {
        crate::cx_eprintln!("quota_probe: unavailable");
        return;
    };
    let backend = payload
        .get("backend")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let service_kind = payload
        .get("service_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let source = payload
        .get("quota_source")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let remaining = payload.get("quota_remaining_pct").and_then(Value::as_f64);

    if service_kind == "local_unmetered" {
        crate::cx_eprintln!(
            "quota_probe: backend={} service_kind=local_unmetered (provider quota unavailable for local model)",
            backend
        );
        return;
    }

    if let Some(rem) = remaining {
        let remaining_pct = format!("{}%", (rem * 100.0).round() as i64);
        crate::cx_eprintln!(
            "quota_probe: backend={} remaining={} source={}",
            backend,
            remaining_pct,
            source
        );
    } else {
        crate::cx_eprintln!(
            "quota_probe: backend={} remaining=unknown source={} (set quota total or refresh catalog)",
            backend,
            source
        );
    }
}

fn llm_use(app_name: &str, args: &[String]) -> i32 {
    let Some(target) = args.get(1).and_then(|s| normalize_llm_backend(s)) else {
        print_llm_usage(app_name);
        return 2;
    };
    if let Err(e) = set_state_path("preferences.llm_backend", Value::String(target.clone())) {
        crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
        return 1;
    }
    if target == "ollama" {
        if let Some(model) = args.get(2) {
            let m = model.trim();
            if m.is_empty() {
                print_llm_usage(app_name);
                return 2;
            }
            let resolved = match resolve_model_for_use("ollama", m) {
                Ok(v) => v,
                Err(e) => {
                    crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
                    return 1;
                }
            };
            if let Err(e) =
                set_state_path("preferences.ollama_model", Value::String(resolved.clone()))
            {
                crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
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
        state_cache_clear();
        let model_opt = if pref.is_empty() {
            None
        } else {
            Some(pref.as_str())
        };
        emit_quota_probe_notice("ollama", model_opt);
        return 0;
    }
    if target == "llamacpp" {
        if let Some(model) = args.get(2) {
            let m = model.trim();
            if m.is_empty() {
                print_llm_usage(app_name);
                return 2;
            }
            let resolved = match resolve_model_for_use("llamacpp", m) {
                Ok(v) => v,
                Err(e) => {
                    crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
                    return 1;
                }
            };
            if let Err(e) = set_state_path(
                "preferences.llama_cpp_model",
                Value::String(resolved.clone()),
            ) {
                crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
                return 1;
            }
        }
        println!("ok");
        println!("llm_backend: llamacpp");
        let pref = llama_cpp_model_preference();
        println!(
            "llama_cpp_model: {}",
            if pref.is_empty() { "<unset>" } else { &pref }
        );
        state_cache_clear();
        let model_opt = if pref.is_empty() {
            None
        } else {
            Some(pref.as_str())
        };
        emit_quota_probe_notice("llamacpp", model_opt);
        return 0;
    }
    if target == "mlx" {
        if let Some(model) = args.get(2) {
            let m = model.trim();
            if m.is_empty() {
                print_llm_usage(app_name);
                return 2;
            }
            let resolved = match resolve_model_for_use("mlx", m) {
                Ok(v) => v,
                Err(e) => {
                    crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
                    return 1;
                }
            };
            if let Err(e) = set_state_path("preferences.mlx_model", Value::String(resolved)) {
                crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
                return 1;
            }
        }
        println!("ok");
        println!("llm_backend: mlx");
        let pref = mlx_model_preference();
        println!(
            "mlx_model: {}",
            if pref.is_empty() { "<unset>" } else { &pref }
        );
        state_cache_clear();
        let model_opt = if pref.is_empty() {
            None
        } else {
            Some(pref.as_str())
        };
        emit_quota_probe_notice("mlx", model_opt);
        return 0;
    }
    println!("ok");
    println!("llm_backend: primary");
    state_cache_clear();
    emit_quota_probe_notice("primary", None);
    0
}

fn llm_unset(app_name: &str, args: &[String]) -> i32 {
    let target = args.get(1).map(String::as_str).unwrap_or("all");
    match target {
        "backend" => {
            if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                crate::cx_eprintln!("{} llm unset backend: {e}", cli_app_name());
                return 1;
            }
            println!("ok");
            println!("llm_backend: <unset>");
            0
        }
        "model" => {
            let backend = llm_backend();
            let path = match backend.as_str() {
                "llamacpp" => "preferences.llama_cpp_model",
                "mlx" => "preferences.mlx_model",
                _ => "preferences.ollama_model",
            };
            if let Err(e) = set_state_path(path, Value::Null) {
                crate::cx_eprintln!("{} llm unset model: {e}", cli_app_name());
                return 1;
            }
            println!("ok");
            match backend.as_str() {
                "llamacpp" => println!("llama_cpp_model: <unset>"),
                "mlx" => println!("mlx_model: <unset>"),
                _ => println!("ollama_model: <unset>"),
            }
            0
        }
        "all" => {
            if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                crate::cx_eprintln!("{} llm unset all: {e}", cli_app_name());
                return 1;
            }
            if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                crate::cx_eprintln!("{} llm unset all: {e}", cli_app_name());
                return 1;
            }
            if let Err(e) = set_state_path("preferences.llama_cpp_model", Value::Null) {
                crate::cx_eprintln!("{} llm unset all: {e}", cli_app_name());
                return 1;
            }
            if let Err(e) = set_state_path("preferences.mlx_model", Value::Null) {
                crate::cx_eprintln!("{} llm unset all: {e}", cli_app_name());
                return 1;
            }
            println!("ok");
            println!("llm_backend: <unset>");
            println!("ollama_model: <unset>");
            println!("llama_cpp_model: <unset>");
            println!("mlx_model: <unset>");
            0
        }
        _ => {
            print_llm_usage(app_name);
            2
        }
    }
}

fn llm_set_backend(app_name: &str, args: &[String]) -> i32 {
    let Some(v) = args.get(1).and_then(|s| normalize_llm_backend(s)) else {
        print_llm_usage(app_name);
        return 2;
    };
    if let Err(e) = set_state_path("preferences.llm_backend", Value::String(v.clone())) {
        crate::cx_eprintln!("{} llm set-backend: {e}", cli_app_name());
        return 1;
    }
    println!("ok");
    println!("llm_backend: {v}");
    state_cache_clear();
    emit_quota_probe_notice(&v, None);
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
    let backend = llm_backend();
    let path = match backend.as_str() {
        "llamacpp" => "preferences.llama_cpp_model",
        "mlx" => "preferences.mlx_model",
        _ => "preferences.ollama_model",
    };
    if let Err(e) = set_state_path(path, Value::String(model.trim().to_string())) {
        crate::cx_eprintln!("{} llm set-model: {e}", cli_app_name());
        return 1;
    }
    println!("ok");
    match backend.as_str() {
        "llamacpp" => println!("llama_cpp_model: {}", model.trim()),
        "mlx" => println!("mlx_model: {}", model.trim()),
        _ => println!("ollama_model: {}", model.trim()),
    }
    state_cache_clear();
    emit_quota_probe_notice(&backend, Some(model.trim()));
    0
}

fn llm_clear_model() -> i32 {
    let backend = llm_backend();
    let path = match backend.as_str() {
        "llamacpp" => "preferences.llama_cpp_model",
        "mlx" => "preferences.mlx_model",
        _ => "preferences.ollama_model",
    };
    if let Err(e) = set_state_path(path, Value::Null) {
        crate::cx_eprintln!("{} llm clear-model: {e}", cli_app_name());
        return 1;
    }
    println!("ok");
    match backend.as_str() {
        "llamacpp" => println!("llama_cpp_model: <unset>"),
        "mlx" => println!("mlx_model: <unset>"),
        _ => println!("ollama_model: <unset>"),
    }
    0
}

fn command_available(bin: &str) -> bool {
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", "command -v \"$1\" >/dev/null 2>&1", "_", bin]);
    run_command_output_with_timeout(cmd, "llm check command")
        .ok()
        .is_some_and(|out| out.status.success())
}

fn mlx_available(python: &str) -> bool {
    let mut cmd = Command::new(python);
    cmd.args(["-c", "import mlx_lm"]);
    run_command_output_with_timeout(cmd, "llm check mlx")
        .ok()
        .is_some_and(|out| out.status.success())
}

fn check_model_for_backend(backend: &str) -> String {
    match backend {
        "ollama" => ollama_model_preference(),
        "llamacpp" => llama_cpp_model_preference(),
        "mlx" => mlx_model_preference(),
        _ => llm_model(),
    }
}

fn llm_check(app_name: &str, args: &[String]) -> i32 {
    let backend = if let Some(raw) = args.get(1) {
        let Some(v) = normalize_llm_backend(raw) else {
            print_llm_usage(app_name);
            return 2;
        };
        v
    } else {
        llm_backend()
    };
    let model = check_model_for_backend(&backend);
    let (runtime_ok, runtime, hint) = match backend.as_str() {
        "ollama" => (
            command_available("ollama"),
            "ollama".to_string(),
            "install Ollama and ensure 'ollama' is on PATH",
        ),
        "llamacpp" => {
            let bin = std::env::var("CX_LLAMA_CPP_BIN")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "llama-cli".to_string());
            (
                command_available(&bin),
                bin,
                "install llama.cpp and ensure llama-cli is on PATH or set CX_LLAMA_CPP_BIN",
            )
        }
        "mlx" => {
            let python = std::env::var("CX_MLX_PYTHON")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "python3".to_string());
            (
                mlx_available(&python),
                python,
                "install mlx-lm in the selected Python environment or set CX_MLX_PYTHON",
            )
        }
        _ => (
            command_available(concat!("co", "dex")),
            "primary".to_string(),
            "install the primary process backend and ensure its runtime is on PATH",
        ),
    };
    let model_required = matches!(backend.as_str(), "ollama" | "llamacpp" | "mlx");
    let model_ok = !model_required || !model.trim().is_empty();
    println!("backend: {backend}");
    println!("runtime: {runtime}");
    println!("runtime_ok: {}", if runtime_ok { "yes" } else { "no" });
    println!(
        "model: {}",
        if model.is_empty() { "<unset>" } else { &model }
    );
    println!("model_ok: {}", if model_ok { "yes" } else { "no" });
    if !runtime_ok {
        println!("runtime_hint: {hint}");
    }
    if !model_ok {
        println!("model_hint: {app_name} llm use {backend} <model>");
    }
    if runtime_ok && model_ok { 0 } else { 1 }
}

fn llm_smoke(args: &[String]) -> i32 {
    let prompt = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        "Respond with OK only.".to_string()
    };
    let backend = llm_backend();
    let model = llm_model();
    let adapter = match resolve_provider_adapter() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("llm smoke: {e}");
            return 1;
        }
    };
    let text = match adapter.run_plain(&prompt) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("llm smoke: {e}");
            return 1;
        }
    };
    println!("smoke_backend: {backend}");
    println!(
        "smoke_model: {}",
        if model.is_empty() { "<unset>" } else { &model }
    );
    println!("smoke_output:");
    println!("{}", text.trim());
    if matches!(backend.as_str(), "ollama" | "llamacpp" | "mlx") && !model.trim().is_empty() {
        let _ = note_model_usage(&backend, &model, None);
    }
    0
}

fn note_model_usage(backend: &str, model: &str, smoke_status: Option<&str>) -> Option<()> {
    match touch_model_record(backend, model, Some(&utc_now_iso()), smoke_status) {
        Ok(_) => Some(()),
        Err(e) => {
            crate::cx_eprintln!(
                "{} llm: unable to update local model registry metadata: {e}",
                cli_app_name()
            );
            None
        }
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn option_value(args: &[String], flag: &str) -> Result<Option<String>, String> {
    let mut out = None;
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == flag {
            let Some(v) = args.get(i + 1) else {
                return Err(format!("{flag} requires a value"));
            };
            out = Some(v.clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    Ok(out)
}

fn emit_json(value: &Value) -> i32 {
    match serde_json::to_string_pretty(value) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!(
                "{} llm verify: failed to serialize json: {e}",
                cli_app_name()
            );
            1
        }
    }
}

fn parse_ctx(args: &[String]) -> Result<u64, String> {
    let Some(raw) = option_value(args, "--ctx")? else {
        return Ok(8_192);
    };
    raw.parse::<u64>()
        .map_err(|_| format!("--ctx expects integer tokens, got '{raw}'"))
}

fn resolve_mlx_model_for_verify() -> Result<Value, String> {
    let input = mlx_model_preference();
    if input.trim().is_empty() {
        return Err(format!(
            "MLX model is unset; set CX_MLX_MODEL or run '{} llm use mlx <model>'",
            cli_app_name()
        ));
    }
    let resolved = resolve_model_for_backend("mlx", &input)?;
    Ok(json!({
        "input": input,
        "resolved": resolved.resolved_model,
        "alias": resolved.alias,
        "id": resolved.id,
        "preferred_args": selector_preferred_args("mlx", &input)?
    }))
}

fn mean_f64(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn run_mlx_benchmark_profile(ctx: u64, model_info: &Value) -> Result<Value, String> {
    let python = env::var("CX_MLX_PYTHON")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "python3".to_string());
    let script = env::var("CX_MLX_VERIFY_SCRIPT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            repo_root_hint()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("scripts")
                .join("tq_mlx_probe.py")
        });
    let out_file = env::temp_dir().join(format!(
        "cxrs-mlx-verify-{}-{}.json",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    let resolved_model = model_info
        .get("resolved")
        .and_then(Value::as_str)
        .ok_or_else(|| "resolved model missing".to_string())?;
    let preferred_args = model_info
        .get("preferred_args")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let env_args = env::var("CX_MLX_ARGS")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let script_arg = script.display().to_string();
    let out_arg = out_file.display().to_string();
    let ctx_arg = ctx.to_string();
    let mut cmd = Command::new(&python);
    cmd.args([
        script_arg.as_str(),
        "run",
        "--model",
        resolved_model,
        "--out",
        out_arg.as_str(),
        "--ctx",
        ctx_arg.as_str(),
        "--python",
        python.as_str(),
    ]);
    if let Some(raw_args) = preferred_args {
        cmd.args(["--preferred-args", raw_args]);
    }
    if let Some(raw_args) = env_args.as_deref() {
        cmd.args(["--mlx-args", raw_args]);
    }
    let out = run_command_output_with_timeout(cmd, "llm verify mlx benchmark")
        .map_err(|e| format!("benchmark runner failed: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "benchmark runner exited with non-zero status".to_string()
        } else {
            format!("benchmark runner failed: {stderr}")
        });
    }
    let raw = fs::read_to_string(&out_file)
        .map_err(|e| format!("benchmark output missing at {}: {e}", out_file.display()))?;
    let payload = serde_json::from_str::<Value>(&raw)
        .map_err(|e| format!("benchmark output invalid JSON: {e}"))?;
    let runs = payload
        .get("runs")
        .and_then(Value::as_array)
        .ok_or_else(|| "benchmark output missing runs".to_string())?;
    let passes = payload.get("passes").and_then(Value::as_u64).unwrap_or(0);
    let total = payload.get("total").and_then(Value::as_u64).unwrap_or(0);

    let mut prompt_tps = Vec::new();
    let mut decode_tps = Vec::new();
    let mut wall_ms = Vec::new();
    let mut cache_nbytes = Vec::new();
    let mut peak_mem = Vec::new();
    for run in runs {
        if let Some(v) = run.get("prompt_tokens_per_sec").and_then(Value::as_f64) {
            prompt_tps.push(v);
        }
        if let Some(v) = run.get("decode_tokens_per_sec").and_then(Value::as_f64) {
            decode_tps.push(v);
        }
        if let Some(v) = run.get("wall_ms").and_then(Value::as_u64) {
            wall_ms.push(v as f64);
        }
        if let Some(v) = run.get("cache_nbytes").and_then(Value::as_u64) {
            cache_nbytes.push(v as f64);
        }
        if let Some(v) = run.get("peak_memory_gb").and_then(Value::as_f64) {
            peak_mem.push(v);
        }
    }
    let peak_memory_gb_max = peak_mem.iter().fold(None, |acc: Option<f64>, v| {
        Some(acc.map_or(*v, |x| x.max(*v)))
    });
    Ok(json!({
        "profile": "benchmark",
        "model": model_info,
        "correctness": {
            "passes": passes,
            "total": total,
            "exact": total > 0 && passes == total
        },
        "runtime": {
            "prompt_tps_mean": mean_f64(&prompt_tps),
            "decode_tps_mean": mean_f64(&decode_tps),
            "wall_ms_mean": mean_f64(&wall_ms)
        },
        "memory": {
            "cache_metric_kind": "cache_nbytes",
            "cache_metric_value": mean_f64(&cache_nbytes),
            "cache_metric_unit": "bytes",
            "peak_memory_gb_max": peak_memory_gb_max
        },
        "raw_probe": payload
    }))
}

fn run_mlx_smoke_profile(prompt: &str, model_info: &Value) -> Result<Value, String> {
    let python = env::var("CX_MLX_PYTHON")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "python3".to_string());
    let resolved_model = model_info
        .get("resolved")
        .and_then(Value::as_str)
        .ok_or_else(|| "resolved model missing".to_string())?;
    let preferred_args = model_info.get("preferred_args").and_then(Value::as_str);
    let start = Instant::now();
    let output = run_mlx_plain(prompt, resolved_model, &python, preferred_args)
        .map_err(|e| e.to_string())?;
    let wall_ms = start.elapsed().as_millis() as u64;
    let exact = output.trim() == "OK";
    Ok(json!({
        "profile": "smoke",
        "model": model_info,
        "correctness": {
            "passes": if exact { 1 } else { 0 },
            "total": 1,
            "exact": exact
        },
        "runtime": {
            "prompt_tps_mean": Value::Null,
            "decode_tps_mean": Value::Null,
            "wall_ms_mean": wall_ms
        },
        "memory": {
            "cache_metric_kind": "cache_nbytes",
            "cache_metric_value": Value::Null,
            "cache_metric_unit": "bytes",
            "peak_memory_gb_max": Value::Null
        },
        "smoke_output": output.trim(),
        "prompt": prompt
    }))
}

fn llm_verify(app_name: &str, args: &[String]) -> i32 {
    let mut backend = llm_backend();
    let mut idx = 1usize;
    if let Some(token) = args.get(1).map(String::as_str)
        && !token.starts_with("--")
    {
        let Some(v) = normalize_llm_backend(token) else {
            print_llm_usage(app_name);
            return 2;
        };
        backend = v;
        idx = 2;
    }
    if backend != "mlx" {
        crate::cx_eprintln!(
            "{} llm verify: only mlx is supported in this slice; pass '{} llm verify mlx ...'",
            cli_app_name(),
            app_name
        );
        return 1;
    }
    let opts = &args[idx..];
    let profile = match option_value(opts, "--profile") {
        Ok(Some(v)) => v,
        Ok(None) => "smoke".to_string(),
        Err(e) => {
            crate::cx_eprintln!("{} llm verify: {e}", cli_app_name());
            return 2;
        }
    };
    let ctx = match parse_ctx(opts) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} llm verify: {e}", cli_app_name());
            return 2;
        }
    };
    let prompt = match option_value(opts, "--prompt") {
        Ok(Some(v)) => v,
        Ok(None) => "Respond with OK only.".to_string(),
        Err(e) => {
            crate::cx_eprintln!("{} llm verify: {e}", cli_app_name());
            return 2;
        }
    };
    let model_info = match resolve_mlx_model_for_verify() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} llm verify: {e}", cli_app_name());
            return 1;
        }
    };
    let result = match profile.as_str() {
        "smoke" => run_mlx_smoke_profile(&prompt, &model_info),
        "benchmark" => run_mlx_benchmark_profile(ctx, &model_info),
        _ => {
            crate::cx_eprintln!(
                "{} llm verify: --profile must be smoke|benchmark",
                cli_app_name()
            );
            return 2;
        }
    };
    if let Some(model_input) = model_info.get("input").and_then(Value::as_str) {
        let smoke_status = if profile == "smoke" {
            match result
                .as_ref()
                .ok()
                .and_then(|v| v.get("correctness"))
                .and_then(|v| v.get("exact"))
                .and_then(Value::as_bool)
            {
                Some(true) => Some("pass"),
                Some(false) => Some("fail"),
                None => None,
            }
        } else {
            None
        };
        let _ = note_model_usage("mlx", model_input, smoke_status);
    }
    let payload = match result {
        Ok(v) => json!({
            "contract_version": LLM_VERIFY_JSON_CONTRACT_VERSION,
            "timestamp": utc_now_iso(),
            "backend": "mlx",
            "profile": profile,
            "context_target": ctx,
            "result": v
        }),
        Err(e) => {
            crate::cx_eprintln!("{} llm verify: {e}", cli_app_name());
            return 1;
        }
    };
    if has_flag(opts, "--json") {
        return emit_json(&payload);
    }
    println!("verify_backend: mlx");
    println!(
        "verify_model_input: {}",
        payload
            .get("result")
            .and_then(|v| v.get("model"))
            .and_then(|v| v.get("input"))
            .and_then(Value::as_str)
            .unwrap_or("<unset>")
    );
    println!(
        "verify_model_resolved: {}",
        payload
            .get("result")
            .and_then(|v| v.get("model"))
            .and_then(|v| v.get("resolved"))
            .and_then(Value::as_str)
            .unwrap_or("<unset>")
    );
    println!("verify_profile: {}", profile);
    println!(
        "correctness_passes: {}",
        payload
            .get("result")
            .and_then(|v| v.get("correctness"))
            .and_then(|v| v.get("passes"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "correctness_total: {}",
        payload
            .get("result")
            .and_then(|v| v.get("correctness"))
            .and_then(|v| v.get("total"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "correctness_exact: {}",
        payload
            .get("result")
            .and_then(|v| v.get("correctness"))
            .and_then(|v| v.get("exact"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    );
    println!(
        "runtime_wall_ms_mean: {}",
        payload
            .get("result")
            .and_then(|v| v.get("runtime"))
            .and_then(|v| v.get("wall_ms_mean"))
            .map(Value::to_string)
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "memory_cache_metric_kind: {}",
        payload
            .get("result")
            .and_then(|v| v.get("memory"))
            .and_then(|v| v.get("cache_metric_kind"))
            .and_then(Value::as_str)
            .unwrap_or("n/a")
    );
    if let Some(smoke_output) = payload
        .get("result")
        .and_then(|v| v.get("smoke_output"))
        .and_then(Value::as_str)
    {
        println!("smoke_output: {smoke_output}");
    }
    0
}

fn llm_resident(app_name: &str, args: &[String]) -> i32 {
    let op = args.get(1).map(String::as_str).unwrap_or("show");
    let json_out = has_flag(args, "--json");
    let runtime_caps = selected_runtime_caps();
    let mut payload = json!({
        "contract_version": LLM_RESIDENT_JSON_CONTRACT_VERSION,
        "timestamp": utc_now_iso(),
        "selected_adapter": selected_adapter_name(),
        "selected_transport": selected_provider_transport(),
        "http_request_profile": http_profile_opt(),
        "boundary": selected_resident_json(),
        "runtime_capability": {
            "resident_server": runtime_caps.resident_server,
            "openai_compatible": runtime_caps.openai_compatible,
            "anthropic_compatible": runtime_caps.anthropic_compatible,
            "supports_batching": runtime_caps.supports_batching,
            "supports_persisted_kv_restore": runtime_caps.supports_persisted_kv_restore
        }
    });
    if op == "probe-models" {
        match probe_http_models_v1() {
            Ok(v) => {
                payload["probe"] = v;
            }
            Err(e) => {
                crate::cx_eprintln!("{} llm resident probe-models: {e}", cli_app_name());
                return 1;
            }
        }
    } else if op != "show" {
        print_llm_usage(app_name);
        return 2;
    }

    if json_out {
        return emit_json(&payload);
    }
    println!("resident_adapter: {}", selected_adapter_name());
    println!("resident_transport: {}", selected_provider_transport());
    println!(
        "resident_http_profile: {}",
        http_profile_opt().unwrap_or("n/a")
    );
    println!(
        "resident_capability: {}",
        runtime_caps
            .resident_server
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    if let Some(boundary) = payload.get("boundary") {
        println!(
            "resident_boundary_eligible: {}",
            boundary
                .get("eligible")
                .and_then(Value::as_bool)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "false".to_string())
        );
        println!(
            "resident_boundary_reason: {}",
            boundary
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
    }
    println!(
        "openai_compatible: {}",
        runtime_caps
            .openai_compatible
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    if let Some(probe) = payload.get("probe") {
        println!(
            "probe_url: {}",
            probe
                .get("probe_url")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>")
        );
        println!(
            "probe_model_count: {}",
            probe
                .get("model_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        let ids = probe
            .get("model_ids")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<&str>>()
                    .join(",")
            })
            .unwrap_or_default();
        println!(
            "probe_model_ids: {}",
            if ids.is_empty() { "<none>" } else { &ids }
        );
    }
    0
}

pub fn cmd_llm(app_name: &str, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("show") {
        "show" => llm_show(),
        "check" => llm_check(app_name, args),
        "smoke" => llm_smoke(args),
        "verify" => llm_verify(app_name, args),
        "resident" => llm_resident(app_name, args),
        "models" => llm_models::dispatch(app_name, &args[1..]),
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

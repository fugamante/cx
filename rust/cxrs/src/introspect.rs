use std::env;

use crate::capture::budget_config_from_env;
use crate::capture::rtk_is_usable;
use crate::execmeta::toolchain_version_string;
use crate::paths::{resolve_log_file, resolve_quarantine_dir, resolve_state_file};
use crate::runtime::{llm_backend, llm_model, logging_enabled};
use crate::state::{read_state_value, value_at_path};

fn value_to_display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

fn bin_in_path(bin: &str) -> bool {
    let path = match env::var_os("PATH") {
        Some(v) => v,
        None => return false,
    };
    env::split_paths(&path).any(|dir| dir.join(bin).is_file())
}

pub fn print_version(app_name: &str, app_version: &str) {
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let source = env::var("CX_SOURCE_LOCATION").unwrap_or_else(|_| "standalone:cxrs".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state_file = resolve_state_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let mode = env::var("CX_MODE").unwrap_or_else(|_| "lean".to_string());
    let schema_relaxed = env::var("CX_SCHEMA_RELAXED").unwrap_or_else(|_| "0".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    let capture_provider = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "auto".to_string());
    let native_reduce = env::var("CX_NATIVE_REDUCE").unwrap_or_else(|_| "1".to_string());
    let rtk_min = env::var("CX_RTK_MIN_VERSION").unwrap_or_else(|_| "0.22.1".to_string());
    let rtk_max = env::var("CX_RTK_MAX_VERSION").unwrap_or_default();
    let rtk_available = bin_in_path("rtk");
    let rtk_usable = rtk_is_usable();
    let budget_chars = env::var("CX_CONTEXT_BUDGET_CHARS").unwrap_or_else(|_| "12000".to_string());
    let budget_lines = env::var("CX_CONTEXT_BUDGET_LINES").unwrap_or_else(|_| "300".to_string());
    let clip_mode = env::var("CX_CONTEXT_CLIP_MODE").unwrap_or_else(|_| "smart".to_string());
    let quarantine_dir = resolve_quarantine_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state = read_state_value();
    let cc = state
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.conventional_commits"))
        .map(value_to_display)
        .unwrap_or_else(|| "n/a".to_string());
    let pr_fmt = state
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.pr_summary_format"))
        .map(value_to_display)
        .unwrap_or_else(|| "n/a".to_string());
    println!("name: {app_name}");
    println!("version: {}", toolchain_version_string(app_version));
    println!("cwd: {cwd}");
    println!("execution_path: {execution_path}");
    println!("source: {source}");
    println!("log_file: {log_file}");
    println!("state_file: {state_file}");
    println!("quarantine_dir: {quarantine_dir}");
    println!("mode: {mode}");
    println!("llm_backend: {backend}");
    println!("llm_model: {active_model}");
    println!("backend_resolution: backend={backend} model={active_model}");
    println!("schema_relaxed: {schema_relaxed}");
    println!("capture_provider: {capture_provider}");
    println!("native_reduce: {native_reduce}");
    println!("rtk_available: {rtk_available}");
    println!("rtk_supported_range_min: {rtk_min}");
    println!(
        "rtk_supported_range_max: {}",
        if rtk_max.is_empty() {
            "<unset>"
        } else {
            &rtk_max
        }
    );
    println!("rtk_usable: {rtk_usable}");
    println!("budget_chars: {budget_chars}");
    println!("budget_lines: {budget_lines}");
    println!("clip_mode: {clip_mode}");
    println!("state.preferences.conventional_commits: {cc}");
    println!("state.preferences.pr_summary_format: {pr_fmt}");
}

pub fn cmd_core(app_version: &str) -> i32 {
    let mode = env::var("CX_MODE").unwrap_or_else(|_| "lean".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    let capture_provider = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "auto".to_string());
    let rtk_enabled = env::var("CX_RTK_SYSTEM")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let rtk_available = rtk_is_usable();
    let cfg = budget_config_from_env();
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let bash_fallback = execution_path.contains("bash");

    println!("== cxcore ==");
    println!("version: {}", toolchain_version_string(app_version));
    println!("execution_path: {execution_path}");
    println!("bash_fallback_used: {bash_fallback}");
    println!("backend: {backend}");
    println!("active_model: {active_model}");
    println!("execution_mode: {mode}");
    println!("capture_provider: {capture_provider}");
    println!("capture_rtk_enabled: {rtk_enabled}");
    println!("capture_rtk_available: {rtk_available}");
    println!("budget_chars: {}", cfg.budget_chars);
    println!("budget_lines: {}", cfg.budget_lines);
    println!("clip_mode: {}", cfg.clip_mode);
    println!("clip_footer: {}", cfg.clip_footer);
    println!("schema_enforcement: true");
    println!("logging_enabled: {}", logging_enabled());
    println!("log_file: {log_file}");
    0
}

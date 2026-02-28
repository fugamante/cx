use std::env;

use crate::capture::budget_config_from_env;
use crate::capture::rtk_is_usable;
use crate::config::app_config;
use crate::execmeta::toolchain_version_string;
use crate::paths::{resolve_log_file, resolve_quarantine_dir, resolve_state_file};
use crate::provider_adapter::selected_adapter_name;
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

fn state_pref(path: &str) -> String {
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, path))
        .map(value_to_display)
        .unwrap_or_else(|| "n/a".to_string())
}

fn print_version_header(
    app_name: &str,
    app_version: &str,
    cwd: &str,
    execution_path: &str,
    source: &str,
) {
    println!("name: {app_name}");
    println!("version: {}", toolchain_version_string(app_version));
    println!("cwd: {cwd}");
    println!("execution_path: {execution_path}");
    println!("source: {source}");
}

fn print_version_paths(log_file: &str, state_file: &str, quarantine_dir: &str) {
    println!("log_file: {log_file}");
    println!("state_file: {state_file}");
    println!("quarantine_dir: {quarantine_dir}");
}

fn print_version_runtime(mode: &str, backend: &str, active_model: &str, schema_relaxed: &str) {
    let adapter_name = selected_adapter_name();
    println!("mode: {mode}");
    println!("llm_backend: {backend}");
    println!("provider_adapter: {adapter_name}");
    println!("llm_model: {active_model}");
    println!("backend_resolution: backend={backend} model={active_model}");
    println!("schema_relaxed: {schema_relaxed}");
}

fn print_version_capture(
    capture_provider: &str,
    native_reduce: &str,
    rtk_available: bool,
    rtk_min: &str,
    rtk_max: &str,
    rtk_usable: bool,
) {
    println!("capture_provider: {capture_provider}");
    println!("native_reduce: {native_reduce}");
    println!("rtk_available: {rtk_available}");
    println!("rtk_supported_range_min: {rtk_min}");
    println!(
        "rtk_supported_range_max: {}",
        if rtk_max.is_empty() {
            "<unset>"
        } else {
            rtk_max
        }
    );
    println!("rtk_usable: {rtk_usable}");
}

fn print_version_preferences() {
    println!(
        "state.preferences.conventional_commits: {}",
        state_pref("preferences.conventional_commits")
    );
    println!(
        "state.preferences.pr_summary_format: {}",
        state_pref("preferences.pr_summary_format")
    );
}

pub fn print_version(app_name: &str, app_version: &str) {
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let cfg = app_config();
    let source = env::var("CX_SOURCE_LOCATION").unwrap_or_else(|_| "standalone:cxrs".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state_file = resolve_state_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let quarantine_dir = resolve_quarantine_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };

    print_version_header(app_name, app_version, &cwd, &execution_path, &source);
    print_version_paths(&log_file, &state_file, &quarantine_dir);
    print_version_runtime(
        &cfg.cx_mode,
        &backend,
        active_model,
        if cfg.schema_relaxed { "1" } else { "0" },
    );

    let native_reduce = env::var("CX_NATIVE_REDUCE").unwrap_or_else(|_| "1".to_string());
    let rtk_min = env::var("CX_RTK_MIN_VERSION").unwrap_or_else(|_| "0.22.1".to_string());
    let rtk_max = env::var("CX_RTK_MAX_VERSION").unwrap_or_default();
    print_version_capture(
        &cfg.capture_provider,
        &native_reduce,
        bin_in_path("rtk"),
        &rtk_min,
        &rtk_max,
        rtk_is_usable(),
    );

    println!("budget_chars: {}", cfg.budget_chars);
    println!("budget_lines: {}", cfg.budget_lines);
    println!("cmd_timeout_secs: {}", cfg.cmd_timeout_secs);
    println!("clip_mode: {}", cfg.clip_mode);
    print_version_preferences();
}

pub fn cmd_core(app_version: &str) -> i32 {
    let runtime_cfg = app_config();
    let mode = runtime_cfg.cx_mode.clone();
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    let capture_provider = runtime_cfg.capture_provider.clone();
    let adapter_name = selected_adapter_name();
    let rtk_enabled = env::var("CX_RTK_SYSTEM")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let rtk_available = rtk_is_usable();
    let budget_cfg = budget_config_from_env();
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
    println!("provider_adapter: {adapter_name}");
    println!("active_model: {active_model}");
    println!("execution_mode: {mode}");
    println!("capture_provider: {capture_provider}");
    println!("capture_rtk_enabled: {rtk_enabled}");
    println!("capture_rtk_available: {rtk_available}");
    println!("budget_chars: {}", budget_cfg.budget_chars);
    println!("budget_lines: {}", budget_cfg.budget_lines);
    println!("cmd_timeout_secs: {}", runtime_cfg.cmd_timeout_secs);
    println!("clip_mode: {}", budget_cfg.clip_mode);
    println!("clip_footer: {}", budget_cfg.clip_footer);
    println!("schema_enforcement: true");
    println!("logging_enabled: {}", logging_enabled());
    println!("log_file: {log_file}");
    0
}

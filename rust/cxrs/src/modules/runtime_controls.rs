use std::env;

use crate::config::cli_app_name;

pub fn cmd_log_off() -> i32 {
    println!("{} logging: OFF (process-local)", cli_app_name());
    0
}

pub fn cmd_log_on() -> i32 {
    println!("{} logging: ON (process-local)", cli_app_name());
    0
}

pub fn cmd_alert_show() -> i32 {
    let enabled = env::var("CXALERT_ENABLED").unwrap_or_else(|_| "1".to_string());
    let max_ms = env::var("CXALERT_MAX_MS").unwrap_or_else(|_| "8000".to_string());
    let max_eff = env::var("CXALERT_MAX_EFF_IN").unwrap_or_else(|_| "5000".to_string());
    let max_out = env::var("CXALERT_MAX_OUT").unwrap_or_else(|_| "500".to_string());
    println!("{} alerts:", cli_app_name());
    println!("enabled={enabled}");
    println!("max_ms={max_ms}");
    println!("max_eff_in={max_eff}");
    println!("max_out={max_out}");
    0
}

pub fn cmd_alert_off() -> i32 {
    println!("{} alerts: OFF (process-local)", cli_app_name());
    0
}

pub fn cmd_alert_on() -> i32 {
    println!("{} alerts: ON (process-local)", cli_app_name());
    0
}

pub fn cmd_capture_status() -> i32 {
    let provider = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "native".to_string());
    let native_reduce = env::var("CX_NATIVE_REDUCE").unwrap_or_else(|_| "1".to_string());
    let prefer_native = env::var("CX_CAPTURE_PREFER_NATIVE").unwrap_or_else(|_| "1".to_string());
    println!("capture_provider: native");
    println!("capture_provider_config: {provider}");
    println!("native_reduce: {native_reduce}");
    println!("capture_prefer_native: {prefer_native}");
    println!("external_capture_dependencies: none");
    0
}

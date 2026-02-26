use std::env;

use crate::capture::rtk_is_usable;
use crate::capture::rtk_version_raw;

pub fn cmd_log_off() -> i32 {
    println!("cx logging: OFF (process-local)");
    0
}

pub fn cmd_log_on() -> i32 {
    println!("cx logging: ON (process-local)");
    0
}

pub fn cmd_alert_show() -> i32 {
    let enabled = env::var("CXALERT_ENABLED").unwrap_or_else(|_| "1".to_string());
    let max_ms = env::var("CXALERT_MAX_MS").unwrap_or_else(|_| "8000".to_string());
    let max_eff = env::var("CXALERT_MAX_EFF_IN").unwrap_or_else(|_| "5000".to_string());
    let max_out = env::var("CXALERT_MAX_OUT").unwrap_or_else(|_| "500".to_string());
    println!("cx alerts:");
    println!("enabled={enabled}");
    println!("max_ms={max_ms}");
    println!("max_eff_in={max_eff}");
    println!("max_out={max_out}");
    0
}

pub fn cmd_alert_off() -> i32 {
    println!("cx alerts: OFF (process-local)");
    0
}

pub fn cmd_alert_on() -> i32 {
    println!("cx alerts: ON (process-local)");
    0
}

pub fn cmd_rtk_status() -> i32 {
    let enabled = env::var("CX_RTK_ENABLED").unwrap_or_else(|_| "0".to_string());
    let system = env::var("CX_RTK_SYSTEM").unwrap_or_else(|_| "0".to_string());
    let mode = env::var("CX_RTK_MODE").unwrap_or_else(|_| "condense".to_string());
    let min = env::var("CX_RTK_MIN_VERSION").unwrap_or_else(|_| "0.22.1".to_string());
    let max = env::var("CX_RTK_MAX_VERSION").unwrap_or_default();
    let ver = rtk_version_raw().unwrap_or_else(|| "unavailable".to_string());
    let usable = rtk_is_usable();

    println!(
        "cxrtk: version={} range=[{}, {}] usable={} enabled={} system={} mode={}",
        ver,
        min,
        if max.is_empty() { "<unset>" } else { &max },
        usable,
        enabled,
        system,
        mode
    );
    println!("rtk_version: {ver}");
    println!("rtk_supported_min: {min}");
    println!(
        "rtk_supported_max: {}",
        if max.is_empty() { "<unset>" } else { &max }
    );
    println!("rtk_usable: {usable}");
    println!("rtk_enabled: {enabled}");
    println!("rtk_system: {system}");
    println!("rtk_mode: {mode}");
    println!(
        "fallback: {}",
        if usable { "none" } else { "raw command output" }
    );
    0
}

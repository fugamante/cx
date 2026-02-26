use std::env;
use std::process::Command;

use crate::process::run_command_output_with_timeout;
use crate::types::CaptureStats;

use super::capture_budget::{budget_config_from_env, clip_text_with_config};
use super::capture_reduce::native_reduce_output;
use super::capture_rtk::{rtk_is_usable, should_use_rtk};

fn run_capture(command: &[String]) -> Result<(String, i32), String> {
    if command.is_empty() {
        return Err("missing command".to_string());
    }
    let mut c = Command::new(&command[0]);
    if command.len() > 1 {
        c.args(&command[1..]);
    }
    let output = run_command_output_with_timeout(c, &format!("system command '{}'", command[0]))?;
    let status = output.status.code().unwrap_or(1);
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !stderr.trim().is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }
    Ok((combined, status))
}

pub fn run_system_command_capture(cmd: &[String]) -> Result<(String, i32, CaptureStats), String> {
    if cmd.is_empty() {
        return Err("missing command".to_string());
    }
    let (raw_out, status) = run_capture(cmd)?;
    let rtk_enabled = env::var("CX_RTK_SYSTEM")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let native_reduce = env::var("CX_NATIVE_REDUCE")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let provider_mode = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "auto".to_string());
    let provider_mode = match provider_mode.as_str() {
        "rtk" | "native" | "auto" => provider_mode,
        _ => "auto".to_string(),
    };
    let rtk_usable = rtk_is_usable();
    let use_rtk = should_use_rtk(cmd, &provider_mode, rtk_enabled, rtk_usable);
    let mut provider_used = if use_rtk { "rtk" } else { "native" }.to_string();
    let processed = if use_rtk {
        let mut rtk_cmd = vec!["rtk".to_string()];
        rtk_cmd.extend_from_slice(cmd);
        match run_capture(&rtk_cmd) {
            Ok((rtk_out, rtk_status)) if rtk_status == 0 && !rtk_out.trim().is_empty() => rtk_out,
            _ => {
                provider_used = "native".to_string();
                raw_out.clone()
            }
        }
    } else {
        raw_out.clone()
    };
    let reduced = if native_reduce {
        native_reduce_output(cmd, &processed)
    } else {
        processed
    };
    let (clipped_text, mut stats) = clip_text_with_config(&reduced, &budget_config_from_env());
    stats.rtk_used = Some(provider_used == "rtk");
    stats.capture_provider = Some(provider_used);
    Ok((clipped_text, status, stats))
}

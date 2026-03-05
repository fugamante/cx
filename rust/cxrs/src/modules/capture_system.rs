use std::env;
use std::process::Command;

use crate::process::run_command_output_with_timeout;
use crate::types::CaptureStats;

use super::capture_budget::{budget_config_from_env, clip_text_with_config};
use super::capture_reduce::native_reduce_output;

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
    let native_reduce = env::var("CX_NATIVE_REDUCE")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let processed = raw_out.clone();
    let reduced = if native_reduce {
        native_reduce_output(cmd, &processed)
    } else {
        processed
    };
    let (clipped_text, mut stats) = clip_text_with_config(&reduced, &budget_config_from_env());
    stats.rtk_used = Some(false);
    stats.capture_provider = Some("native".to_string());
    Ok((clipped_text, status, stats))
}

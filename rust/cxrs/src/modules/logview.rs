use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::config::app_config;
use crate::logs::load_runs;
use crate::paths::resolve_log_file;

fn show_field<T: ToString>(label: &str, value: Option<T>) {
    match value {
        Some(v) => println!("{label}: {}", v.to_string()),
        None => println!("{label}: n/a"),
    }
}

pub fn cmd_budget() -> i32 {
    let Some(log_file) = resolve_log_file() else {
        crate::cx_eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    let cfg = app_config();
    println!("== cxbudget ==");
    println!("CX_CONTEXT_BUDGET_CHARS={}", cfg.budget_chars);
    println!("CX_CONTEXT_BUDGET_LINES={}", cfg.budget_lines);
    println!("CX_CONTEXT_CLIP_MODE={}", cfg.clip_mode);
    println!(
        "CX_CONTEXT_CLIP_FOOTER={}",
        if cfg.clip_footer { "1" } else { "0" }
    );
    println!("log_file: {}", log_file.display());

    if !log_file.exists() {
        return 0;
    }
    let runs = load_runs(&log_file, 1).unwrap_or_default();
    if let Some(last) = runs.last() {
        println!();
        println!("Last run clip fields:");
        show_field("system_output_len_raw", last.system_output_len_raw);
        show_field(
            "system_output_len_processed",
            last.system_output_len_processed,
        );
        show_field("system_output_len_clipped", last.system_output_len_clipped);
        show_field("system_output_lines_raw", last.system_output_lines_raw);
        show_field(
            "system_output_lines_processed",
            last.system_output_lines_processed,
        );
        show_field(
            "system_output_lines_clipped",
            last.system_output_lines_clipped,
        );
        show_field("clipped", last.clipped);
        show_field("budget_chars", last.budget_chars);
        show_field("budget_lines", last.budget_lines);
        show_field("clip_mode", last.clip_mode.clone());
        show_field("clip_footer", last.clip_footer);
        show_field("rtk_used", last.rtk_used);
        show_field("capture_provider", last.capture_provider.clone());
    }
    0
}

pub fn cmd_log_tail(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        crate::cx_eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        crate::cx_eprintln!("cxrs log-tail: no log file at {}", log_file.display());
        return 1;
    }
    let file = match File::open(&log_file) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("cxrs log-tail: cannot open {}: {e}", log_file.display());
            return 1;
        }
    };
    let reader = BufReader::new(file);
    let mut lines: Vec<String> = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if !line.trim().is_empty() {
            lines.push(line);
        }
    }
    let start = lines.len().saturating_sub(n);
    for line in &lines[start..] {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            match serde_json::to_string_pretty(&v) {
                Ok(s) => println!("{s}"),
                Err(_) => println!("{line}"),
            }
        } else {
            println!("{line}");
        }
    }
    0
}

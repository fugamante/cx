use serde_json::{Value, json};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::command_names::{is_compat_name, is_native_name};
use crate::execmeta::toolchain_version_string;
use crate::paths::{repo_root_hint, resolve_log_file, resolve_state_file};
use crate::process::run_command_output_with_timeout;
use crate::runtime::{llm_backend, llm_model};

const ROUTE_NAMES: &[&str] = &[
    "help",
    "version",
    "where",
    "routes",
    "logs",
    "ci",
    "task",
    "diag",
    "parity",
    "doctor",
    "state",
    "llm",
    "policy",
    "bench",
    "metrics",
    "prompt",
    "roles",
    "fanout",
    "promptlint",
    "cx",
    "cxj",
    "cxo",
    "cxol",
    "cxcopy",
    "fix",
    "budget",
    "log-tail",
    "health",
    "rtk-status",
    "log-on",
    "log-off",
    "alert-show",
    "alert-on",
    "alert-off",
    "chunk",
    "cx-compat",
    "profile",
    "alert",
    "optimize",
    "worklog",
    "trace",
    "next",
    "fix-run",
    "diffsum",
    "diffsum-staged",
    "commitjson",
    "commitmsg",
    "replay",
    "quarantine",
    "supports",
    "cxversion",
    "cxdoctor",
    "cxwhere",
    "cxdiag",
    "cxparity",
    "cxlogs",
    "cxmetrics",
    "cxprofile",
    "cxtrace",
    "cxalert",
    "cxoptimize",
    "cxworklog",
    "cxpolicy",
    "cxstate",
    "cxllm",
    "cxbench",
    "cxprompt",
    "cxroles",
    "cxfanout",
    "cxpromptlint",
    "cxnext",
    "cxfix",
    "cxdiffsum",
    "cxdiffsum_staged",
    "cxcommitjson",
    "cxcommitmsg",
    "cxbudget",
    "cxlog_tail",
    "cxhealth",
    "cxrtk",
    "cxlog_on",
    "cxlog_off",
    "cxalert_show",
    "cxalert_on",
    "cxalert_off",
    "cxchunk",
    "cxfix_run",
    "cxreplay",
    "cxquarantine",
    "cxtask",
];

pub fn bash_type_of_function(repo: &Path, name: &str) -> Option<String> {
    let cx_sh = repo.join("cx.sh");
    let cmd = format!(
        "source '{}' >/dev/null 2>&1; type -a {} 2>/dev/null",
        cx_sh.display(),
        name
    );
    let mut bash_cmd = Command::new("bash");
    bash_cmd.arg("-lc").arg(cmd);
    let out = run_command_output_with_timeout(bash_cmd, "bash type -a").ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

pub fn route_handler_for(name: &str) -> Option<String> {
    if is_native_name(name) {
        Some(name.to_string())
    } else if is_compat_name(name) {
        Some(format!("cx-compat {name}"))
    } else {
        None
    }
}

pub fn rust_route_names() -> Vec<String> {
    let mut out: Vec<String> = ROUTE_NAMES.iter().map(|s| (*s).to_string()).collect();
    out.sort();
    out.dedup();
    out
}

pub fn cmd_routes(args: &[String]) -> i32 {
    let json_out = args.first().is_some_and(|a| a == "--json");
    let names: Vec<String> = if json_out {
        args[1..].to_vec()
    } else {
        args.to_vec()
    };
    let targets = if names.is_empty() {
        rust_route_names()
    } else {
        names
    };
    if json_out {
        let arr: Vec<Value> = targets
            .iter()
            .filter_map(|name| {
                route_handler_for(name).map(|handler| {
                    json!({
                        "name": name,
                        "route": "rust",
                        "handler": handler
                    })
                })
            })
            .collect();
        match serde_json::to_string_pretty(&arr) {
            Ok(s) => {
                println!("{s}");
                0
            }
            Err(e) => {
                crate::cx_eprintln!("cxrs routes: failed to render json: {e}");
                1
            }
        }
    } else {
        for name in targets {
            if let Some(handler) = route_handler_for(&name) {
                println!("{name}: rust ({handler})");
            }
        }
        0
    }
}

fn print_where_header(repo: &Path, app_version: &str) {
    let exe = env::current_exe()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let bin_cx =
        env::var("CX_BIN_CX").unwrap_or_else(|_| repo.join("bin").join("cx").display().to_string());
    let bash_lib = repo.join("lib").join("cx.sh").display().to_string();
    let source = env::var("CX_SOURCE_LOCATION").unwrap_or_else(|_| "standalone:cxrs".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state_file = resolve_state_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let repo_root = repo.display().to_string();
    let bash_sourceable = repo.join("lib").join("cx.sh").is_file();

    println!("== cxwhere ==");
    println!("bin_cx: {bin_cx}");
    println!("cxrs_path: {exe}");
    println!("cxrs_version: {}", toolchain_version_string(app_version));
    println!("bash_lib: {bash_lib}");
    println!("bash_lib_sourceable: {bash_sourceable}");
    println!("repo_root: {repo_root}");
    println!("log_file: {log_file}");
    println!("source: {source}");
    println!("state_file: {state_file}");
    println!("backend: {backend}");
    println!(
        "active_model: {}",
        if model.is_empty() { "<unset>" } else { &model }
    );
}

fn print_where_routes(repo: &Path, cmds: &[String]) {
    if cmds.is_empty() {
        return;
    }
    println!("routes:");
    for cmd in cmds {
        if let Some(handler) = route_handler_for(cmd) {
            println!("- {cmd}: route=rust handler={handler}");
            continue;
        }
        if let Some(type_out) = bash_type_of_function(repo, cmd) {
            println!("- {cmd}: route=bash function={cmd}");
            for line in type_out.lines() {
                println!("  {line}");
            }
        } else {
            println!("- {cmd}: route=unknown");
        }
    }
}

pub fn print_where(cmds: &[String], app_version: &str) -> i32 {
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    print_where_header(&repo, app_version);
    print_where_routes(&repo, cmds);
    0
}

pub fn bash_function_names(repo: &Path) -> Vec<String> {
    let cx_sh = repo.join("cx.sh");
    let cmd = format!(
        "source '{}' >/dev/null 2>&1; declare -F | awk '{{print $3}}'",
        cx_sh.display()
    );
    let mut bash_cmd = Command::new("bash");
    bash_cmd.arg("-lc").arg(cmd);
    let out = match run_command_output_with_timeout(bash_cmd, "bash declare -F") {
        Ok(v) if v.status.success() => v,
        _ => return Vec::new(),
    };
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

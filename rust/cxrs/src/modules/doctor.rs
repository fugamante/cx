use serde_json::Value;
use std::env;
use std::path::Path;
use std::process::Command;

use crate::capture::rtk_is_usable;
use crate::llm::extract_agent_text;
use crate::process::run_command_output_with_timeout;
use crate::runtime::{llm_backend, llm_bin_name};

type JsonlRunner = fn(&str) -> Result<String, String>;
type CxoRunner = fn(&[String]) -> i32;

fn bin_in_path(bin: &str) -> bool {
    let path = match env::var_os("PATH") {
        Some(v) => v,
        None => return false,
    };
    env::split_paths(&path).any(|dir| {
        let candidate = dir.join(bin);
        Path::new(&candidate).is_file()
    })
}

fn check_required_bins(backend: &str, llm_bin: &str) -> usize {
    let required = ["git", "jq"];
    let optional = ["rtk"];
    let mut missing_required = 0usize;
    for bin in required {
        if bin_in_path(bin) {
            println!("OK: {bin}");
        } else {
            println!("MISSING: {bin}");
            missing_required += 1;
        }
    }
    if bin_in_path(llm_bin) {
        println!("OK: {llm_bin} (selected backend: {backend})");
    } else {
        println!("MISSING: {llm_bin} (selected backend: {backend})");
        missing_required += 1;
    }
    if backend != "codex" {
        if bin_in_path("codex") {
            println!("OK: codex (recommended primary backend)");
        } else {
            println!("WARN: codex not found (recommended primary backend)");
        }
    }
    for bin in optional {
        if bin_in_path(bin) {
            println!("OK: {bin} (optional)");
        } else {
            println!("WARN: {bin} not found (optional)");
        }
    }
    if bin_in_path("rtk") && !rtk_is_usable() {
        println!("WARN: rtk version unsupported by configured range; raw fallback will be used.");
    }
    missing_required
}

fn probe_json_pipeline(backend: &str, run_llm_jsonl: JsonlRunner) -> Result<(), i32> {
    println!();
    println!("== llm json pipeline ({backend}) ==");
    let probe = run_llm_jsonl("ping").map_err(|e| {
        eprintln!("FAIL: {backend} json pipeline failed: {e}");
        1
    })?;
    let mut agent_count = 0u64;
    let mut reasoning_count = 0u64;
    for line in probe.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("item.completed") {
            continue;
        }
        let t = v
            .get("item")
            .and_then(|i| i.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if t == "agent_message" {
            agent_count += 1;
        } else if t == "reasoning" {
            reasoning_count += 1;
        }
    }
    println!("agent_message events: {agent_count}");
    println!("reasoning events:     {reasoning_count}");
    if agent_count < 1 {
        eprintln!("FAIL: expected >=1 agent_message event");
        return Err(1);
    }
    Ok(())
}

fn probe_text_pipeline(backend: &str, run_llm_jsonl: JsonlRunner) -> Result<(), i32> {
    println!();
    println!("== _codex_text equivalent ==");
    let probe2 = run_llm_jsonl("2+2? (just the number)").map_err(|e| {
        eprintln!("FAIL: {backend} text probe failed: {e}");
        1
    })?;
    let txt = extract_agent_text(&probe2).unwrap_or_default();
    println!("output: {txt}");
    if txt.trim() != "4" {
        println!("WARN: expected '4', got '{}'", txt.trim());
    }
    Ok(())
}

fn print_git_context() {
    println!();
    println!("== git context (optional) ==");
    let mut repo_cmd = Command::new("git");
    repo_cmd.args(["rev-parse", "--is-inside-work-tree"]);
    match run_command_output_with_timeout(repo_cmd, "git rev-parse --is-inside-work-tree") {
        Ok(out) if out.status.success() => {
            println!("in git repo: yes");
            let mut branch_cmd = Command::new("git");
            branch_cmd.args(["rev-parse", "--abbrev-ref", "HEAD"]);
            if let Ok(branch_out) =
                run_command_output_with_timeout(branch_cmd, "git rev-parse --abbrev-ref HEAD")
            {
                let branch = String::from_utf8_lossy(&branch_out.stdout)
                    .trim()
                    .to_string();
                if !branch.is_empty() {
                    println!("branch: {branch}");
                }
            }
        }
        _ => println!("in git repo: no (skip git-based checks)"),
    }
}

pub fn print_doctor(run_llm_jsonl: JsonlRunner) -> i32 {
    let backend = llm_backend();
    let llm_bin = llm_bin_name();
    println!("== cxrs doctor ==");
    let missing_required = check_required_bins(&backend, llm_bin);
    if missing_required > 0 {
        println!("FAIL: install required binaries before using cxrs.");
        return 1;
    }
    if let Err(code) = probe_json_pipeline(&backend, run_llm_jsonl) {
        return code;
    }
    if let Err(code) = probe_text_pipeline(&backend, run_llm_jsonl) {
        return code;
    }
    print_git_context();

    println!();
    println!("PASS: core pipeline looks healthy.");
    0
}

pub fn cmd_health(run_llm_jsonl: JsonlRunner, run_cxo: CxoRunner) -> i32 {
    let backend = llm_backend();
    let llm_bin = llm_bin_name();
    println!("== {backend} version ==");
    let mut version_cmd = Command::new(llm_bin);
    version_cmd.arg("--version");
    match run_command_output_with_timeout(version_cmd, &format!("{llm_bin} --version")) {
        Ok(out) => print!("{}", String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            eprintln!("cxrs health: {backend} --version failed: {e}");
            return 1;
        }
    }
    println!();
    println!("== {backend} json ==");
    let jsonl = match run_llm_jsonl("ping") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs health: {backend} json failed: {e}");
            return 1;
        }
    };
    let lines: Vec<&str> = jsonl.lines().collect();
    let keep = lines.len().saturating_sub(4);
    for line in &lines[keep..] {
        println!("{line}");
    }
    println!();
    println!("== _codex_text ==");
    let txt = extract_agent_text(&jsonl).unwrap_or_default();
    println!("{txt}");
    println!();
    println!("== cxo test ==");
    let code = run_cxo(&["git".to_string(), "status".to_string()]);
    if code != 0 {
        return code;
    }
    println!();
    println!("All systems operational.");
    0
}

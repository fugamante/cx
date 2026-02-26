use serde_json::Value;
use std::env;
use std::path::Path;
use std::process::Command;

use crate::capture::rtk_is_usable;
use crate::llm::extract_agent_text;
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

pub fn print_doctor(run_llm_jsonl: JsonlRunner) -> i32 {
    let backend = llm_backend();
    let llm_bin = llm_bin_name();
    let required = ["git", "jq"];
    let optional = ["rtk"];
    let mut missing_required = 0;

    println!("== cxrs doctor ==");
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
    if missing_required > 0 {
        println!("FAIL: install required binaries before using cxrs.");
        return 1;
    }

    println!();
    println!("== llm json pipeline ({backend}) ==");
    let probe = match run_llm_jsonl("ping") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("FAIL: {backend} json pipeline failed: {e}");
            return 1;
        }
    };
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
        return 1;
    }

    println!();
    println!("== _codex_text equivalent ==");
    let probe2 = match run_llm_jsonl("2+2? (just the number)") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("FAIL: {backend} text probe failed: {e}");
            return 1;
        }
    };
    let txt = extract_agent_text(&probe2).unwrap_or_default();
    println!("output: {txt}");
    if txt.trim() != "4" {
        println!("WARN: expected '4', got '{}'", txt.trim());
    }

    println!();
    println!("== git context (optional) ==");
    match Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
    {
        Ok(out) if out.status.success() => {
            println!("in git repo: yes");
            if let Ok(branch_out) = Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
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
    match version_cmd.output() {
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

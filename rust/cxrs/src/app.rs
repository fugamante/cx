use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use crate::agentcmds;
use crate::analytics::{
    parse_ts_epoch, print_alert, print_metrics, print_profile, print_trace, print_worklog,
};
use crate::capture::{chunk_text_by_budget, run_system_command_capture};
use crate::diagnostics::{cmd_diag, has_required_log_fields, last_appended_json_value};
use crate::doctor;
use crate::execmeta::{make_execution_id, utc_now_iso};
use crate::introspect::{
    cmd_core as introspect_cmd_core, print_version as introspect_print_version,
};
use crate::llm::{
    extract_agent_text, run_codex_jsonl, run_codex_plain, run_ollama_plain, usage_from_jsonl,
    wrap_agent_text_as_jsonl,
};
use crate::logs::{cmd_logs, file_len, load_runs_appended};
use crate::logview::{cmd_budget, cmd_log_tail};
use crate::optimize::{parse_optimize_args, print_optimize};
use crate::paths::{repo_root, repo_root_hint, resolve_log_file};
use crate::policy::{SafetyDecision, cmd_policy, evaluate_command_safety};
use crate::prompting::{cmd_fanout, cmd_prompt, cmd_promptlint, cmd_roles};
use crate::quarantine::{cmd_quarantine_list, cmd_quarantine_show, read_quarantine_record};
use crate::routing::{bash_function_names, cmd_routes, print_where, route_handler_for};
use crate::runlog::{RunLogInput, log_codex_run, log_schema_failure};
use crate::runtime::{
    llm_backend, llm_model, logging_enabled, ollama_model_preference, resolve_ollama_model_for_run,
};
use crate::runtime_controls::{
    cmd_alert_off, cmd_alert_on, cmd_alert_show, cmd_log_off, cmd_log_on, cmd_rtk_status,
};
use crate::schema::{build_strict_schema_prompt, load_schema, validate_schema_instance};
use crate::schema_ops::{cmd_ci, cmd_schema};
use crate::state::{
    current_task_id, current_task_parent_id, ensure_state_value, parse_cli_value, read_state_value,
    set_state_path, set_value_at_path, state_cache_clear, value_at_path, write_json_atomic,
};
use crate::taskrun::{TaskRunner, run_task_by_id};
use crate::tasks::{
    cmd_task_add, cmd_task_fanout, cmd_task_list, cmd_task_show, read_tasks, set_task_status,
    write_tasks,
};
use crate::types::{
    CaptureStats, ExecutionResult, LlmOutputKind, QuarantineAttempt, RunEntry, TaskInput, TaskSpec,
    UsageStats,
};
use crate::util::sha256_hex;

const APP_NAME: &str = "cxrs";
const APP_DESC: &str = "Rust spike for the cx toolchain";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("{APP_NAME} - {APP_DESC}");
    println!();
    println!("Usage:");
    println!("  {APP_NAME} <command> [args]");
    println!();
    println!("Commands:");
    println!("  version            Print tool version");
    println!("  where              Show binary/source/log resolution details");
    println!("  routes [--json] [cmd...]  Show routing map/introspection");
    println!("  diag               Non-interactive diagnostic report");
    println!("  parity             Run Rust/Bash parity invariants");
    println!("  schema list [--json]  List registered schemas");
    println!("  logs validate [--fix=false] [--legacy-ok]  Validate execution log JSONL contract");
    println!(
        "  logs migrate [--out PATH] [--in-place]  Normalize legacy run logs to current contract"
    );
    println!(
        "  ci validate [--strict] [--legacy-ok] [--json]  CI-friendly validation gate (no network)"
    );
    println!("  core               Show execution-core pipeline config");
    println!(
        "  task <op> [...]    Task graph management (add/list/claim/complete/fail/show/fanout)"
    );
    println!("  doctor             Run non-interactive environment checks");
    println!("  supports <name>    Exit 0 if subcommand is supported by cxrs");
    println!(
        "  llm <op> [...]     Manage LLM backend/model defaults (show|use|unset|set-backend|set-model|clear-model)"
    );
    println!("  state <op> [...]   Manage repo state JSON (show|get|set)");
    println!("  policy [show|check ...] Show safety rules or classify a command");
    println!("  bench <N> -- <cmd...>  Benchmark command runtime and tokens");
    println!("  cx <cmd...>        Run command output through LLM text mode");
    println!("  cxj <cmd...>       Run command output through LLM JSONL mode");
    println!("  cxo <cmd...>       Run command output and print last agent message");
    println!("  cxol <cmd...>      Run command output through LLM plain mode");
    println!("  cxcopy <cmd...>    Copy cxo output to clipboard (pbcopy)");
    println!("  fix <cmd...>       Explain failures and suggest next steps (text)");
    println!("  budget             Show context budget settings and last clip fields");
    println!("  log-tail [N]       Pretty-print last N log entries");
    println!("  health             Run end-to-end selected-LLM/cx smoke checks");
    println!("  rtk-status         Show RTK version/range decision and fallback mode");
    println!("  log-on             Enable cx logging (process-local)");
    println!("  log-off            Disable cx logging in this process");
    println!("  alert-show         Show active alert thresholds/toggles");
    println!("  alert-on           Enable alerts (process-local)");
    println!("  alert-off          Disable alerts in this process");
    println!("  chunk              Chunk stdin text by context budget chars");
    println!("  metrics [N]        Token and duration aggregates from last N runs");
    println!("  prompt <mode> <request>  Generate Codex-ready prompt block");
    println!("  roles [role]       List roles or print role-specific prompt header");
    println!("  fanout <objective> Generate role-tagged parallelizable subtasks");
    println!("  promptlint [N]     Lint prompt/cost patterns from last N runs");
    println!("  cx-compat <cmd...> Compatibility shim for bash-style cx command names");
    println!("  profile [N]        Summarize last N runs from resolved cx log (default 50)");
    println!("  alert [N]          Report anomalies from last N runs (default 50)");
    println!("  optimize [N] [--json]  Recommend cost/latency improvements from last N runs");
    println!("  worklog [N]        Emit Markdown worklog from last N runs (default 50)");
    println!("  trace [N]          Show Nth most-recent run from resolved cx log (default 1)");
    println!("  next <cmd...>      Suggest next shell commands from command output (strict JSON)");
    println!("  diffsum            Summarize unstaged diff (strict schema)");
    println!("  diffsum-staged     Summarize staged diff (strict schema)");
    println!("  fix-run <cmd...>   Suggest remediation commands for a failed command");
    println!("  commitjson         Generate strict JSON commit object from staged diff");
    println!("  commitmsg          Generate commit message text from staged diff");
    println!("  replay <id>        Replay quarantined schema run in strict mode");
    println!("  quarantine list [N]  Show recent quarantine entries (default 20)");
    println!("  quarantine show <id> Show quarantined entry payload");
    println!("  help               Print this help");
}

fn print_task_help() {
    println!("cx help task");
    println!();
    println!("Task commands:");
    println!("  cx task add \"<objective>\" --role <architect|implementer|reviewer|tester|doc>");
    println!("  cx task list [--status pending|in_progress|complete|failed]");
    println!("  cx task claim <id>");
    println!("  cx task complete <id>");
    println!("  cx task fail <id>");
    println!("  cx task show <id>");
    println!("  cx task fanout \"<objective>\" [--from staged-diff|worktree|log|file:PATH]");
    println!("  cx task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]");
    println!("  cx task run-all [--status pending]");
}

// path resolution moved to `paths.rs`
// schema helpers moved to `schema.rs`
// ensure_parent_dir moved to `paths.rs`
// run logging moved to `runlog.rs`
// state read/write moved to `state.rs`

fn cmd_task_set_status(id: &str, new_status: &str) -> i32 {
    if let Err(e) = set_task_status(id, new_status) {
        eprintln!("cxrs task: {e}");
        return 1;
    }
    if new_status == "in_progress" {
        let _ = set_state_path("runtime.current_task_id", Value::String(id.to_string()));
    } else if matches!(new_status, "complete" | "failed") {
        if current_task_id().as_deref() == Some(id) {
            let _ = set_state_path("runtime.current_task_id", Value::Null);
        }
    }
    println!("{id}: {new_status}");
    0
}

fn task_runner() -> TaskRunner {
    TaskRunner {
        read_tasks,
        write_tasks,
        current_task_id,
        current_task_parent_id,
        set_state_path,
        utc_now_iso,
        cmd_commitjson,
        cmd_commitmsg,
        cmd_diffsum,
        cmd_next,
        cmd_fix_run,
        cmd_fix,
        cmd_cx,
        cmd_cxj,
        cmd_cxo,
        execute_task,
    }
}

fn cmd_task(args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "add" => cmd_task_add(APP_NAME, &args[1..]),
        "list" => {
            let mut status_filter: Option<&str> = None;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--status" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {APP_NAME} task list [--status pending|in_progress|complete|failed]"
                            );
                            return 2;
                        };
                        if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                            eprintln!("cxrs task list: invalid status '{v}'");
                            return 2;
                        }
                        status_filter = Some(v);
                        i += 2;
                    }
                    other => {
                        eprintln!("cxrs task list: unknown flag '{other}'");
                        return 2;
                    }
                }
            }
            cmd_task_list(status_filter)
        }
        "show" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} task show <id>");
                return 2;
            };
            cmd_task_show(id)
        }
        "claim" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} task claim <id>");
                return 2;
            };
            cmd_task_set_status(id, "in_progress")
        }
        "complete" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} task complete <id>");
                return 2;
            };
            cmd_task_set_status(id, "complete")
        }
        "fail" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} task fail <id>");
                return 2;
            };
            cmd_task_set_status(id, "failed")
        }
        "fanout" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} task fanout <objective>");
                return 2;
            }
            let mut objective_parts: Vec<String> = Vec::new();
            let mut from: Option<&str> = None;
            let mut i = 1usize;
            while i < args.len() {
                if args[i] == "--from" {
                    let Some(v) = args.get(i + 1).map(String::as_str) else {
                        eprintln!(
                            "Usage: {APP_NAME} task fanout <objective> [--from staged-diff|worktree|log|file:PATH]"
                        );
                        return 2;
                    };
                    from = Some(v);
                    i += 2;
                    continue;
                }
                objective_parts.push(args[i].clone());
                i += 1;
            }
            cmd_task_fanout(APP_NAME, &objective_parts.join(" "), from)
        }
        "run" => {
            let Some(id) = args.get(1) else {
                eprintln!(
                    "Usage: {APP_NAME} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
                );
                return 2;
            };
            let mut mode_override: Option<&str> = None;
            let mut backend_override: Option<&str> = None;
            let mut i = 2usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--mode" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {APP_NAME} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
                            );
                            return 2;
                        };
                        mode_override = Some(v);
                        i += 2;
                    }
                    "--backend" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {APP_NAME} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
                            );
                            return 2;
                        };
                        backend_override = Some(v);
                        i += 2;
                    }
                    other => {
                        eprintln!("cxrs task run: unknown flag '{other}'");
                        return 2;
                    }
                }
            }
            match run_task_by_id(&task_runner(), id, mode_override, backend_override) {
                Ok((code, execution_id)) => {
                    if let Some(eid) = execution_id {
                        println!("task_id: {id}");
                        println!("execution_id: {eid}");
                    }
                    if code == 0 {
                        println!("{id}: complete");
                    } else {
                        println!("{id}: failed");
                    }
                    code
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        "run-all" => {
            let mut status_filter = "pending";
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--status" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {APP_NAME} task run-all [--status pending|in_progress|complete|failed]"
                            );
                            return 2;
                        };
                        if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                            eprintln!("cxrs task run-all: invalid status '{v}'");
                            return 2;
                        }
                        status_filter = v;
                        i += 2;
                    }
                    other => {
                        eprintln!("cxrs task run-all: unknown flag '{other}'");
                        return 2;
                    }
                }
            }
            let tasks = match read_tasks() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    return 1;
                }
            };
            let pending: Vec<String> = tasks
                .iter()
                .filter(|t| t.status == status_filter)
                .map(|t| t.id.clone())
                .collect();
            if pending.is_empty() {
                println!("No pending tasks.");
                return 0;
            }
            let mut ok = 0usize;
            let mut failed = 0usize;
            for id in pending {
                match run_task_by_id(&task_runner(), &id, None, None) {
                    Ok((code, _)) => {
                        if code == 0 {
                            ok += 1;
                        } else {
                            failed += 1;
                            eprintln!("cxrs task run-all: task failed: {id}");
                        }
                    }
                    Err(e) => {
                        eprintln!("cxrs task run-all: critical error for {id}: {e}");
                        return 1;
                    }
                }
            }
            println!("run-all summary: complete={ok}, failed={failed}");
            if failed > 0 { 1 } else { 0 }
        }
        _ => {
            eprintln!(
                "Usage: {APP_NAME} task <add|list|show|claim|complete|fail|fanout|run|run-all> ..."
            );
            2
        }
    }
}

fn cmd_state_show() -> i32 {
    let (_state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs state show: {e}");
            return 1;
        }
    };
    match serde_json::to_string_pretty(&state) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            eprintln!("cxrs state show: failed to render JSON: {e}");
            1
        }
    }
}

fn cmd_state_get(key: &str) -> i32 {
    let (state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs state get: {e}");
            return 1;
        }
    };
    let Some(v) = value_at_path(&state, key) else {
        eprintln!("cxrs state get: key not found: {key}");
        eprintln!("state_file: {}", state_file.display());
        return 1;
    };
    match v {
        Value::String(s) => println!("{s}"),
        _ => println!("{}", v),
    }
    0
}

fn cmd_state_set(key: &str, raw_value: &str) -> i32 {
    let (state_file, mut state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs state set: {e}");
            return 1;
        }
    };
    if let Err(e) = set_value_at_path(&mut state, key, parse_cli_value(raw_value)) {
        eprintln!("cxrs state set: {e}");
        return 1;
    }
    if let Err(e) = write_json_atomic(&state_file, &state) {
        eprintln!("cxrs state set: {e}");
        return 1;
    }
    state_cache_clear();
    println!("ok");
    0
}

fn cmd_llm(args: &[String]) -> i32 {
    let print_usage = || {
        eprintln!(
            "Usage: {APP_NAME} llm <show|use <codex|ollama> [model]|unset <backend|model|all>|set-backend <codex|ollama>|set-model <model>|clear-model>"
        );
    };
    let sub = args.first().map(String::as_str).unwrap_or("show");
    match sub {
        "show" => {
            let backend = llm_backend();
            let model = llm_model();
            let ollama_pref = ollama_model_preference();
            println!("llm_backend: {backend}");
            println!(
                "active_model: {}",
                if model.is_empty() { "<unset>" } else { &model }
            );
            println!(
                "ollama_model: {}",
                if ollama_pref.is_empty() {
                    "<unset>"
                } else {
                    &ollama_pref
                }
            );
            0
        }
        "use" => {
            let Some(target) = args.get(1).map(|s| s.to_lowercase()) else {
                print_usage();
                return 2;
            };
            if target != "codex" && target != "ollama" {
                print_usage();
                return 2;
            }
            if let Err(e) = set_state_path("preferences.llm_backend", Value::String(target.clone()))
            {
                eprintln!("cxrs llm use: {e}");
                return 1;
            }
            if target == "ollama" {
                if let Some(model) = args.get(2) {
                    let m = model.trim();
                    if m.is_empty() {
                        print_usage();
                        return 2;
                    }
                    if let Err(e) =
                        set_state_path("preferences.ollama_model", Value::String(m.to_string()))
                    {
                        eprintln!("cxrs llm use: {e}");
                        return 1;
                    }
                }
                println!("ok");
                println!("llm_backend: ollama");
                let pref = ollama_model_preference();
                println!(
                    "ollama_model: {}",
                    if pref.is_empty() { "<unset>" } else { &pref }
                );
                return 0;
            }
            println!("ok");
            println!("llm_backend: codex");
            0
        }
        "unset" => {
            let target = args.get(1).map(String::as_str).unwrap_or("all");
            match target {
                "backend" => {
                    if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                        eprintln!("cxrs llm unset backend: {e}");
                        return 1;
                    }
                    println!("ok");
                    println!("llm_backend: <unset>");
                    0
                }
                "model" => {
                    if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                        eprintln!("cxrs llm unset model: {e}");
                        return 1;
                    }
                    println!("ok");
                    println!("ollama_model: <unset>");
                    0
                }
                "all" => {
                    if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                        eprintln!("cxrs llm unset all: {e}");
                        return 1;
                    }
                    if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                        eprintln!("cxrs llm unset all: {e}");
                        return 1;
                    }
                    println!("ok");
                    println!("llm_backend: <unset>");
                    println!("ollama_model: <unset>");
                    0
                }
                _ => {
                    print_usage();
                    2
                }
            }
        }
        "set-backend" => {
            let Some(v) = args.get(1).map(|s| s.to_lowercase()) else {
                print_usage();
                return 2;
            };
            if v != "codex" && v != "ollama" {
                print_usage();
                return 2;
            }
            if let Err(e) = set_state_path("preferences.llm_backend", Value::String(v.clone())) {
                eprintln!("cxrs llm set-backend: {e}");
                return 1;
            }
            println!("ok");
            println!("llm_backend: {v}");
            0
        }
        "set-model" => {
            let Some(model) = args.get(1) else {
                print_usage();
                return 2;
            };
            if model.trim().is_empty() {
                print_usage();
                return 2;
            }
            if let Err(e) = set_state_path(
                "preferences.ollama_model",
                Value::String(model.trim().to_string()),
            ) {
                eprintln!("cxrs llm set-model: {e}");
                return 1;
            }
            println!("ok");
            println!("ollama_model: {}", model.trim());
            0
        }
        "clear-model" => {
            if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                eprintln!("cxrs llm clear-model: {e}");
                return 1;
            }
            println!("ok");
            println!("ollama_model: <unset>");
            0
        }
        other => {
            eprintln!("{APP_NAME} llm: unknown subcommand '{other}'");
            print_usage();
            2
        }
    }
}

// runs.jsonl readers moved to `logs.rs`

fn run_llm_plain(prompt: &str) -> Result<String, String> {
    if llm_backend() == "ollama" {
        run_ollama_plain(prompt, &resolve_ollama_model_for_run()?)
    } else {
        run_codex_plain(prompt)
    }
}

fn run_llm_jsonl(prompt: &str) -> Result<String, String> {
    if llm_backend() != "ollama" {
        return run_codex_jsonl(prompt);
    }
    let text = run_ollama_plain(prompt, &resolve_ollama_model_for_run()?)?;
    wrap_agent_text_as_jsonl(&text)
}

fn execute_task(spec: TaskSpec) -> Result<ExecutionResult, String> {
    let started = Instant::now();
    let execution_id = make_execution_id(&spec.command_name);

    let (prompt, capture_stats, system_status) = match spec.input {
        TaskInput::Prompt(p) => (p, CaptureStats::default(), None),
        TaskInput::SystemCommand(cmd) => {
            let (captured, status, stats) = run_system_command_capture(&cmd)?;
            (captured, stats, Some(status))
        }
    };
    let capture_stats = spec.capture_override.unwrap_or(capture_stats);

    let mut schema_valid: Option<bool> = None;
    let mut quarantine_id: Option<String> = None;
    let mut usage = UsageStats::default();
    let stdout: String;
    let stderr = String::new();

    match spec.output_kind {
        LlmOutputKind::Plain => {
            stdout = run_llm_plain(&prompt)?;
        }
        LlmOutputKind::Jsonl => {
            let jsonl = run_llm_jsonl(&prompt)?;
            usage = usage_from_jsonl(&jsonl);
            stdout = jsonl;
        }
        LlmOutputKind::AgentText => {
            let jsonl = run_llm_jsonl(&prompt)?;
            usage = usage_from_jsonl(&jsonl);
            stdout = extract_agent_text(&jsonl).unwrap_or_default();
        }
        LlmOutputKind::SchemaJson => {
            let schema = spec
                .schema
                .as_ref()
                .ok_or_else(|| "schema execution missing schema".to_string())?;
            let task_input = spec
                .schema_task_input
                .as_deref()
                .unwrap_or(&prompt)
                .to_string();
            let schema_pretty = serde_json::to_string_pretty(&schema.value)
                .unwrap_or_else(|_| schema.value.to_string());
            let retry_allowed = env::var("CX_SCHEMA_RELAXED").ok().as_deref() != Some("1");
            let mut attempts: Vec<QuarantineAttempt> = Vec::new();
            let mut final_reason: Option<String> = None;
            let mut last_full_prompt = build_strict_schema_prompt(&schema_pretty, &task_input);

            let run_attempt = |full_prompt: &str| -> Result<(String, UsageStats), String> {
                let jsonl = run_llm_jsonl(full_prompt)?;
                let usage = usage_from_jsonl(&jsonl);
                let raw = extract_agent_text(&jsonl).unwrap_or_default();
                Ok((raw, usage))
            };

            let validate_raw = |raw: &str| -> Result<Value, String> {
                if raw.trim().is_empty() {
                    return Err("empty_agent_message".to_string());
                }
                validate_schema_instance(schema, raw)
            };

            // Attempt 1
            let full_prompt1 = last_full_prompt.clone();
            let (raw1, usage1) = run_attempt(&full_prompt1)?;
            usage = usage1.clone();
            match validate_raw(&raw1) {
                Ok(valid) => {
                    schema_valid = Some(true);
                    stdout = valid.to_string();
                }
                Err(reason1) => {
                    final_reason = Some(reason1.clone());
                    attempts.push(QuarantineAttempt {
                        reason: reason1.clone(),
                        prompt: full_prompt1.clone(),
                        prompt_sha256: sha256_hex(&full_prompt1),
                        raw_response: raw1.clone(),
                        raw_sha256: sha256_hex(&raw1),
                    });

                    // Attempt 2 (bounded retry) before quarantine
                    if retry_allowed {
                        let full_prompt2 = format!(
                            "You are a structured output generator.\nReturn STRICT JSON ONLY. No markdown. No prose. No code fences.\nOutput MUST be a single valid JSON object matching the schema.\n\nPrevious attempt failed validation: {reason1}\n\nSchema:\n{schema_pretty}\n\nTask input:\n{task_input}\n"
                        );
                        last_full_prompt = full_prompt2.clone();
                        let (raw2, usage2) = run_attempt(&full_prompt2)?;
                        usage = usage2;
                        match validate_raw(&raw2) {
                            Ok(valid) => {
                                schema_valid = Some(true);
                                stdout = valid.to_string();
                                final_reason = None;
                                // No quarantine when retry succeeds.
                            }
                            Err(reason2) => {
                                final_reason = Some(reason2.clone());
                                attempts.push(QuarantineAttempt {
                                    reason: reason2.clone(),
                                    prompt: full_prompt2.clone(),
                                    prompt_sha256: sha256_hex(&full_prompt2),
                                    raw_response: raw2.clone(),
                                    raw_sha256: sha256_hex(&raw2),
                                });

                                let qid = log_schema_failure(
                                    &spec.command_name,
                                    &reason2,
                                    &raw2,
                                    &schema_pretty,
                                    &task_input,
                                    attempts.clone(),
                                )
                                .unwrap_or_else(|_| "".to_string());
                                schema_valid = Some(false);
                                quarantine_id = if qid.is_empty() { None } else { Some(qid) };
                                stdout = raw2;
                            }
                        }
                    } else {
                        let qid = log_schema_failure(
                            &spec.command_name,
                            &reason1,
                            &raw1,
                            &schema_pretty,
                            &task_input,
                            attempts.clone(),
                        )
                        .unwrap_or_else(|_| "".to_string());
                        schema_valid = Some(false);
                        quarantine_id = if qid.is_empty() { None } else { Some(qid) };
                        stdout = raw1;
                    }
                }
            }
            // keep full prompt hash/logging correlation for schema tasks
            if spec.logging_enabled && logging_enabled() {
                let _ = log_codex_run(RunLogInput {
                    tool: &spec.command_name,
                    prompt: &last_full_prompt,
                    duration_ms: started.elapsed().as_millis() as u64,
                    usage: Some(&usage),
                    capture: Some(&capture_stats),
                    schema_ok: schema_valid.unwrap_or(false),
                    schema_reason: if schema_valid == Some(false) {
                        final_reason.as_deref().or(Some("schema_validation_failed"))
                    } else {
                        None
                    },
                    schema_name: spec.schema.as_ref().map(|s| s.name.as_str()),
                    quarantine_id: quarantine_id.as_deref(),
                    policy_blocked: None,
                    policy_reason: None,
                });
            }
            return Ok(ExecutionResult {
                stdout,
                stderr,
                duration_ms: started.elapsed().as_millis() as u64,
                schema_valid,
                quarantine_id,
                capture_stats,
                execution_id,
                usage,
                system_status,
            });
        }
    }

    if spec.logging_enabled && logging_enabled() {
        let schema_name = spec.schema.as_ref().map(|s| s.name.as_str());
        let _ = log_codex_run(RunLogInput {
            tool: &spec.command_name,
            prompt: &prompt,
            duration_ms: started.elapsed().as_millis() as u64,
            usage: Some(&usage),
            capture: Some(&capture_stats),
            schema_ok: schema_valid.unwrap_or(true),
            schema_reason: None,
            schema_name,
            quarantine_id: quarantine_id.as_deref(),
            policy_blocked: None,
            policy_reason: None,
        });
    }

    Ok(ExecutionResult {
        stdout,
        stderr,
        duration_ms: started.elapsed().as_millis() as u64,
        schema_valid,
        quarantine_id,
        capture_stats,
        execution_id,
        usage,
        system_status,
    })
}

fn run_command_for_bench(
    command: &[String],
    disable_cx_log: bool,
    passthru: bool,
) -> Result<i32, String> {
    if command.is_empty() {
        return Err("missing command".to_string());
    }
    let mut cmd = Command::new(&command[0]);
    if command.len() > 1 {
        cmd.args(&command[1..]);
    }
    if disable_cx_log {
        cmd.env("CXLOG_ENABLED", "0");
    }
    let output = cmd
        .output()
        .map_err(|e| format!("failed to execute '{}': {e}", command[0]))?;
    if passthru {
        let mut out = std::io::stdout();
        let mut err = std::io::stderr();
        let _ = out.write_all(&output.stdout);
        let _ = err.write_all(&output.stderr);
    }
    Ok(output.status.code().unwrap_or(1))
}

fn cmd_bench(runs: usize, command: &[String]) -> i32 {
    if runs == 0 {
        eprintln!("cxrs bench: runs must be > 0");
        return 2;
    }
    if command.is_empty() {
        eprintln!("Usage: {APP_NAME} bench <runs> -- <command...>");
        return 2;
    }

    let disable_cx_log = env::var("CXBENCH_LOG").ok().as_deref() == Some("0");
    let passthru = env::var("CXBENCH_PASSTHRU").ok().as_deref() == Some("1");
    let log_file = resolve_log_file();
    let mut durations: Vec<u64> = Vec::with_capacity(runs);
    let mut eff_totals: Vec<u64> = Vec::new();
    let mut out_totals: Vec<u64> = Vec::new();
    let mut failures = 0usize;
    let mut prompt_hash_matched = 0usize;
    let mut appended_row_total = 0usize;

    for _ in 0..runs {
        let before_offset = if let Some(path) = &log_file {
            if path.exists() { file_len(path) } else { 0 }
        } else {
            0
        };

        let started = Instant::now();
        let started_epoch = Utc::now().timestamp();
        let code = match run_command_for_bench(command, disable_cx_log, passthru) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("cxrs bench: {e}");
                return 1;
            }
        };
        let ended_epoch = Utc::now().timestamp();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        durations.push(elapsed_ms);
        if code != 0 {
            failures += 1;
        }

        if let Some(path) = &log_file {
            if path.exists() && !disable_cx_log {
                let appended = load_runs_appended(path, before_offset).unwrap_or_default();
                if !appended.is_empty() {
                    let windowed: Vec<RunEntry> = appended
                        .into_iter()
                        .filter(|r| {
                            let Some(ts) = r.ts.as_deref() else {
                                return true;
                            };
                            let Some(epoch) = parse_ts_epoch(ts) else {
                                return true;
                            };
                            epoch >= started_epoch.saturating_sub(1)
                                && epoch <= ended_epoch.saturating_add(1)
                        })
                        .collect();
                    appended_row_total += windowed.len();

                    let mut hash_counts: HashMap<String, usize> = HashMap::new();
                    for r in &windowed {
                        if let Some(h) = r.prompt_sha256.as_deref() {
                            if !h.is_empty() {
                                *hash_counts.entry(h.to_string()).or_insert(0) += 1;
                            }
                        }
                    }

                    let preferred_hash = hash_counts.into_iter().max_by(|a, b| a.1.cmp(&b.1));
                    let correlated: Vec<&RunEntry> = if let Some((h, _)) = preferred_hash {
                        prompt_hash_matched += 1;
                        windowed
                            .iter()
                            .filter(|r| r.prompt_sha256.as_deref() == Some(h.as_str()))
                            .collect()
                    } else {
                        windowed.iter().collect()
                    };

                    if !correlated.is_empty() {
                        let mut eff_sum = 0u64;
                        let mut out_sum = 0u64;
                        let mut any_eff = false;
                        let mut any_out = false;
                        for r in correlated {
                            if let Some(v) = r.effective_input_tokens {
                                eff_sum += v;
                                any_eff = true;
                            }
                            if let Some(v) = r.output_tokens {
                                out_sum += v;
                                any_out = true;
                            }
                        }
                        if any_eff {
                            eff_totals.push(eff_sum);
                        }
                        if any_out {
                            out_totals.push(out_sum);
                        }
                    }
                }
            }
        }
    }

    let min = durations.iter().min().copied().unwrap_or(0);
    let max = durations.iter().max().copied().unwrap_or(0);
    let sum: u64 = durations.iter().sum();
    let avg = if durations.is_empty() {
        0
    } else {
        sum / (durations.len() as u64)
    };

    println!("== cxrs bench ==");
    println!("runs: {runs}");
    println!("command: {}", command.join(" "));
    println!("duration_ms avg/min/max: {avg}/{min}/{max}");
    println!("failures: {failures}");
    if !eff_totals.is_empty() {
        let eff_avg = eff_totals.iter().sum::<u64>() / (eff_totals.len() as u64);
        println!("avg effective_input_tokens: {eff_avg}");
    } else {
        println!("avg effective_input_tokens: n/a");
    }
    if !out_totals.is_empty() {
        let out_avg = out_totals.iter().sum::<u64>() / (out_totals.len() as u64);
        println!("avg output_tokens: {out_avg}");
    } else {
        println!("avg output_tokens: n/a");
    }
    if disable_cx_log {
        println!("cxbench_log: disabled (CXBENCH_LOG=0)");
    } else {
        println!("cxbench_log: enabled");
    }
    println!(
        "cxbench_passthru: {}",
        if passthru { "enabled" } else { "disabled" }
    );
    if !disable_cx_log {
        println!(
            "cxbench_correlation: prompt_hash_matches={}/{} runs, appended_rows={}",
            prompt_hash_matched, runs, appended_row_total
        );
    }

    if failures > 0 { 1 } else { 0 }
}

fn parse_commands_array(raw: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;
    let arr = v
        .get("commands")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing required key 'commands' array".to_string())?;
    let mut out: Vec<String> = Vec::new();
    for item in arr {
        let Some(s) = item.as_str() else {
            return Err("commands array must contain strings".to_string());
        };
        if !s.trim().is_empty() {
            out.push(s.to_string());
        }
    }
    Ok(out)
}

fn cmd_cx(command: &[String]) -> i32 {
    agentcmds::cmd_cx(command, execute_task)
}

fn cmd_cxj(command: &[String]) -> i32 {
    agentcmds::cmd_cxj(command, execute_task)
}

fn cmd_cxo(command: &[String]) -> i32 {
    agentcmds::cmd_cxo(command, execute_task)
}

fn cmd_cxol(command: &[String]) -> i32 {
    agentcmds::cmd_cxol(command, execute_task)
}

fn cmd_cxcopy(command: &[String]) -> i32 {
    agentcmds::cmd_cxcopy(command, execute_task)
}

fn cmd_fix(command: &[String]) -> i32 {
    agentcmds::cmd_fix(command, run_system_command_capture, execute_task)
}

fn cmd_parity() -> i32 {
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cxparity: cannot resolve current executable: {e}");
            return 1;
        }
    };
    let budget_chars = env::var("CX_CONTEXT_BUDGET_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(12000);

    #[derive(Default)]
    struct Row {
        cmd: String,
        rust_ok: bool,
        bash_ok: bool,
        json_ok: bool,
        logs_ok: bool,
        budget_ok: bool,
        checked: bool,
    }

    let mut rows: Vec<Row> = Vec::new();
    let mut pass_all = true;
    let ts = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let temp_repo = env::temp_dir().join(format!("cxparity-{}-{}", std::process::id(), ts));
    if fs::create_dir_all(&temp_repo).is_err() {
        eprintln!(
            "cxparity: failed to create temp repo {}",
            temp_repo.display()
        );
        return 1;
    }
    let init_ok = Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&temp_repo)
        .status()
        .ok()
        .is_some_and(|s| s.success());
    if !init_ok {
        eprintln!("cxparity: git init failed in {}", temp_repo.display());
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }
    let stage_file = temp_repo.join("cxparity_tmp.txt");
    if fs::write(&stage_file, "cx parity staged change\n").is_err() {
        eprintln!("cxparity: failed to write staged file");
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }
    let stage_ok = Command::new("git")
        .arg("add")
        .arg("cxparity_tmp.txt")
        .current_dir(&temp_repo)
        .status()
        .ok()
        .is_some_and(|s| s.success());
    if !stage_ok {
        eprintln!("cxparity: git add failed");
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }
    let temp_log_file = temp_repo.join(".codex").join("cxlogs").join("runs.jsonl");
    let bash_funcs = bash_function_names(&repo);
    let parity_catalog: Vec<(&str, Vec<&str>, Option<Vec<&str>>)> = vec![
        ("cxo", vec!["echo", "hi"], None),
        (
            "cxcommitjson",
            vec![],
            Some(vec!["subject", "body", "breaking", "tests"]),
        ),
    ];
    let overlap: Vec<(&str, Vec<&str>, Option<Vec<&str>>)> = parity_catalog
        .into_iter()
        .filter(|(cmd, _, _)| {
            route_handler_for(cmd).is_some() && bash_funcs.iter().any(|f| f == cmd)
        })
        .collect();
    if overlap.is_empty() {
        eprintln!("cxparity: no overlap commands found");
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }

    for (cmd, args, schema_keys) in overlap {
        let mut row = Row {
            cmd: cmd.to_string(),
            ..Row::default()
        };
        row.checked = true;
        let before_rust = file_len(&temp_log_file);
        let rust_out = Command::new(&exe)
            .arg("cx-compat")
            .arg(cmd)
            .args(&args)
            .current_dir(&temp_repo)
            .env("CX_EXECUTION_PATH", "rust:cxparity")
            .output();
        let rust_ok = rust_out.as_ref().is_ok_and(|o| o.status.success());
        row.rust_ok = rust_ok;
        let rust_stdout = rust_out
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let rust_json_ok = if let Some(keys) = schema_keys.as_ref() {
            if let Ok(v) = serde_json::from_str::<Value>(rust_stdout.trim()) {
                keys.iter().all(|k| v.get(*k).is_some())
            } else {
                false
            }
        } else {
            true
        };
        let rust_row = last_appended_json_value(&temp_log_file, before_rust);
        let rust_budget_ok = rust_stdout.chars().count() <= budget_chars
            || rust_row
                .as_ref()
                .and_then(|v| v.get("clipped"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let rust_log_ok = rust_row.as_ref().is_some_and(has_required_log_fields);

        let before_bash = file_len(&temp_log_file);
        let bash_cmd = format!(
            "source '{}' >/dev/null 2>&1; {} {}",
            repo.join("cx.sh").display(),
            cmd,
            args.join(" ")
        );
        let bash_out = Command::new("bash")
            .arg("-lc")
            .arg(bash_cmd)
            .current_dir(&temp_repo)
            .env("CX_EXECUTION_PATH", "bash:cxparity")
            .output();
        let bash_ok = bash_out.as_ref().is_ok_and(|o| o.status.success());
        row.bash_ok = bash_ok;
        let bash_stdout = bash_out
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let bash_json_ok = if let Some(keys) = schema_keys.as_ref() {
            if let Ok(v) = serde_json::from_str::<Value>(bash_stdout.trim()) {
                keys.iter().all(|k| v.get(*k).is_some())
            } else {
                false
            }
        } else {
            true
        };
        let bash_row = last_appended_json_value(&temp_log_file, before_bash);
        let bash_budget_ok = bash_stdout.chars().count() <= budget_chars
            || bash_row
                .as_ref()
                .and_then(|v| v.get("clipped"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let bash_log_ok = bash_row.as_ref().is_some_and(has_required_log_fields);

        row.json_ok = rust_json_ok && bash_json_ok;
        row.logs_ok = rust_log_ok && bash_log_ok;
        row.budget_ok = rust_budget_ok && bash_budget_ok;

        let row_pass = row.rust_ok && row.bash_ok && row.json_ok && row.logs_ok && row.budget_ok;
        if !row_pass {
            pass_all = false;
            eprintln!(
                "cxparity: FAIL {} rust_ok={} bash_ok={} json_ok={} logs_ok={} budget_ok={}",
                row.cmd, row.rust_ok, row.bash_ok, row.json_ok, row.logs_ok, row.budget_ok
            );
        }
        rows.push(row);
    }
    let _ = fs::remove_dir_all(&temp_repo);

    println!("cmd | rust | bash | json | logs | budget | result");
    println!("--- | --- | --- | --- | --- | --- | ---");
    for row in rows {
        let result = if row.checked
            && row.rust_ok
            && row.bash_ok
            && row.json_ok
            && row.logs_ok
            && row.budget_ok
        {
            "PASS"
        } else {
            "FAIL"
        };
        println!(
            "{} | {} | {} | {} | {} | {} | {}",
            row.cmd, row.rust_ok, row.bash_ok, row.json_ok, row.logs_ok, row.budget_ok, result
        );
    }
    if pass_all { 0 } else { 1 }
}

fn cmd_chunk() -> i32 {
    let mut buf = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
        eprintln!("cxrs chunk: failed to read stdin: {e}");
        return 1;
    }
    let budget = env::var("CX_CONTEXT_BUDGET_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(12000);
    let chunks = chunk_text_by_budget(&buf, budget);
    let total = chunks.len();
    for (i, ch) in chunks.iter().enumerate() {
        println!("----- cx chunk {}/{} -----", i + 1, total);
        print!("{ch}");
        if !ch.ends_with('\n') {
            println!();
        }
    }
    0
}

fn cmd_next(command: &[String]) -> i32 {
    let (captured, exit_status, capture_stats) = match run_system_command_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };

    let schema = match load_schema("next") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };
    let task_input = format!(
        "Based on the terminal command output below, propose the NEXT shell commands to run.\nReturn 1-6 commands in execution order.\n\nExecuted command:\n{}\nExit status: {}\n\nTERMINAL OUTPUT:\n{}",
        command.join(" "),
        exit_status,
        captured
    );
    let result = match execute_task(TaskSpec {
        command_name: "cxrs_next".to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input.clone()),
        logging_enabled: true,
        capture_override: Some(capture_stats),
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };
    if result.schema_valid == Some(false) {
        if let Some(qid) = result.quarantine_id.as_deref() {
            eprintln!("cxrs next: schema failure; quarantine_id={qid}");
        }
        eprintln!("cxrs next: raw response follows:");
        eprintln!("{}", result.stdout);
        return 1;
    }
    let schema_value: Value = match serde_json::from_str(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: invalid JSON after schema run: {e}");
            return 1;
        }
    };
    let commands = match parse_commands_array(&schema_value.to_string()) {
        Ok(v) => v,
        Err(reason) => {
            eprintln!("cxrs next: {reason}");
            return 1;
        }
    };
    for cmd in commands {
        println!("{cmd}");
    }
    0
}

fn cmd_fix_run(command: &[String]) -> i32 {
    let mut unsafe_override = false;
    let mut cmdv = command.to_vec();
    if cmdv.first().map(String::as_str) == Some("--unsafe") {
        unsafe_override = true;
        cmdv = cmdv.into_iter().skip(1).collect();
    }
    if cmdv.is_empty() {
        eprintln!("Usage: {APP_NAME} fix-run [--unsafe] <command> [args...]");
        return 2;
    }
    let (captured, exit_status, capture_stats) = match run_system_command_capture(&cmdv) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };

    let schema = match load_schema("fixrun") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };
    let task_input = format!(
        "You are my terminal debugging assistant.\nGiven the command, exit status, and output, provide concise remediation.\n\nCommand:\n{}\n\nExit status: {}\n\nOutput:\n{}",
        cmdv.join(" "),
        exit_status,
        captured
    );
    let result = match execute_task(TaskSpec {
        command_name: "cxrs_fix_run".to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input.clone()),
        logging_enabled: false,
        capture_override: Some(capture_stats),
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };
    if result.schema_valid == Some(false) {
        let _ = log_codex_run(RunLogInput {
            tool: "cxrs_fix_run",
            prompt: &task_input,
            duration_ms: result.duration_ms,
            usage: Some(&result.usage),
            capture: Some(&result.capture_stats),
            schema_ok: false,
            schema_reason: Some("schema_validation_failed"),
            schema_name: Some(schema.name.as_str()),
            quarantine_id: result.quarantine_id.as_deref(),
            policy_blocked: None,
            policy_reason: None,
        });
        if let Some(qid) = result.quarantine_id.as_deref() {
            eprintln!("cxrs fix-run: schema failure; quarantine_id={qid}");
        }
        eprintln!("cxrs fix-run: raw response follows:");
        eprintln!("{}", result.stdout);
        return 1;
    }
    let v: Value = match serde_json::from_str(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: invalid JSON after schema run: {e}");
            return 1;
        }
    };
    let analysis = v
        .get("analysis")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let commands = match parse_commands_array(&result.stdout) {
        Ok(v) => v,
        Err(reason) => {
            eprintln!("cxrs fix-run: {reason}");
            return 1;
        }
    };

    if !analysis.is_empty() {
        println!("Analysis:");
        println!("{analysis}");
        println!();
    }
    println!("Suggested commands:");
    println!("-------------------");
    for c in &commands {
        println!("{c}");
    }
    println!("-------------------");

    let should_run = env::var("CXFIX_RUN").ok().as_deref() == Some("1");
    let force = env::var("CXFIX_FORCE").ok().as_deref() == Some("1");
    let unsafe_env = env::var("CX_UNSAFE").ok().as_deref() == Some("1");
    let allow_unsafe = unsafe_override || unsafe_env;
    if !should_run {
        println!("Not running suggested commands (set CXFIX_RUN=1 to execute).");
        let _ = log_codex_run(RunLogInput {
            tool: "cxrs_fix_run",
            prompt: &task_input,
            duration_ms: result.duration_ms,
            usage: Some(&result.usage),
            capture: Some(&result.capture_stats),
            schema_ok: true,
            schema_reason: None,
            schema_name: Some(schema.name.as_str()),
            quarantine_id: None,
            policy_blocked: None,
            policy_reason: None,
        });
        return if exit_status == 0 { 0 } else { exit_status };
    }

    let mut policy_blocked = false;
    let mut policy_reasons: Vec<String> = Vec::new();
    for c in commands {
        let root = repo_root()
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        match evaluate_command_safety(&c, &root) {
            SafetyDecision::Safe => {}
            SafetyDecision::Dangerous(reason) => {
                if !(force || allow_unsafe) {
                    policy_blocked = true;
                    policy_reasons.push(reason.clone());
                    eprintln!(
                        "WARN blocked dangerous command ({reason}); use CXFIX_FORCE=1 or --unsafe: {c}"
                    );
                    continue;
                }
                eprintln!("WARN unsafe override active; executing: {c}");
            }
        }
        println!("-> {c}");
        let status = Command::new("bash").args(["-lc", &c]).status();
        if let Err(e) = status {
            eprintln!("cxrs fix-run: failed to execute command: {e}");
        }
    }

    let policy_reason_joined = if policy_reasons.is_empty() {
        None
    } else {
        Some(policy_reasons.join("; "))
    };
    let _ = log_codex_run(RunLogInput {
        tool: "cxrs_fix_run",
        prompt: &task_input,
        duration_ms: result.duration_ms,
        usage: Some(&result.usage),
        capture: Some(&result.capture_stats),
        schema_ok: true,
        schema_reason: None,
        schema_name: Some(schema.name.as_str()),
        quarantine_id: None,
        policy_blocked: Some(policy_blocked),
        policy_reason: policy_reason_joined.as_deref(),
    });

    if exit_status == 0 { 0 } else { exit_status }
}

fn generate_commitjson_value() -> Result<Value, String> {
    let (diff_out, status, capture_stats) = run_system_command_capture(&[
        "git".to_string(),
        "diff".to_string(),
        "--staged".to_string(),
        "--no-color".to_string(),
    ])?;
    if status != 0 {
        return Err(format!(
            "git diff --staged failed with exit status {status}: {diff_out}"
        ));
    }
    if diff_out.trim().is_empty() {
        return Err("no staged changes. run: git add -p".to_string());
    }

    let conventional = read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.conventional_commits"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let style_hint = if conventional {
        "Use concise conventional-commit style subject."
    } else {
        "Use concise imperative subject (non-conventional format)."
    };
    let schema = load_schema("commitjson")?;
    let task_input = format!(
        "Generate a commit object from this STAGED diff.\n{style_hint}\n\nSTAGED DIFF:\n{diff_out}"
    );
    let result = execute_task(TaskSpec {
        command_name: "cxrs_commitjson".to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input),
        logging_enabled: true,
        capture_override: Some(capture_stats),
    })?;
    let mut v: Value = if result.schema_valid == Some(false) {
        let qid = result.quarantine_id.unwrap_or_default();
        return Err(format!(
            "schema validation failed; quarantine_id={qid}; raw={}",
            result.stdout
        ));
    } else {
        serde_json::from_str(&result.stdout).map_err(|e| format!("invalid JSON: {e}"))?
    };
    if v.get("scope").is_none() {
        if let Some(obj) = v.as_object_mut() {
            obj.insert("scope".to_string(), Value::Null);
        }
    }
    Ok(v)
}

fn render_bullets(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Some(Value::String(s)) => {
            if s.trim().is_empty() {
                Vec::new()
            } else {
                vec![s.trim().to_string()]
            }
        }
        _ => Vec::new(),
    }
}

fn print_diffsum_human(v: &Value) {
    let title = v.get("title").and_then(Value::as_str).unwrap_or("");
    let summary = render_bullets(v.get("summary"));
    let risks = render_bullets(v.get("risk_edge_cases"));
    let tests = render_bullets(v.get("suggested_tests"));

    println!("Title: {title}");
    println!();
    println!("Summary:");
    if summary.is_empty() {
        println!("- n/a");
    } else {
        for s in summary {
            println!("- {s}");
        }
    }
    println!();
    println!("Risk/edge cases:");
    if risks.is_empty() {
        println!("- n/a");
    } else {
        for s in risks {
            println!("- {s}");
        }
    }
    println!();
    println!("Suggested tests:");
    if tests.is_empty() {
        println!("- n/a");
    } else {
        for s in tests {
            println!("- {s}");
        }
    }
}

fn generate_diffsum_value(tool: &str, staged: bool) -> Result<Value, String> {
    let git_cmd = if staged {
        vec![
            "git".to_string(),
            "diff".to_string(),
            "--staged".to_string(),
            "--no-color".to_string(),
        ]
    } else {
        vec![
            "git".to_string(),
            "diff".to_string(),
            "--no-color".to_string(),
        ]
    };
    let (diff_out, status, capture_stats) = run_system_command_capture(&git_cmd)?;
    if status != 0 {
        return Err(format!("git diff failed with status {status}"));
    }
    if diff_out.trim().is_empty() {
        if staged {
            return Err("no staged changes.".to_string());
        }
        return Err("no unstaged changes.".to_string());
    }

    let pr_fmt = read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.pr_summary_format"))
        .and_then(Value::as_str)
        .unwrap_or("standard")
        .to_string();
    let schema = load_schema("diffsum")?;
    let diff_label = if staged { "STAGED DIFF" } else { "DIFF" };
    let task_input = format!(
        "Write a PR-ready summary of this diff.\nKeep bullets concise and actionable.\nPreferred PR summary format: {pr_fmt}\n\n{diff_label}:\n{diff_out}"
    );
    let result = execute_task(TaskSpec {
        command_name: tool.to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input),
        logging_enabled: true,
        capture_override: Some(capture_stats),
    })?;
    if result.schema_valid == Some(false) {
        return Err(format!(
            "schema validation failed; quarantine_id={}; raw={}",
            result.quarantine_id.unwrap_or_default(),
            result.stdout
        ));
    }
    serde_json::from_str(&result.stdout).map_err(|e| format!("invalid JSON: {e}"))
}

fn cmd_diffsum(staged: bool) -> i32 {
    let tool = if staged {
        "cxrs_diffsum_staged"
    } else {
        "cxrs_diffsum"
    };
    match generate_diffsum_value(tool, staged) {
        Ok(v) => {
            print_diffsum_human(&v);
            0
        }
        Err(e) => {
            eprintln!(
                "cxrs {}: {e}",
                if staged { "diffsum-staged" } else { "diffsum" }
            );
            1
        }
    }
}

fn cmd_commitjson() -> i32 {
    match generate_commitjson_value() {
        Ok(v) => match serde_json::to_string_pretty(&v) {
            Ok(s) => {
                println!("{s}");
                0
            }
            Err(e) => {
                eprintln!("cxrs commitjson: render failure: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("cxrs commitjson: {e}");
            1
        }
    }
}

fn cmd_commitmsg() -> i32 {
    let v = match generate_commitjson_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs commitmsg: {e}");
            return 1;
        }
    };
    let subject = v
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let body_items: Vec<String> = v
        .get("body")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    let test_items: Vec<String> = v
        .get("tests")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    println!("{subject}");
    println!();
    if !body_items.is_empty() {
        for line in body_items {
            println!("- {line}");
        }
    }
    if !test_items.is_empty() {
        println!();
        println!("Tests:");
        for line in test_items {
            println!("- {line}");
        }
    }
    0
}

fn cmd_replay(id: &str) -> i32 {
    let rec = match read_quarantine_record(id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs replay: {e}");
            return 1;
        }
    };

    if rec.schema.trim().is_empty() || rec.prompt.trim().is_empty() {
        eprintln!("cxrs replay: quarantine entry is missing schema/prompt payload");
        return 1;
    }

    let full_prompt = build_strict_schema_prompt(&rec.schema, &rec.prompt);
    let jsonl = match run_llm_jsonl(&full_prompt) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs replay: {e}");
            return 1;
        }
    };
    let raw = extract_agent_text(&jsonl).unwrap_or_default();
    if raw.trim().is_empty() {
        match log_schema_failure(
            &format!("{}_replay", rec.tool),
            "empty_agent_message",
            &raw,
            &rec.schema,
            &rec.prompt,
            Vec::new(),
        ) {
            Ok(qid) => eprintln!("cxrs replay: empty response; quarantine_id={qid}"),
            Err(e) => eprintln!("cxrs replay: failed to log schema failure: {e}"),
        }
        return 1;
    }

    if serde_json::from_str::<Value>(&raw).is_err() {
        match log_schema_failure(
            &format!("{}_replay", rec.tool),
            "invalid_json",
            &raw,
            &rec.schema,
            &rec.prompt,
            Vec::new(),
        ) {
            Ok(qid) => eprintln!("cxrs replay: invalid JSON; quarantine_id={qid}"),
            Err(e) => eprintln!("cxrs replay: failed to log schema failure: {e}"),
        }
        eprintln!("cxrs replay: raw response follows:");
        eprintln!("{raw}");
        return 1;
    }

    println!("{raw}");
    0
}

fn cmd_cx_compat(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!("Usage: {APP_NAME} cx <command> [args...]");
        return 2;
    }
    let sub = args[0].as_str();
    match sub {
        "help" => {
            if args.get(1).map(String::as_str) == Some("task") {
                print_task_help();
            } else {
                print_help();
            }
            0
        }
        "cxversion" | "version" => {
            introspect_print_version(APP_NAME, APP_VERSION);
            0
        }
        "cxdoctor" | "doctor" => doctor::print_doctor(run_llm_jsonl),
        "cxwhere" | "where" => print_where(&args[1..], APP_VERSION),
        "cxroutes" | "routes" => cmd_routes(&args[1..]),
        "cxdiag" | "diag" => cmd_diag(APP_VERSION),
        "cxparity" | "parity" => cmd_parity(),
        "cxcore" | "core" => introspect_cmd_core(APP_VERSION),
        "cxlogs" | "logs" => cmd_logs(APP_NAME, &args[1..]),
        "cxtask" | "task" => cmd_task(&args[1..]),
        "cxmetrics" | "metrics" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_metrics(n)
        }
        "cxprofile" | "profile" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_profile(n)
        }
        "cxtrace" | "trace" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(1);
            print_trace(n)
        }
        "cxalert" | "alert" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_alert(n)
        }
        "cxoptimize" | "optimize" => {
            let (n, json_out) = match parse_optimize_args(&args[1..], 200) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{APP_NAME} cx optimize: {e}");
                    return 2;
                }
            };
            print_optimize(n, json_out)
        }
        "cxworklog" | "worklog" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_worklog(n)
        }
        "cx" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx <command> [args...]");
                return 2;
            }
            cmd_cx(&args[1..])
        }
        "cxj" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cxj <command> [args...]");
                return 2;
            }
            cmd_cxj(&args[1..])
        }
        "cxo" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cxo <command> [args...]");
                return 2;
            }
            cmd_cxo(&args[1..])
        }
        "cxol" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cxol <command> [args...]");
                return 2;
            }
            cmd_cxol(&args[1..])
        }
        "cxcopy" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cxcopy <command> [args...]");
                return 2;
            }
            cmd_cxcopy(&args[1..])
        }
        "cxpolicy" | "policy" => cmd_policy(&args[1..], APP_NAME),
        "cxstate" | "state" => match args.get(1).map(String::as_str).unwrap_or("show") {
            "show" => cmd_state_show(),
            "get" => {
                let Some(key) = args.get(2) else {
                    eprintln!("Usage: {APP_NAME} cx state get <key>");
                    return 2;
                };
                cmd_state_get(key)
            }
            "set" => {
                let Some(key) = args.get(2) else {
                    eprintln!("Usage: {APP_NAME} cx state set <key> <value>");
                    return 2;
                };
                let Some(value) = args.get(3) else {
                    eprintln!("Usage: {APP_NAME} cx state set <key> <value>");
                    return 2;
                };
                cmd_state_set(key, value)
            }
            other => {
                eprintln!("{APP_NAME} cx state: unknown subcommand '{other}'");
                2
            }
        },
        "cxllm" | "llm" => cmd_llm(&args[1..]),
        "cxbench" | "bench" => {
            let Some(runs) = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
            else {
                eprintln!("Usage: {APP_NAME} cx bench <runs> -- <command...>");
                return 2;
            };
            let delim = args.iter().position(|v| v == "--");
            let Some(i) = delim else {
                eprintln!("Usage: {APP_NAME} cx bench <runs> -- <command...>");
                return 2;
            };
            if i + 1 >= args.len() {
                eprintln!("Usage: {APP_NAME} cx bench <runs> -- <command...>");
                return 2;
            }
            cmd_bench(runs, &args[i + 1..])
        }
        "cxprompt" | "prompt" => {
            let Some(mode) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} cx prompt <mode> <request>");
                return 2;
            };
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cx prompt <mode> <request>");
                return 2;
            }
            cmd_prompt(mode, &args[2..].join(" "))
        }
        "cxroles" | "roles" => cmd_roles(args.get(1).map(String::as_str)),
        "cxfanout" | "fanout" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx fanout <objective>");
                return 2;
            }
            cmd_fanout(&args[1..].join(" "))
        }
        "cxpromptlint" | "promptlint" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(200);
            cmd_promptlint(n)
        }
        "cxnext" | "next" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx next <command> [args...]");
                return 2;
            }
            cmd_next(&args[1..])
        }
        "cxfix" | "fix" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx fix <command> [args...]");
                return 2;
            }
            cmd_fix(&args[1..])
        }
        "cxdiffsum" | "diffsum" => cmd_diffsum(false),
        "cxdiffsum_staged" | "diffsum-staged" => cmd_diffsum(true),
        "cxcommitjson" | "commitjson" => cmd_commitjson(),
        "cxcommitmsg" | "commitmsg" => cmd_commitmsg(),
        "cxbudget" | "budget" => cmd_budget(),
        "cxlog_tail" | "log-tail" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(10);
            cmd_log_tail(n)
        }
        "cxhealth" | "health" => doctor::cmd_health(run_llm_jsonl, cmd_cxo),
        "cxrtk" | "rtk-status" => cmd_rtk_status(),
        "cxlog_on" | "log-on" => cmd_log_on(),
        "cxlog_off" | "log-off" => cmd_log_off(),
        "cxalert_show" | "alert-show" => cmd_alert_show(),
        "cxalert_on" | "alert-on" => cmd_alert_on(),
        "cxalert_off" | "alert-off" => cmd_alert_off(),
        "cxchunk" | "chunk" => cmd_chunk(),
        "cxfix_run" | "fix-run" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx fix-run <command> [args...]");
                return 2;
            }
            cmd_fix_run(&args[1..])
        }
        "cxreplay" | "replay" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} cx replay <quarantine_id>");
                return 2;
            };
            cmd_replay(id)
        }
        "cxquarantine" | "quarantine" => match args.get(1).map(String::as_str).unwrap_or("list") {
            "list" => {
                let n = args
                    .get(2)
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0)
                    .unwrap_or(20);
                cmd_quarantine_list(n)
            }
            "show" => {
                let Some(id) = args.get(2) else {
                    eprintln!("Usage: {APP_NAME} cx quarantine show <quarantine_id>");
                    return 2;
                };
                cmd_quarantine_show(id)
            }
            other => {
                eprintln!("{APP_NAME} cx quarantine: unknown subcommand '{other}'");
                2
            }
        },
        other => {
            eprintln!("{APP_NAME} cx: unsupported command '{other}'");
            2
        }
    }
}

pub(crate) fn is_compat_name(name: &str) -> bool {
    matches!(
        name,
        "help"
            | "cxversion"
            | "version"
            | "cxdoctor"
            | "doctor"
            | "cxwhere"
            | "where"
            | "cxroutes"
            | "routes"
            | "cxdiag"
            | "diag"
            | "cxparity"
            | "parity"
            | "cxcore"
            | "core"
            | "cxlogs"
            | "logs"
            | "cxtask"
            | "task"
            | "cxmetrics"
            | "metrics"
            | "cxprofile"
            | "profile"
            | "cxtrace"
            | "trace"
            | "cxalert"
            | "alert"
            | "cxoptimize"
            | "optimize"
            | "cxworklog"
            | "worklog"
            | "cx"
            | "cxj"
            | "cxo"
            | "cxol"
            | "cxcopy"
            | "cxpolicy"
            | "policy"
            | "cxstate"
            | "state"
            | "cxllm"
            | "llm"
            | "cxbench"
            | "bench"
            | "cxprompt"
            | "prompt"
            | "cxroles"
            | "roles"
            | "cxfanout"
            | "fanout"
            | "cxpromptlint"
            | "promptlint"
            | "cxnext"
            | "next"
            | "cxfix"
            | "fix"
            | "cxdiffsum"
            | "diffsum"
            | "cxdiffsum_staged"
            | "diffsum-staged"
            | "cxcommitjson"
            | "commitjson"
            | "cxcommitmsg"
            | "commitmsg"
            | "cxbudget"
            | "budget"
            | "cxlog_tail"
            | "log-tail"
            | "cxhealth"
            | "health"
            | "cxrtk"
            | "rtk-status"
            | "cxlog_on"
            | "log-on"
            | "cxlog_off"
            | "log-off"
            | "cxalert_show"
            | "alert-show"
            | "cxalert_on"
            | "alert-on"
            | "cxalert_off"
            | "alert-off"
            | "cxchunk"
            | "chunk"
            | "cxfix_run"
            | "fix-run"
            | "cxreplay"
            | "replay"
            | "cxquarantine"
            | "quarantine"
            | "schema"
    )
}

pub(crate) fn is_native_name(name: &str) -> bool {
    matches!(
        name,
        "help"
            | "-h"
            | "--help"
            | "version"
            | "-V"
            | "--version"
            | "where"
            | "routes"
            | "diag"
            | "parity"
            | "core"
            | "logs"
            | "ci"
            | "task"
            | "doctor"
            | "state"
            | "llm"
            | "policy"
            | "bench"
            | "metrics"
            | "prompt"
            | "roles"
            | "fanout"
            | "promptlint"
            | "cx"
            | "cxj"
            | "cxo"
            | "cxol"
            | "cxcopy"
            | "fix"
            | "budget"
            | "log-tail"
            | "health"
            | "rtk-status"
            | "log-on"
            | "log-off"
            | "alert-show"
            | "alert-on"
            | "alert-off"
            | "chunk"
            | "cx-compat"
            | "profile"
            | "alert"
            | "optimize"
            | "worklog"
            | "trace"
            | "next"
            | "fix-run"
            | "diffsum"
            | "diffsum-staged"
            | "commitjson"
            | "commitmsg"
            | "replay"
            | "quarantine"
            | "supports"
            | "schema"
    )
}

pub fn run() -> i32 {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    let code = match cmd {
        "help" | "-h" | "--help" => {
            if args.get(2).map(String::as_str) == Some("task") {
                print_task_help();
            } else {
                print_help();
            }
            0
        }
        "version" | "-V" | "--version" => {
            introspect_print_version(APP_NAME, APP_VERSION);
            0
        }
        "schema" => cmd_schema(APP_NAME, &args[2..]),
        "logs" => cmd_logs(APP_NAME, &args[2..]),
        "ci" => cmd_ci(APP_NAME, &args[2..]),
        "core" => introspect_cmd_core(APP_VERSION),
        "task" => cmd_task(&args[2..]),
        "where" => print_where(&args[2..], APP_VERSION),
        "routes" => cmd_routes(&args[2..]),
        "diag" => cmd_diag(APP_VERSION),
        "parity" => cmd_parity(),
        "supports" => {
            let Some(name) = args.get(2) else {
                eprintln!("Usage: {APP_NAME} supports <subcommand>");
                std::process::exit(2);
            };
            if is_native_name(name) || is_compat_name(name) {
                println!("true");
                0
            } else {
                println!("false");
                1
            }
        }
        "doctor" => doctor::print_doctor(run_llm_jsonl),
        "state" => match args.get(2).map(String::as_str).unwrap_or("show") {
            "show" => cmd_state_show(),
            "get" => {
                let Some(key) = args.get(3) else {
                    eprintln!("Usage: {APP_NAME} state get <key>");
                    std::process::exit(2);
                };
                cmd_state_get(key)
            }
            "set" => {
                let Some(key) = args.get(3) else {
                    eprintln!("Usage: {APP_NAME} state set <key> <value>");
                    std::process::exit(2);
                };
                let Some(value) = args.get(4) else {
                    eprintln!("Usage: {APP_NAME} state set <key> <value>");
                    std::process::exit(2);
                };
                cmd_state_set(key, value)
            }
            other => {
                eprintln!("{APP_NAME}: unknown state subcommand '{other}'");
                eprintln!("Usage: {APP_NAME} state <show|get <key>|set <key> <value>>");
                2
            }
        },
        "llm" => cmd_llm(&args[2..]),
        "policy" => cmd_policy(&args[2..], APP_NAME),
        "bench" => {
            if args.len() < 5 {
                eprintln!("Usage: {APP_NAME} bench <runs> -- <command...>");
                std::process::exit(2);
            }
            let runs = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(0);
            let delim = args.iter().position(|v| v == "--");
            let Some(i) = delim else {
                eprintln!("Usage: {APP_NAME} bench <runs> -- <command...>");
                std::process::exit(2);
            };
            if i + 1 >= args.len() {
                eprintln!("Usage: {APP_NAME} bench <runs> -- <command...>");
                std::process::exit(2);
            }
            cmd_bench(runs, &args[i + 1..])
        }
        "metrics" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_metrics(n)
        }
        "prompt" => {
            let Some(mode) = args.get(2) else {
                eprintln!("Usage: {APP_NAME} prompt <implement|fix|test|doc|ops> <request>");
                std::process::exit(2);
            };
            if args.len() < 4 {
                eprintln!("Usage: {APP_NAME} prompt <implement|fix|test|doc|ops> <request>");
                std::process::exit(2);
            }
            let request = args[3..].join(" ");
            cmd_prompt(mode, &request)
        }
        "roles" => cmd_roles(args.get(2).map(String::as_str)),
        "fanout" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} fanout <objective>");
                std::process::exit(2);
            }
            cmd_fanout(&args[2..].join(" "))
        }
        "promptlint" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(200);
            cmd_promptlint(n)
        }
        "cx" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cx <command> [args...]");
                std::process::exit(2);
            }
            if is_compat_name(&args[2]) {
                cmd_cx_compat(&args[2..])
            } else {
                cmd_cx(&args[2..])
            }
        }
        "cxj" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cxj <command> [args...]");
                std::process::exit(2);
            }
            cmd_cxj(&args[2..])
        }
        "cxo" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cxo <command> [args...]");
                std::process::exit(2);
            }
            cmd_cxo(&args[2..])
        }
        "cxol" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cxol <command> [args...]");
                std::process::exit(2);
            }
            cmd_cxol(&args[2..])
        }
        "cxcopy" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cxcopy <command> [args...]");
                std::process::exit(2);
            }
            cmd_cxcopy(&args[2..])
        }
        "fix" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} fix <command> [args...]");
                std::process::exit(2);
            }
            cmd_fix(&args[2..])
        }
        "budget" => cmd_budget(),
        "log-tail" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(10);
            cmd_log_tail(n)
        }
        "health" => doctor::cmd_health(run_llm_jsonl, cmd_cxo),
        "rtk-status" => cmd_rtk_status(),
        "log-on" => cmd_log_on(),
        "log-off" => cmd_log_off(),
        "alert-show" => cmd_alert_show(),
        "alert-on" => cmd_alert_on(),
        "alert-off" => cmd_alert_off(),
        "chunk" => cmd_chunk(),
        "cx-compat" => cmd_cx_compat(&args[2..]),
        "profile" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_profile(n)
        }
        "alert" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_alert(n)
        }
        "optimize" => {
            let (n, json_out) = match parse_optimize_args(&args[2..], 200) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{APP_NAME} optimize: {e}");
                    std::process::exit(2);
                }
            };
            print_optimize(n, json_out)
        }
        "worklog" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_worklog(n)
        }
        "trace" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(1);
            print_trace(n)
        }
        "next" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} next <command> [args...]");
                std::process::exit(2);
            }
            cmd_next(&args[2..])
        }
        "diffsum" => cmd_diffsum(false),
        "diffsum-staged" => cmd_diffsum(true),
        "fix-run" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} fix-run <command> [args...]");
                std::process::exit(2);
            }
            cmd_fix_run(&args[2..])
        }
        "commitjson" => cmd_commitjson(),
        "commitmsg" => cmd_commitmsg(),
        "replay" => {
            let Some(id) = args.get(2) else {
                eprintln!("Usage: {APP_NAME} replay <quarantine_id>");
                std::process::exit(2);
            };
            cmd_replay(id)
        }
        "quarantine" => match args.get(2).map(String::as_str).unwrap_or("list") {
            "list" => {
                let n = args
                    .get(3)
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0)
                    .unwrap_or(20);
                cmd_quarantine_list(n)
            }
            "show" => {
                let Some(id) = args.get(3) else {
                    eprintln!("Usage: {APP_NAME} quarantine show <quarantine_id>");
                    std::process::exit(2);
                };
                cmd_quarantine_show(id)
            }
            other => {
                eprintln!("{APP_NAME}: unknown quarantine subcommand '{other}'");
                eprintln!("Usage: {APP_NAME} quarantine <list [N]|show <id>>");
                2
            }
        },
        _ => {
            eprintln!("{APP_NAME}: unknown command '{cmd}'");
            eprintln!("Run '{APP_NAME} help' for usage.");
            2
        }
    };
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{BudgetConfig, choose_clip_mode, clip_text_with_config, should_use_rtk};
    use crate::logs::append_jsonl;
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn smart_mode_prefers_tail_on_error_keywords() {
        assert_eq!(choose_clip_mode("all good", "smart"), "head");
        assert_eq!(choose_clip_mode("WARNING: issue", "smart"), "tail");
        assert_eq!(choose_clip_mode("failed to run", "smart"), "tail");
    }

    #[test]
    fn clip_text_respects_line_and_char_budget() {
        let cfg = BudgetConfig {
            budget_chars: 12,
            budget_lines: 2,
            clip_mode: "head".to_string(),
            clip_footer: false,
        };
        let (out, stats) = clip_text_with_config("line1\nline2\nline3\n", &cfg);
        assert!(out.starts_with("line1\nline2"));
        assert_eq!(stats.budget_chars, Some(12));
        assert_eq!(stats.budget_lines, Some(2));
        assert_eq!(stats.clipped, Some(true));
    }

    #[test]
    fn jsonl_append_integrity() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("runs.jsonl");
        append_jsonl(&file, &json!({"a":1})).expect("append 1");
        append_jsonl(&file, &json!({"b":2})).expect("append 2");
        let content = fs::read_to_string(&file).expect("read");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        let v1: Value = serde_json::from_str(lines[0]).expect("line1 json");
        let v2: Value = serde_json::from_str(lines[1]).expect("line2 json");
        assert_eq!(v1.get("a").and_then(Value::as_i64), Some(1));
        assert_eq!(v2.get("b").and_then(Value::as_i64), Some(2));
    }

    #[test]
    fn rtk_unavailable_path_uses_native() {
        let cmd = vec!["git".to_string(), "status".to_string()];
        assert!(!should_use_rtk(&cmd, "auto", true, false));
        assert!(!should_use_rtk(&cmd, "native", true, true));
    }

    #[test]
    fn schema_failure_writes_quarantine_and_logs() {
        let _guard = cwd_lock().lock().expect("lock");
        let dir = tempdir().expect("tempdir");
        let prev = env::current_dir().expect("cwd");
        env::set_current_dir(dir.path()).expect("cd temp");
        let _ = Command::new("git")
            .args(["init"])
            .output()
            .expect("git init");

        let qid = log_schema_failure(
            "cxrs_next",
            "invalid_json",
            "raw",
            "{}",
            "prompt",
            Vec::new(),
        )
        .expect("schema failure log");
        assert!(!qid.is_empty());

        let qfile = dir
            .path()
            .join(".codex")
            .join("quarantine")
            .join(format!("{qid}.json"));
        assert!(qfile.exists());

        let sf_log = dir
            .path()
            .join(".codex")
            .join("cxlogs")
            .join("schema_failures.jsonl");
        let sf = fs::read_to_string(&sf_log).expect("read schema fail log");
        let last_sf: Value =
            serde_json::from_str(sf.lines().last().expect("sf line")).expect("sf json");
        assert_eq!(
            last_sf.get("quarantine_id").and_then(Value::as_str),
            Some(qid.as_str())
        );

        let runs_log = dir.path().join(".codex").join("cxlogs").join("runs.jsonl");
        let runs = fs::read_to_string(&runs_log).expect("read runs");
        let last_run: Value =
            serde_json::from_str(runs.lines().last().expect("run line")).expect("run json");
        assert_eq!(
            last_run.get("quarantine_id").and_then(Value::as_str),
            Some(qid.as_str())
        );
        assert_eq!(
            last_run.get("schema_valid").and_then(Value::as_bool),
            Some(false)
        );

        env::set_current_dir(prev).expect("restore cwd");
    }
}

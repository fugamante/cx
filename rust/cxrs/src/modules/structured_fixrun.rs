use serde_json::Value;
use std::env;
use std::path::PathBuf;
use std::process::Command;

use crate::capture::run_system_command_capture;
use crate::config::app_config;
use crate::error::{EXIT_OK, EXIT_RUNTIME, EXIT_USAGE, format_error};
use crate::paths::repo_root;
use crate::policy::{SafetyDecision, evaluate_command_safety};
use crate::runlog::{RunLogInput, log_codex_run};
use crate::schema::load_schema;
use crate::types::{ExecutionResult, LlmOutputKind, TaskInput, TaskSpec};

pub type ExecuteTaskFn = fn(TaskSpec) -> Result<ExecutionResult, String>;

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

pub fn cmd_fix_run(app_name: &str, command: &[String], execute_task: ExecuteTaskFn) -> i32 {
    let mut unsafe_override = false;
    let mut cmdv = command.to_vec();
    if cmdv.first().map(String::as_str) == Some("--unsafe") {
        unsafe_override = true;
        cmdv = cmdv.into_iter().skip(1).collect();
    }
    if cmdv.is_empty() {
        eprintln!(
            "{}",
            format_error(
                "fix-run",
                &format!("Usage: {app_name} fix-run [--unsafe] <command> [args...]")
            )
        );
        return EXIT_USAGE;
    }
    let (captured, exit_status, capture_stats) = match run_system_command_capture(&cmdv) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", format_error("fix-run", &e));
            return EXIT_RUNTIME;
        }
    };

    let schema = match load_schema("fixrun") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", format_error("fix-run", &e));
            return EXIT_RUNTIME;
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
            eprintln!("{}", format_error("fix-run", &e));
            return EXIT_RUNTIME;
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
            eprintln!(
                "{}",
                format_error("fix-run", &format!("schema failure; quarantine_id={qid}"))
            );
        }
        eprintln!("{}", format_error("fix-run", "raw response follows:"));
        eprintln!("{}", result.stdout);
        return EXIT_RUNTIME;
    }
    let v: Value = match serde_json::from_str(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "{}",
                format_error("fix-run", &format!("invalid JSON after schema run: {e}"))
            );
            return EXIT_RUNTIME;
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
            eprintln!("{}", format_error("fix-run", &reason));
            return EXIT_RUNTIME;
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

    let cfg = app_config();
    let should_run = cfg.cxfix_run;
    let force = cfg.cxfix_force;
    let unsafe_env = cfg.cx_unsafe;
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
        return if exit_status == 0 {
            EXIT_OK
        } else {
            exit_status
        };
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
            eprintln!(
                "{}",
                format_error("fix-run", &format!("failed to execute command: {e}"))
            );
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

    if exit_status == 0 {
        EXIT_OK
    } else {
        exit_status
    }
}

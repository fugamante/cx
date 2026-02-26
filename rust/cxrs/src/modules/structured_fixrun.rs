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
struct FixRunCtx {
    exit_status: i32,
    task_input: String,
    schema_name: String,
    result: ExecutionResult,
    analysis: String,
    commands: Vec<String>,
}

fn load_fix_schema_or_exit() -> Result<crate::types::LoadedSchema, i32> {
    load_schema("fixrun").map_err(|e| {
        eprintln!("{}", format_error("fix-run", &e));
        EXIT_RUNTIME
    })
}

fn capture_fix_context(cmdv: &[String]) -> Result<(String, i32, crate::types::CaptureStats), i32> {
    run_system_command_capture(cmdv).map_err(|e| {
        eprintln!("{}", format_error("fix-run", &e));
        EXIT_RUNTIME
    })
}

fn execute_fix_schema_task(
    execute_task: ExecuteTaskFn,
    schema: &crate::types::LoadedSchema,
    task_input: &str,
    capture_stats: crate::types::CaptureStats,
) -> Result<ExecutionResult, i32> {
    execute_task(TaskSpec {
        command_name: "cxrs_fix_run".to_string(),
        input: TaskInput::Prompt(task_input.to_string()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input.to_string()),
        logging_enabled: false,
        capture_override: Some(capture_stats),
    })
    .map_err(|e| {
        eprintln!("{}", format_error("fix-run", &e));
        EXIT_RUNTIME
    })
}

fn parse_fix_response(raw: &str) -> Result<(String, Vec<String>), i32> {
    let v: Value = serde_json::from_str(raw).map_err(|e| {
        eprintln!(
            "{}",
            format_error("fix-run", &format!("invalid JSON after schema run: {e}"))
        );
        EXIT_RUNTIME
    })?;
    let analysis = v
        .get("analysis")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let commands = parse_commands_array(raw).map_err(|reason| {
        eprintln!("{}", format_error("fix-run", &reason));
        EXIT_RUNTIME
    })?;
    Ok((analysis, commands))
}

fn log_schema_failure_and_exit(
    schema_name: &str,
    task_input: &str,
    result: &ExecutionResult,
) -> Result<(), i32> {
    if result.schema_valid != Some(false) {
        return Ok(());
    }
    let _ = log_codex_run(RunLogInput {
        tool: "cxrs_fix_run",
        prompt: task_input,
        schema_prompt: None,
        schema_raw: None,
        schema_attempt: None,
        duration_ms: result.duration_ms,
        usage: Some(&result.usage),
        capture: Some(&result.capture_stats),
        schema_ok: false,
        schema_reason: Some("schema_validation_failed"),
        schema_name: Some(schema_name),
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
    Err(EXIT_RUNTIME)
}

fn log_fix_run(ctx: &FixRunCtx, policy_blocked: Option<bool>, policy_reason: Option<&str>) {
    let _ = log_codex_run(RunLogInput {
        tool: "cxrs_fix_run",
        prompt: &ctx.task_input,
        schema_prompt: None,
        schema_raw: None,
        schema_attempt: None,
        duration_ms: ctx.result.duration_ms,
        usage: Some(&ctx.result.usage),
        capture: Some(&ctx.result.capture_stats),
        schema_ok: true,
        schema_reason: None,
        schema_name: Some(ctx.schema_name.as_str()),
        quarantine_id: None,
        policy_blocked,
        policy_reason,
    });
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

fn parse_fix_run_args(app_name: &str, command: &[String]) -> Result<(bool, Vec<String>), i32> {
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
        return Err(EXIT_USAGE);
    }
    Ok((unsafe_override, cmdv))
}

fn run_fix_analysis(cmdv: Vec<String>, execute_task: ExecuteTaskFn) -> Result<FixRunCtx, i32> {
    let (captured, exit_status, capture_stats) = capture_fix_context(&cmdv)?;
    let schema = load_fix_schema_or_exit()?;
    let task_input = format!(
        "You are my terminal debugging assistant.\nGiven the command, exit status, and output, provide concise remediation.\n\nCommand:\n{}\n\nExit status: {}\n\nOutput:\n{}",
        cmdv.join(" "),
        exit_status,
        captured
    );
    let result = execute_fix_schema_task(execute_task, &schema, &task_input, capture_stats)?;
    log_schema_failure_and_exit(schema.name.as_str(), &task_input, &result)?;
    let (analysis, commands) = parse_fix_response(&result.stdout)?;
    Ok(FixRunCtx {
        exit_status,
        task_input,
        schema_name: schema.name,
        result,
        analysis,
        commands,
    })
}

fn print_fix_suggestions(analysis: &str, commands: &[String]) {
    if !analysis.is_empty() {
        println!("Analysis:");
        println!("{analysis}");
        println!();
    }
    println!("Suggested commands:");
    println!("-------------------");
    for c in commands {
        println!("{c}");
    }
    println!("-------------------");
}

fn execute_fix_commands(
    commands: &[String],
    force: bool,
    allow_unsafe: bool,
) -> (bool, Option<String>) {
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
        if let Err(e) = Command::new("bash").args(["-lc", c]).status() {
            eprintln!(
                "{}",
                format_error("fix-run", &format!("failed to execute command: {e}"))
            );
        }
    }
    let reason = if policy_reasons.is_empty() {
        None
    } else {
        Some(policy_reasons.join("; "))
    };
    (policy_blocked, reason)
}

pub fn cmd_fix_run(app_name: &str, command: &[String], execute_task: ExecuteTaskFn) -> i32 {
    let (unsafe_override, cmdv) = match parse_fix_run_args(app_name, command) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let ctx = match run_fix_analysis(cmdv, execute_task) {
        Ok(v) => v,
        Err(code) => return code,
    };
    print_fix_suggestions(&ctx.analysis, &ctx.commands);

    let cfg = app_config();
    let should_run = cfg.cxfix_run;
    let force = cfg.cxfix_force;
    let unsafe_env = cfg.cx_unsafe;
    let allow_unsafe = unsafe_override || unsafe_env;
    if !should_run {
        println!("Not running suggested commands (set CXFIX_RUN=1 to execute).");
        log_fix_run(&ctx, None, None);
        return if ctx.exit_status == 0 {
            EXIT_OK
        } else {
            ctx.exit_status
        };
    }
    let (policy_blocked, policy_reason_joined) =
        execute_fix_commands(&ctx.commands, force, allow_unsafe);
    log_fix_run(&ctx, Some(policy_blocked), policy_reason_joined.as_deref());

    if ctx.exit_status == 0 {
        EXIT_OK
    } else {
        ctx.exit_status
    }
}

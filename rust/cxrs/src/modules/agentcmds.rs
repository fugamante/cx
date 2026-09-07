use std::process::Command;
use std::time::Instant;

use crate::error::{EXIT_OK, format_error, print_runtime_error};
use crate::process::run_command_with_stdin_output_with_timeout;
use crate::runlog::{RunLogInput, log_primary_run};
use crate::types::UsageStats;
use crate::types::{CaptureStats, ExecutionResult, LlmOutputKind, TaskInput, TaskSpec};

type TaskRunner = fn(TaskSpec) -> Result<ExecutionResult, String>;
type CaptureRunner = fn(&[String]) -> Result<(String, i32, CaptureStats), String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LlmMode {
    Plain,
    Jsonl,
    AgentText,
    SchemaJson,
}

struct ClipboardBackend {
    bin: &'static str,
    args: &'static [&'static str],
    label: &'static str,
}

fn clipboard_backends() -> &'static [ClipboardBackend] {
    &[
        ClipboardBackend {
            bin: "pbcopy",
            args: &[],
            label: "pbcopy",
        },
        ClipboardBackend {
            bin: "wl-copy",
            args: &[],
            label: "wl-copy",
        },
        ClipboardBackend {
            bin: "xclip",
            args: &["-selection", "clipboard"],
            label: "xclip",
        },
    ]
}

fn mode_to_task_spec(command: &[String], mode: LlmMode) -> Result<TaskSpec, String> {
    let (command_name, output_kind) = match mode {
        LlmMode::Plain => ("cx", LlmOutputKind::Plain),
        LlmMode::Jsonl => ("cxj", LlmOutputKind::Jsonl),
        LlmMode::AgentText => ("cxo", LlmOutputKind::AgentText),
        LlmMode::SchemaJson => {
            return Err(
                "LlmMode::SchemaJson requires explicit schema metadata; use structured commands"
                    .to_string(),
            );
        }
    };
    Ok(TaskSpec {
        command_name: command_name.to_string(),
        input: TaskInput::SystemCommand(command.to_vec()),
        output_kind,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    })
}

pub fn execute_llm_command(
    command: &[String],
    mode: LlmMode,
    run_task: TaskRunner,
) -> Result<ExecutionResult, String> {
    let spec = mode_to_task_spec(command, mode)?;
    run_task(spec)
}

fn run_and_print(
    command: &[String],
    mode: LlmMode,
    run_task: TaskRunner,
    with_newline: bool,
) -> i32 {
    let result = match execute_llm_command(command, mode, run_task) {
        Ok(v) => v,
        Err(e) => {
            let name = match mode {
                LlmMode::Plain => "cx",
                LlmMode::Jsonl => "cxj",
                LlmMode::AgentText => "cxo",
                LlmMode::SchemaJson => "cx-schema",
            };
            return print_runtime_error(name, &e);
        }
    };
    if with_newline {
        println!("{}", result.stdout);
    } else {
        print!("{}", result.stdout);
    }
    result.system_status.unwrap_or(0)
}

pub fn cmd_cx(command: &[String], run_task: TaskRunner) -> i32 {
    run_and_print(command, LlmMode::Plain, run_task, false)
}

pub fn cmd_cxj(command: &[String], run_task: TaskRunner) -> i32 {
    run_and_print(command, LlmMode::Jsonl, run_task, false)
}

pub fn cmd_cxo(command: &[String], run_task: TaskRunner) -> i32 {
    run_and_print(command, LlmMode::AgentText, run_task, true)
}

pub fn cmd_cxol(command: &[String], run_task: TaskRunner) -> i32 {
    run_and_print(command, LlmMode::Plain, run_task, false)
}

pub fn cmd_cxcopy(command: &[String], run_task: TaskRunner) -> i32 {
    let result = match run_task(TaskSpec {
        command_name: "cxcopy".to_string(),
        input: TaskInput::SystemCommand(command.to_vec()),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    }) {
        Ok(v) => v,
        Err(e) => {
            return print_runtime_error("cxcopy", &e);
        }
    };
    let text = result.stdout;
    if text.trim().is_empty() {
        return print_runtime_error("cxcopy", "nothing to copy");
    }
    let mut failures: Vec<String> = Vec::new();
    for backend in clipboard_backends() {
        let mut cmd = Command::new(backend.bin);
        if !backend.args.is_empty() {
            cmd.args(backend.args);
        }
        match run_command_with_stdin_output_with_timeout(cmd, &text, backend.label) {
            Ok(out) if out.status.success() => {
                println!("Copied to clipboard ({})", backend.bin);
                return result.system_status.unwrap_or(0);
            }
            Ok(out) => failures.push(format!("{} exited with status {}", backend.bin, out.status)),
            Err(e) => failures.push(format!("{} unavailable/failed: {}", backend.bin, e)),
        }
    }
    print_runtime_error(
        "cxcopy",
        &format!("all clipboard backends failed: {}", failures.join("; ")),
    )
}

pub fn cmd_capture(command: &[String], run_capture: CaptureRunner) -> i32 {
    let started = Instant::now();
    let (captured, status, capture_stats) = match run_capture(command) {
        Ok(v) => v,
        Err(e) => {
            return print_runtime_error("capture", &e);
        }
    };
    print!("{captured}");
    if !captured.ends_with('\n') {
        println!();
    }

    let usage = UsageStats {
        input_tokens: Some(0),
        cached_input_tokens: Some(0),
        output_tokens: Some(0),
    };
    let command_label = shell_words::join(command.iter().map(String::as_str));
    if let Err(e) = log_primary_run(RunLogInput {
        tool: "capture",
        prompt: &captured,
        prompt_raw: Some(&captured),
        prompt_filtered: Some(&captured),
        schema_prompt: None,
        schema_raw: None,
        schema_attempt: None,
        timed_out: None,
        timeout_secs: None,
        system_status: Some(status),
        command_label: Some(&command_label),
        duration_ms: started.elapsed().as_millis() as u64,
        usage: Some(&usage),
        capture: Some(&capture_stats),
        schema_ok: true,
        schema_reason: None,
        schema_name: None,
        quarantine_id: None,
        policy_blocked: None,
        policy_reason: None,
    }) {
        crate::cx_eprintln!("capture: failed to write run log: {e}");
    }
    status
}

pub fn cmd_fix(command: &[String], run_capture: CaptureRunner, run_task: TaskRunner) -> i32 {
    let (captured, status, capture_stats) = match run_capture(command) {
        Ok(v) => v,
        Err(e) => {
            return print_runtime_error("fix", &e);
        }
    };
    let prompt = format!(
        "You are my terminal debugging assistant.\nTask:\n1) Explain what happened (brief).\n2) If the command failed, diagnose likely cause(s).\n3) Propose the next 3 commands to run to confirm/fix.\n4) If it is a configuration issue, point to exact file/line patterns to check.\n\nCommand:\n{}\n\nExit status: {}\n\nOutput:\n{}",
        command.join(" "),
        status,
        captured
    );
    let result = match run_task(TaskSpec {
        command_name: "cxfix".to_string(),
        input: TaskInput::Prompt(prompt),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: Some(capture_stats),
    }) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{}", format_error("fix", &e));
            return status;
        }
    };
    println!("{}", result.stdout);
    if status == 0 { EXIT_OK } else { status }
}

use std::io::Write;
use std::process::{Command, Stdio};

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
            eprintln!("cxrs {name}: {e}");
            return 1;
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
            eprintln!("cxrs cxcopy: {e}");
            return 1;
        }
    };
    let text = result.stdout;
    if text.trim().is_empty() {
        eprintln!("cxrs cxcopy: nothing to copy");
        return 1;
    }
    let mut pb = match Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxcopy: pbcopy unavailable: {e}");
            return 1;
        }
    };
    if let Some(stdin) = pb.stdin.as_mut() {
        let _ = stdin.write_all(text.as_bytes());
    }
    match pb.wait() {
        Ok(s) if s.success() => {
            println!("Copied to clipboard.");
            result.system_status.unwrap_or(0)
        }
        Ok(s) => {
            eprintln!("cxrs cxcopy: pbcopy failed with status {}", s);
            1
        }
        Err(e) => {
            eprintln!("cxrs cxcopy: pbcopy wait failed: {e}");
            1
        }
    }
}

pub fn cmd_fix(command: &[String], run_capture: CaptureRunner, run_task: TaskRunner) -> i32 {
    let (captured, status, capture_stats) = match run_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix: {e}");
            return 1;
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
            eprintln!("cxrs fix: {e}");
            return status;
        }
    };
    println!("{}", result.stdout);
    status
}

use std::io::Write;
use std::process::{Command, Stdio};

use crate::types::{CaptureStats, ExecutionResult, LlmOutputKind, TaskInput, TaskSpec};

type TaskRunner = fn(TaskSpec) -> Result<ExecutionResult, String>;
type CaptureRunner = fn(&[String]) -> Result<(String, i32, CaptureStats), String>;

pub fn cmd_cx(command: &[String], run_task: TaskRunner) -> i32 {
    let result = match run_task(TaskSpec {
        command_name: "cx".to_string(),
        input: TaskInput::SystemCommand(command.to_vec()),
        output_kind: LlmOutputKind::Plain,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cx: {e}");
            return 1;
        }
    };
    print!("{}", result.stdout);
    result.system_status.unwrap_or(0)
}

pub fn cmd_cxj(command: &[String], run_task: TaskRunner) -> i32 {
    let result = match run_task(TaskSpec {
        command_name: "cxj".to_string(),
        input: TaskInput::SystemCommand(command.to_vec()),
        output_kind: LlmOutputKind::Jsonl,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxj: {e}");
            return 1;
        }
    };
    print!("{}", result.stdout);
    result.system_status.unwrap_or(0)
}

pub fn cmd_cxo(command: &[String], run_task: TaskRunner) -> i32 {
    let result = match run_task(TaskSpec {
        command_name: "cxo".to_string(),
        input: TaskInput::SystemCommand(command.to_vec()),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxo: {e}");
            return 1;
        }
    };
    println!("{}", result.stdout);
    result.system_status.unwrap_or(0)
}

pub fn cmd_cxol(command: &[String], run_task: TaskRunner) -> i32 {
    cmd_cx(command, run_task)
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

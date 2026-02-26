use serde_json::Value;
use std::env;
use std::fmt;
use std::process::Command;

use crate::types::{ExecutionResult, LlmOutputKind, TaskInput, TaskRecord, TaskSpec};

#[derive(Debug, Clone)]
pub enum TaskRunError {
    Critical(String),
}

impl fmt::Display for TaskRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskRunError::Critical(s) => write!(f, "{s}"),
        }
    }
}

pub struct TaskRunner {
    pub read_tasks: fn() -> Result<Vec<TaskRecord>, String>,
    pub write_tasks: fn(&[TaskRecord]) -> Result<(), String>,
    pub current_task_id: fn() -> Option<String>,
    pub current_task_parent_id: fn() -> Option<String>,
    pub set_state_path: fn(&str, Value) -> Result<(), String>,
    pub utc_now_iso: fn() -> String,
    pub cmd_commitjson: fn() -> i32,
    pub cmd_commitmsg: fn() -> i32,
    pub cmd_diffsum: fn(bool) -> i32,
    pub cmd_next: fn(&[String]) -> i32,
    pub cmd_fix_run: fn(&[String]) -> i32,
    pub cmd_fix: fn(&[String]) -> i32,
    pub cmd_cx: fn(&[String]) -> i32,
    pub cmd_cxj: fn(&[String]) -> i32,
    pub cmd_cxo: fn(&[String]) -> i32,
    pub execute_task: fn(TaskSpec) -> Result<ExecutionResult, String>,
}

fn parse_words(input: &str) -> Vec<String> {
    match shell_words::split(input) {
        Ok(v) => v,
        Err(_) => input
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
    }
}

fn command_status_or_usage(run: fn(&[String]) -> i32, args: &[String]) -> i32 {
    if args.is_empty() { 2 } else { run(args) }
}

fn task_prompt(task: &TaskRecord) -> String {
    if task.context_ref.trim().is_empty() {
        return format!(
            "Task Objective:\n{}\n\nRespond with concise execution notes and next actions.",
            task.objective
        );
    }
    format!(
        "Task Objective:\n{}\n\nContext Ref:\n{}\n\nRespond with concise execution notes and next actions.",
        task.objective, task.context_ref
    )
}

fn run_task_prompt(
    runner: &TaskRunner,
    task: &TaskRecord,
) -> Result<(i32, Option<String>), String> {
    let res = (runner.execute_task)(TaskSpec {
        command_name: "cxtask_run".to_string(),
        input: TaskInput::Prompt(task_prompt(task)),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    })?;
    println!("{}", res.stdout);
    Ok((0, Some(res.execution_id)))
}

fn run_objective_subprocess(
    objective_words: &[String],
    mode_override: Option<&str>,
    backend_override: Option<&str>,
) -> Result<i32, String> {
    if objective_words.is_empty() {
        return Ok(2);
    }
    let exe = env::current_exe().map_err(|e| format!("cxrs task run: current_exe failed: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.args(objective_words);
    if let Some(mode) = mode_override {
        cmd.env("CX_MODE", mode);
    }
    if let Some(backend) = backend_override {
        cmd.env("CX_LLM_BACKEND", backend);
    }
    let status = crate::process::run_command_status_with_timeout(cmd, "cxtask_run subprocess")?;
    Ok(status.code().unwrap_or(1))
}

fn dispatch_task_command(
    runner: &TaskRunner,
    words: &[String],
    task: &TaskRecord,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
) -> Result<(i32, Option<String>), String> {
    let Some(cmd0) = words.first().map(String::as_str) else {
        return run_task_prompt(runner, task);
    };
    let args: Vec<String> = words.iter().skip(1).cloned().collect();
    if mode_override.is_some() || backend_override.is_some() {
        match cmd0 {
            "cxcommitjson" | "commitjson" | "cxcommitmsg" | "commitmsg" | "cxdiffsum"
            | "diffsum" | "cxdiffsum_staged" | "diffsum-staged" | "cxnext" | "next"
            | "cxfix_run" | "fix-run" | "cxfix" | "fix" | "cx" | "cxj" | "cxo" => {
                let code = run_objective_subprocess(words, mode_override, backend_override)?;
                return Ok((code, None));
            }
            _ => {}
        }
    }
    let status = match cmd0 {
        "cxcommitjson" | "commitjson" => (runner.cmd_commitjson)(),
        "cxcommitmsg" | "commitmsg" => (runner.cmd_commitmsg)(),
        "cxdiffsum" | "diffsum" => (runner.cmd_diffsum)(false),
        "cxdiffsum_staged" | "diffsum-staged" => (runner.cmd_diffsum)(true),
        "cxnext" | "next" => command_status_or_usage(runner.cmd_next, &args),
        "cxfix_run" | "fix-run" => command_status_or_usage(runner.cmd_fix_run, &args),
        "cxfix" | "fix" => command_status_or_usage(runner.cmd_fix, &args),
        "cx" => command_status_or_usage(runner.cmd_cx, &args),
        "cxj" => command_status_or_usage(runner.cmd_cxj, &args),
        "cxo" => command_status_or_usage(runner.cmd_cxo, &args),
        _ => return run_task_prompt(runner, task),
    };
    Ok((status, None))
}

fn run_task_objective(
    runner: &TaskRunner,
    task: &TaskRecord,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
) -> Result<(i32, Option<String>), String> {
    let words = parse_words(&task.objective);
    dispatch_task_command(runner, &words, task, mode_override, backend_override)
}

fn set_runtime_task_state(runner: &TaskRunner, id: &str, parent_id: Option<&String>) {
    let _ = (runner.set_state_path)("runtime.current_task_id", Value::String(id.to_string()));
    let _ = (runner.set_state_path)(
        "runtime.current_task_parent_id",
        match parent_id {
            Some(v) => Value::String(v.clone()),
            None => Value::Null,
        },
    );
}

fn restore_runtime_task_state(
    runner: &TaskRunner,
    prev_task_id: Option<String>,
    prev_parent_id: Option<String>,
) {
    let _ = (runner.set_state_path)(
        "runtime.current_task_id",
        prev_task_id.map_or(Value::Null, Value::String),
    );
    let _ = (runner.set_state_path)(
        "runtime.current_task_parent_id",
        prev_parent_id.map_or(Value::Null, Value::String),
    );
}

fn finalize_task_status(
    runner: &TaskRunner,
    id: &str,
    status_code: i32,
) -> Result<(), TaskRunError> {
    let mut tasks = (runner.read_tasks)().map_err(TaskRunError::Critical)?;
    let idx = tasks
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| TaskRunError::Critical(format!("cxrs task run: task disappeared: {id}")))?;
    tasks[idx].status = if status_code == 0 {
        "complete".to_string()
    } else {
        "failed".to_string()
    };
    tasks[idx].updated_at = (runner.utc_now_iso)();
    (runner.write_tasks)(&tasks).map_err(TaskRunError::Critical)?;
    if (runner.current_task_id)().as_deref() == Some(id) {
        let _ = (runner.set_state_path)("runtime.current_task_id", Value::Null);
    }
    Ok(())
}

pub fn run_task_by_id(
    runner: &TaskRunner,
    id: &str,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
) -> Result<(i32, Option<String>), TaskRunError> {
    let mut tasks = (runner.read_tasks)().map_err(TaskRunError::Critical)?;
    let idx = tasks
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| TaskRunError::Critical(format!("cxrs task run: task not found: {id}")))?;
    if matches!(tasks[idx].status.as_str(), "complete" | "failed") {
        return Ok((0, None));
    }
    tasks[idx].status = "in_progress".to_string();
    tasks[idx].updated_at = (runner.utc_now_iso)();
    (runner.write_tasks)(&tasks).map_err(TaskRunError::Critical)?;
    let prev_task_id = (runner.current_task_id)();
    let prev_parent_id = (runner.current_task_parent_id)();
    set_runtime_task_state(runner, id, tasks[idx].parent_id.as_ref());

    let exec = run_task_objective(runner, &tasks[idx], mode_override, backend_override);
    restore_runtime_task_state(runner, prev_task_id, prev_parent_id);

    let (status_code, execution_id, objective_err) = match exec {
        Ok((c, eid)) => (c, eid, None),
        Err(e) => (1, None, Some(e)),
    };

    finalize_task_status(runner, id, status_code)?;
    if let Some(e) = objective_err {
        crate::cx_eprintln!("cxrs task run: objective failed for {id}: {e}");
    }
    Ok((status_code, execution_id))
}

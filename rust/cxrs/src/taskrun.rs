use serde_json::Value;
use std::env;
use std::fmt;

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
    input
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn run_task_objective(runner: &TaskRunner, task: &TaskRecord) -> Result<(i32, Option<String>), String> {
    let words = parse_words(&task.objective);
    if let Some(cmd0) = words.first().map(String::as_str) {
        let args: Vec<String> = words.iter().skip(1).cloned().collect();
        let status = match cmd0 {
            "cxcommitjson" | "commitjson" => (runner.cmd_commitjson)(),
            "cxcommitmsg" | "commitmsg" => (runner.cmd_commitmsg)(),
            "cxdiffsum" | "diffsum" => (runner.cmd_diffsum)(false),
            "cxdiffsum_staged" | "diffsum-staged" => (runner.cmd_diffsum)(true),
            "cxnext" | "next" => {
                if args.is_empty() {
                    2
                } else {
                    (runner.cmd_next)(&args)
                }
            }
            "cxfix_run" | "fix-run" => {
                if args.is_empty() {
                    2
                } else {
                    (runner.cmd_fix_run)(&args)
                }
            }
            "cxfix" | "fix" => {
                if args.is_empty() {
                    2
                } else {
                    (runner.cmd_fix)(&args)
                }
            }
            "cx" => {
                if args.is_empty() {
                    2
                } else {
                    (runner.cmd_cx)(&args)
                }
            }
            "cxj" => {
                if args.is_empty() {
                    2
                } else {
                    (runner.cmd_cxj)(&args)
                }
            }
            "cxo" => {
                if args.is_empty() {
                    2
                } else {
                    (runner.cmd_cxo)(&args)
                }
            }
            _ => {
                let prompt = if task.context_ref.trim().is_empty() {
                    format!(
                        "Task Objective:\n{}\n\nRespond with concise execution notes and next actions.",
                        task.objective
                    )
                } else {
                    format!(
                        "Task Objective:\n{}\n\nContext Ref:\n{}\n\nRespond with concise execution notes and next actions.",
                        task.objective, task.context_ref
                    )
                };
                let res = (runner.execute_task)(TaskSpec {
                    command_name: "cxtask_run".to_string(),
                    input: TaskInput::Prompt(prompt),
                    output_kind: LlmOutputKind::AgentText,
                    schema: None,
                    schema_task_input: None,
                    logging_enabled: true,
                    capture_override: None,
                })?;
                println!("{}", res.stdout);
                return Ok((0, Some(res.execution_id)));
            }
        };
        return Ok((status, None));
    }

    let prompt = format!(
        "Task Objective:\n{}\n\nRespond with concise execution notes and next actions.",
        task.objective
    );
    let res = (runner.execute_task)(TaskSpec {
        command_name: "cxtask_run".to_string(),
        input: TaskInput::Prompt(prompt),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    })?;
    println!("{}", res.stdout);
    Ok((0, Some(res.execution_id)))
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
    let _ = (runner.set_state_path)("runtime.current_task_id", Value::String(id.to_string()));
    let _ = (runner.set_state_path)(
        "runtime.current_task_parent_id",
        match tasks[idx].parent_id.as_ref() {
            Some(v) => Value::String(v.clone()),
            None => Value::Null,
        },
    );

    let prev_mode = env::var("CX_MODE").ok();
    let prev_backend = env::var("CX_LLM_BACKEND").ok();
    if let Some(m) = mode_override {
        // SAFETY: cx task run/run-all are sequential command paths; overrides are restored before return.
        unsafe { env::set_var("CX_MODE", m) };
    }
    if let Some(b) = backend_override {
        // SAFETY: cx task run/run-all are sequential command paths; overrides are restored before return.
        unsafe { env::set_var("CX_LLM_BACKEND", b) };
    }

    let exec = run_task_objective(runner, &tasks[idx]);

    match prev_mode {
        Some(v) => {
            // SAFETY: restoring process env after scoped override.
            unsafe { env::set_var("CX_MODE", v) }
        }
        None => {
            // SAFETY: restoring process env after scoped override.
            unsafe { env::remove_var("CX_MODE") }
        }
    }
    match prev_backend {
        Some(v) => {
            // SAFETY: restoring process env after scoped override.
            unsafe { env::set_var("CX_LLM_BACKEND", v) }
        }
        None => {
            // SAFETY: restoring process env after scoped override.
            unsafe { env::remove_var("CX_LLM_BACKEND") }
        }
    }
    let _ = (runner.set_state_path)(
        "runtime.current_task_id",
        match prev_task_id {
            Some(v) => Value::String(v),
            None => Value::Null,
        },
    );
    let _ = (runner.set_state_path)(
        "runtime.current_task_parent_id",
        match prev_parent_id {
            Some(v) => Value::String(v),
            None => Value::Null,
        },
    );

    let (status_code, execution_id, objective_err) = match exec {
        Ok((c, eid)) => (c, eid, None),
        Err(e) => (1, None, Some(e)),
    };

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
    if let Some(e) = objective_err {
        eprintln!("cxrs task run: objective failed for {id}: {e}");
    }
    Ok((status_code, execution_id))
}


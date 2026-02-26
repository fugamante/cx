use serde_json::Value;

use crate::cmdctx::CmdCtx;
use crate::state::{current_task_id, set_state_path};
use crate::taskrun::{TaskRunError, TaskRunner};
use crate::tasks::set_task_status;
use crate::types::TaskRecord;

pub struct TaskCmdDeps {
    pub cmd_task_add: fn(&str, &[String]) -> i32,
    pub cmd_task_list: fn(Option<&str>) -> i32,
    pub cmd_task_show: fn(&str) -> i32,
    pub cmd_task_fanout: fn(&str, &str, Option<&str>) -> i32,
    pub read_tasks: fn() -> Result<Vec<TaskRecord>, String>,
    pub run_task_by_id: fn(
        &TaskRunner,
        &str,
        Option<&str>,
        Option<&str>,
    ) -> Result<(i32, Option<String>), TaskRunError>,
    pub make_task_runner: fn() -> TaskRunner,
}

pub fn cmd_task_set_status(id: &str, new_status: &str) -> i32 {
    if let Err(e) = set_task_status(id, new_status) {
        crate::cx_eprintln!("cxrs task: {e}");
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

fn parse_status_filter(args: &[String], usage: &str, prefix: &str) -> Result<String, i32> {
    let mut status_filter = "pending".to_string();
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                    crate::cx_eprintln!("{prefix}: invalid status '{v}'");
                    return Err(2);
                }
                status_filter = v.to_string();
                i += 2;
            }
            other => {
                crate::cx_eprintln!("{prefix}: unknown flag '{other}'");
                return Err(2);
            }
        }
    }
    Ok(status_filter)
}

fn handle_list(app_name: &str, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let usage =
        format!("Usage: {app_name} task list [--status pending|in_progress|complete|failed]");
    let mut status_filter: Option<&str> = None;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return 2;
                };
                if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                    crate::cx_eprintln!("cxrs task list: invalid status '{v}'");
                    return 2;
                }
                status_filter = Some(v);
                i += 2;
            }
            other => {
                crate::cx_eprintln!("cxrs task list: unknown flag '{other}'");
                return 2;
            }
        }
    }
    (deps.cmd_task_list)(status_filter)
}

fn require_id(app_name: &str, args: &[String], cmd: &str) -> Result<String, i32> {
    args.get(1).cloned().ok_or_else(|| {
        crate::cx_eprintln!("Usage: {app_name} task {cmd} <id>");
        2
    })
}

fn handle_fanout(app_name: &str, args: &[String], deps: &TaskCmdDeps) -> i32 {
    if args.len() < 2 {
        crate::cx_eprintln!("Usage: {app_name} task fanout <objective>");
        return 2;
    }
    let mut objective_parts: Vec<String> = Vec::new();
    let mut from: Option<&str> = None;
    let mut i = 1usize;
    while i < args.len() {
        if args[i] == "--from" {
            let Some(v) = args.get(i + 1).map(String::as_str) else {
                crate::cx_eprintln!(
                    "Usage: {app_name} task fanout <objective> [--from staged-diff|worktree|log|file:PATH]"
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
    (deps.cmd_task_fanout)(app_name, &objective_parts.join(" "), from)
}

fn parse_task_run_overrides(
    app_name: &str,
    args: &[String],
) -> Result<(Option<String>, Option<String>), i32> {
    let usage = format!(
        "Usage: {app_name} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
    );
    let mut mode_override: Option<String> = None;
    let mut backend_override: Option<String> = None;
    let mut i = 2usize;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                mode_override = Some(v.to_string());
                i += 2;
            }
            "--backend" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                backend_override = Some(v.to_string());
                i += 2;
            }
            other => {
                crate::cx_eprintln!("cxrs task run: unknown flag '{other}'");
                return Err(2);
            }
        }
    }
    Ok((mode_override, backend_override))
}

fn handle_run(app_name: &str, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let Some(id) = args.get(1).cloned() else {
        crate::cx_eprintln!(
            "Usage: {app_name} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
        );
        return 2;
    };
    let (mode_override, backend_override) = match parse_task_run_overrides(app_name, args) {
        Ok(v) => v,
        Err(code) => return code,
    };

    match (deps.run_task_by_id)(
        &(deps.make_task_runner)(),
        &id,
        mode_override.as_deref(),
        backend_override.as_deref(),
    ) {
        Ok((code, execution_id)) => {
            if let Some(eid) = execution_id {
                println!("task_id: {id}");
                println!("execution_id: {eid}");
            }
            println!("{id}: {}", if code == 0 { "complete" } else { "failed" });
            code
        }
        Err(e) => {
            crate::cx_eprintln!("{e}");
            1
        }
    }
}

fn handle_run_all(app_name: &str, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let usage =
        format!("Usage: {app_name} task run-all [--status pending|in_progress|complete|failed]");
    let status_filter = match parse_status_filter(args, &usage, "cxrs task run-all") {
        Ok(v) => v,
        Err(code) => return code,
    };

    let tasks = match (deps.read_tasks)() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
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
        match (deps.run_task_by_id)(&(deps.make_task_runner)(), &id, None, None) {
            Ok((code, _)) => {
                if code == 0 {
                    ok += 1;
                } else {
                    failed += 1;
                    crate::cx_eprintln!("cxrs task run-all: task failed: {id}");
                }
            }
            Err(e) => {
                crate::cx_eprintln!("cxrs task run-all: critical error for {id}: {e}");
                return 1;
            }
        }
    }
    println!("run-all summary: complete={ok}, failed={failed}");
    if failed > 0 { 1 } else { 0 }
}

pub fn handler(ctx: &CmdCtx, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let app_name = ctx.app_name;
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "add" => (deps.cmd_task_add)(app_name, &args[1..]),
        "list" => handle_list(app_name, args, deps),
        "show" => match require_id(app_name, args, "show") {
            Ok(id) => (deps.cmd_task_show)(&id),
            Err(code) => code,
        },
        "claim" => match require_id(app_name, args, "claim") {
            Ok(id) => cmd_task_set_status(&id, "in_progress"),
            Err(code) => code,
        },
        "complete" => match require_id(app_name, args, "complete") {
            Ok(id) => cmd_task_set_status(&id, "complete"),
            Err(code) => code,
        },
        "fail" => match require_id(app_name, args, "fail") {
            Ok(id) => cmd_task_set_status(&id, "failed"),
            Err(code) => code,
        },
        "fanout" => handle_fanout(app_name, args, deps),
        "run" => handle_run(app_name, args, deps),
        "run-all" => handle_run_all(app_name, args, deps),
        _ => {
            crate::cx_eprintln!(
                "Usage: {app_name} task <add|list|show|claim|complete|fail|fanout|run|run-all> ..."
            );
            2
        }
    }
}

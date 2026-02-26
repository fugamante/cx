use serde_json::Value;

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

pub fn cmd_task(app_name: &str, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "add" => (deps.cmd_task_add)(app_name, &args[1..]),
        "list" => {
            let mut status_filter: Option<&str> = None;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--status" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {app_name} task list [--status pending|in_progress|complete|failed]"
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
            (deps.cmd_task_list)(status_filter)
        }
        "show" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {app_name} task show <id>");
                return 2;
            };
            (deps.cmd_task_show)(id)
        }
        "claim" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {app_name} task claim <id>");
                return 2;
            };
            cmd_task_set_status(id, "in_progress")
        }
        "complete" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {app_name} task complete <id>");
                return 2;
            };
            cmd_task_set_status(id, "complete")
        }
        "fail" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {app_name} task fail <id>");
                return 2;
            };
            cmd_task_set_status(id, "failed")
        }
        "fanout" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} task fanout <objective>");
                return 2;
            }
            let mut objective_parts: Vec<String> = Vec::new();
            let mut from: Option<&str> = None;
            let mut i = 1usize;
            while i < args.len() {
                if args[i] == "--from" {
                    let Some(v) = args.get(i + 1).map(String::as_str) else {
                        eprintln!(
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
        "run" => {
            let Some(id) = args.get(1) else {
                eprintln!(
                    "Usage: {app_name} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
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
                                "Usage: {app_name} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
                            );
                            return 2;
                        };
                        mode_override = Some(v);
                        i += 2;
                    }
                    "--backend" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {app_name} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
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
            match (deps.run_task_by_id)(
                &(deps.make_task_runner)(),
                id,
                mode_override,
                backend_override,
            ) {
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
                                "Usage: {app_name} task run-all [--status pending|in_progress|complete|failed]"
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
            let tasks = match (deps.read_tasks)() {
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
                match (deps.run_task_by_id)(&(deps.make_task_runner)(), &id, None, None) {
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
                "Usage: {app_name} task <add|list|show|claim|complete|fail|fanout|run|run-all> ..."
            );
            2
        }
    }
}

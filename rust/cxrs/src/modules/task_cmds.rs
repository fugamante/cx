use serde_json::Value;
use std::collections::HashMap;

use crate::cmdctx::CmdCtx;
use crate::config::app_config;
use crate::state::{current_task_id, set_state_path};
use crate::taskrun::{TaskRunError, TaskRunner};
use crate::tasks::set_task_status;
use crate::tasks_plan::build_task_run_plan;
use crate::types::TaskRecord;

pub struct TaskCmdDeps {
    pub cmd_task_add: fn(&str, &[String]) -> i32,
    pub cmd_task_list: fn(Option<&str>) -> i32,
    pub cmd_task_show: fn(&str) -> i32,
    pub cmd_task_fanout: fn(&str, &str, Option<&str>) -> i32,
    pub read_tasks: fn() -> Result<Vec<TaskRecord>, String>,
    pub run_task_by_id: TaskRunByIdFn,
    pub make_task_runner: fn() -> TaskRunner,
}

type TaskRunByIdFn = fn(
    &TaskRunner,
    &str,
    Option<&str>,
    Option<&str>,
) -> Result<(i32, Option<String>), TaskRunError>;

pub fn cmd_task_set_status(id: &str, new_status: &str) -> i32 {
    if let Err(e) = set_task_status(id, new_status) {
        crate::cx_eprintln!("cxrs task: {e}");
        return 1;
    }
    if new_status == "in_progress" {
        let _ = set_state_path("runtime.current_task_id", Value::String(id.to_string()));
    } else if matches!(new_status, "complete" | "failed")
        && current_task_id().as_deref() == Some(id)
    {
        let _ = set_state_path("runtime.current_task_id", Value::Null);
    }
    println!("{id}: {new_status}");
    0
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
    let options = match parse_run_all_options(app_name, args) {
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
    let selected_count = tasks
        .iter()
        .filter(|t| t.status == options.status_filter)
        .count();
    if selected_count == 0 {
        println!("No tasks matched status '{}'.", options.status_filter);
        return 0;
    }
    let task_index: HashMap<String, TaskRecord> =
        tasks.iter().map(|t| (t.id.clone(), t.clone())).collect();

    let schedule: Vec<String> = if options.run_mode == "mixed" {
        let plan = build_task_run_plan(&tasks, &options.status_filter);
        if !plan.blocked.is_empty() {
            crate::cx_eprintln!("cxrs task run-all: blocked tasks prevent full schedule:");
            for b in &plan.blocked {
                crate::cx_eprintln!(" - {}: {}", b.id, b.reason);
            }
        }
        let ids: Vec<String> = plan
            .waves
            .iter()
            .flat_map(|wave| wave.task_ids.iter().cloned())
            .collect();
        if ids.is_empty() {
            println!("No runnable tasks for status '{}'.", options.status_filter);
            return if plan.blocked.is_empty() { 0 } else { 1 };
        }
        let pool = options.backend_pool.join(",");
        let cap_notes = render_backend_caps(&options.backend_caps);
        println!(
            "run-all mode=mixed waves={} runnable={} backend_pool={} max_workers={} backend_caps={}",
            plan.waves.len(),
            ids.len(),
            pool,
            options.max_workers,
            cap_notes
        );
        for wave in &plan.waves {
            println!(
                "wave {} [{}] -> {}",
                wave.index,
                wave.mode,
                wave.task_ids.join(",")
            );
        }
        ids
    } else {
        tasks
            .iter()
            .filter(|t| t.status == options.status_filter)
            .map(|t| t.id.clone())
            .collect()
    };

    let mut ok = 0usize;
    let mut failed = 0usize;
    for (idx, id) in schedule.iter().enumerate() {
        let task = task_index.get(id);
        let backend_selected = choose_backend_for_task(task, &options.backend_pool, idx);
        match (deps.run_task_by_id)(
            &(deps.make_task_runner)(),
            id,
            None,
            backend_selected.as_deref(),
        ) {
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
    println!(
        "run-all summary: mode={}, complete={ok}, failed={failed}",
        options.run_mode
    );
    if failed > 0 { 1 } else { 0 }
}

#[derive(Debug, Clone)]
struct RunAllOptions {
    status_filter: String,
    run_mode: String,
    backend_pool: Vec<String>,
    backend_caps: HashMap<String, usize>,
    max_workers: usize,
}

fn normalize_backend(v: &str) -> Option<String> {
    let b = v.trim().to_lowercase();
    if matches!(b.as_str(), "codex" | "ollama") {
        Some(b)
    } else {
        None
    }
}

fn parse_backend_pool(raw: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = raw.split(',').filter_map(normalize_backend).collect();
    out.sort();
    out.dedup();
    if out.is_empty() {
        return Err("cxrs task run-all: --backend-pool requires codex and/or ollama".to_string());
    }
    Ok(out)
}

fn parse_backend_cap(raw: &str) -> Result<(String, usize), String> {
    let mut parts = raw.splitn(2, '=');
    let Some(name_raw) = parts.next() else {
        return Err("cxrs task run-all: invalid --backend-cap".to_string());
    };
    let Some(limit_raw) = parts.next() else {
        return Err("cxrs task run-all: --backend-cap must use backend=limit".to_string());
    };
    let Some(name) = normalize_backend(name_raw) else {
        return Err(format!(
            "cxrs task run-all: invalid backend in cap '{name_raw}'"
        ));
    };
    let Ok(limit) = limit_raw.parse::<usize>() else {
        return Err(format!(
            "cxrs task run-all: invalid cap limit '{limit_raw}'"
        ));
    };
    if limit == 0 {
        return Err("cxrs task run-all: backend cap must be >= 1".to_string());
    }
    Ok((name, limit))
}

fn default_backend_pool() -> Vec<String> {
    let backend = app_config().llm_backend.to_lowercase();
    if matches!(backend.as_str(), "codex" | "ollama") {
        vec![backend]
    } else {
        vec!["codex".to_string()]
    }
}

fn parse_run_all_options(app_name: &str, args: &[String]) -> Result<RunAllOptions, i32> {
    let usage = format!(
        "Usage: {app_name} task run-all [--status pending|in_progress|complete|failed] [--mode sequential|mixed] [--backend-pool codex,ollama] [--backend-cap backend=limit] [--max-workers N]"
    );
    let mut status_filter = "pending".to_string();
    let mut run_mode = "sequential".to_string();
    let mut backend_pool = default_backend_pool();
    let mut backend_caps: HashMap<String, usize> = HashMap::new();
    let mut max_workers = 1usize;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                    crate::cx_eprintln!("cxrs task run-all: invalid status '{v}'");
                    return Err(2);
                }
                status_filter = v.to_string();
                i += 2;
            }
            "--mode" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                if !matches!(v, "sequential" | "mixed") {
                    crate::cx_eprintln!("cxrs task run-all: invalid mode '{v}'");
                    return Err(2);
                }
                run_mode = v.to_string();
                i += 2;
            }
            "--backend-pool" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                match parse_backend_pool(v) {
                    Ok(pool) => backend_pool = pool,
                    Err(e) => {
                        crate::cx_eprintln!("{e}");
                        return Err(2);
                    }
                }
                i += 2;
            }
            "--backend-cap" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                match parse_backend_cap(v) {
                    Ok((backend, cap)) => {
                        backend_caps.insert(backend, cap);
                    }
                    Err(e) => {
                        crate::cx_eprintln!("{e}");
                        return Err(2);
                    }
                }
                i += 2;
            }
            "--max-workers" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                let Ok(n) = v.parse::<usize>() else {
                    crate::cx_eprintln!("cxrs task run-all: --max-workers must be an integer");
                    return Err(2);
                };
                if n == 0 {
                    crate::cx_eprintln!("cxrs task run-all: --max-workers must be >= 1");
                    return Err(2);
                }
                max_workers = n;
                i += 2;
            }
            other => {
                crate::cx_eprintln!("cxrs task run-all: unknown flag '{other}'");
                return Err(2);
            }
        }
    }
    Ok(RunAllOptions {
        status_filter,
        run_mode,
        backend_pool,
        backend_caps,
        max_workers,
    })
}

fn choose_backend_for_task(
    task: Option<&TaskRecord>,
    pool: &[String],
    index: usize,
) -> Option<String> {
    if pool.is_empty() {
        return None;
    }
    if let Some(t) = task
        && let Some(task_backend) = normalize_backend(&t.backend)
        && pool.contains(&task_backend)
    {
        return Some(task_backend);
    }
    if pool.len() == 1 {
        return Some(pool[0].clone());
    }
    let policy = app_config().broker_policy.to_lowercase();
    match policy.as_str() {
        "quality" => {
            if pool.iter().any(|b| b == "codex") {
                Some("codex".to_string())
            } else {
                Some(pool[index % pool.len()].clone())
            }
        }
        "latency" | "cost" => {
            if pool.iter().any(|b| b == "ollama") {
                Some("ollama".to_string())
            } else {
                Some(pool[index % pool.len()].clone())
            }
        }
        _ => Some(pool[index % pool.len()].clone()),
    }
}

fn render_backend_caps(caps: &HashMap<String, usize>) -> String {
    if caps.is_empty() {
        return "none".to_string();
    }
    let mut kv: Vec<(String, usize)> = caps.iter().map(|(k, v)| (k.clone(), *v)).collect();
    kv.sort_by(|a, b| a.0.cmp(&b.0));
    kv.into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<String>>()
        .join(",")
}

fn handle_run_plan(app_name: &str, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let usage = format!(
        "Usage: {app_name} task run-plan [--status pending|in_progress|complete|failed] [--json]"
    );
    let mut status_filter = "pending".to_string();
    let mut as_json = false;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return 2;
                };
                if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                    crate::cx_eprintln!("cxrs task run-plan: invalid status '{v}'");
                    return 2;
                }
                status_filter = v.to_string();
                i += 2;
            }
            "--json" => {
                as_json = true;
                i += 1;
            }
            other => {
                crate::cx_eprintln!("cxrs task run-plan: unknown flag '{other}'");
                return 2;
            }
        }
    }

    let tasks = match (deps.read_tasks)() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            return 1;
        }
    };
    let plan = build_task_run_plan(&tasks, &status_filter);

    if as_json {
        match serde_json::to_string_pretty(&plan) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("cxrs task run-plan: failed to render json: {e}");
                return 1;
            }
        }
        return if plan.blocked.is_empty() { 0 } else { 1 };
    }

    println!("== cx task run-plan ==");
    println!("status_filter: {}", plan.status_filter);
    println!("selected: {}", plan.selected);
    println!("waves: {}", plan.waves.len());
    if plan.waves.is_empty() {
        println!("No tasks matched filter.");
    } else {
        println!("index | mode | task_ids");
        println!("---|---|---");
        for wave in &plan.waves {
            println!(
                "{} | {} | {}",
                wave.index,
                wave.mode,
                wave.task_ids.join(",")
            );
        }
    }
    if !plan.blocked.is_empty() {
        println!();
        println!("blocked: {}", plan.blocked.len());
        println!("id | reason");
        println!("---|---");
        for blocked in &plan.blocked {
            println!("{} | {}", blocked.id, blocked.reason);
        }
        return 1;
    }
    0
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
        "run-plan" => handle_run_plan(app_name, args, deps),
        "run" => handle_run(app_name, args, deps),
        "run-all" => handle_run_all(app_name, args, deps),
        _ => {
            crate::cx_eprintln!(
                "Usage: {app_name} task <add|list|show|claim|complete|fail|fanout|run-plan|run|run-all> ..."
            );
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_task(backend: &str) -> TaskRecord {
        TaskRecord {
            id: "task_001".to_string(),
            parent_id: None,
            role: "implementer".to_string(),
            objective: "noop".to_string(),
            context_ref: String::new(),
            backend: backend.to_string(),
            model: None,
            profile: "balanced".to_string(),
            converge: "none".to_string(),
            replicas: 1,
            max_concurrency: None,
            run_mode: "sequential".to_string(),
            depends_on: Vec::new(),
            resource_keys: Vec::new(),
            max_retries: None,
            timeout_secs: None,
            status: "pending".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn parse_run_all_options_accepts_backend_flags() {
        let args = vec![
            "run-all".to_string(),
            "--mode".to_string(),
            "mixed".to_string(),
            "--backend-pool".to_string(),
            "codex,ollama".to_string(),
            "--backend-cap".to_string(),
            "codex=2".to_string(),
            "--max-workers".to_string(),
            "3".to_string(),
        ];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert_eq!(opts.run_mode, "mixed");
        assert!(opts.backend_pool.iter().any(|b| b == "codex"));
        assert!(opts.backend_pool.iter().any(|b| b == "ollama"));
        assert_eq!(opts.backend_caps.get("codex"), Some(&2usize));
        assert_eq!(opts.max_workers, 3);
    }

    #[test]
    fn choose_backend_prefers_task_backend_when_in_pool() {
        let task = mk_task("ollama");
        let selected =
            choose_backend_for_task(Some(&task), &["codex".to_string(), "ollama".to_string()], 0);
        assert_eq!(selected.as_deref(), Some("ollama"));
    }
}

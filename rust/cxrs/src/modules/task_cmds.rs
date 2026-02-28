use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::cmdctx::CmdCtx;
use crate::config::app_config;
use crate::paths::resolve_log_file;
use crate::process::{run_command_output_with_timeout, run_command_status_with_timeout};
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
    bool,
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
) -> Result<(Option<String>, Option<String>, bool), i32> {
    let usage = format!(
        "Usage: {app_name} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
    );
    let mut mode_override: Option<String> = None;
    let mut backend_override: Option<String> = None;
    let mut managed_by_parent = false;
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
            "--managed-by-parent" => {
                managed_by_parent = true;
                i += 1;
            }
            other => {
                crate::cx_eprintln!("cxrs task run: unknown flag '{other}'");
                return Err(2);
            }
        }
    }
    Ok((mode_override, backend_override, managed_by_parent))
}

fn handle_run(app_name: &str, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let Some(id) = args.get(1).cloned() else {
        crate::cx_eprintln!(
            "Usage: {app_name} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
        );
        return 2;
    };
    let (mode_override, backend_override, managed_by_parent) =
        match parse_task_run_overrides(app_name, args) {
            Ok(v) => v,
            Err(code) => return code,
        };

    match (deps.run_task_by_id)(
        &(deps.make_task_runner)(),
        &id,
        mode_override.as_deref(),
        backend_override.as_deref(),
        managed_by_parent,
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

struct PendingLaunch {
    id: String,
    backend: String,
    queue_since: Instant,
}

struct ActiveLaunch {
    id: String,
    backend: String,
    join: thread::JoinHandle<Result<(i32, Option<String>), String>>,
}

#[derive(Debug, Clone, Copy)]
enum FailureClass {
    Retryable,
    NonRetryable,
    Blocked,
}

#[derive(Debug, Clone)]
struct FailureInfo {
    class: FailureClass,
    reason: String,
}

#[derive(Debug, Clone, Default)]
struct RunAllSummary {
    ok: usize,
    failed: usize,
    retryable_failed: usize,
    non_retryable_failed: usize,
    blocked: usize,
}

impl RunAllSummary {
    fn record_success(&mut self) {
        self.ok += 1;
    }

    fn record_failure(&mut self, class: FailureClass) {
        self.failed += 1;
        match class {
            FailureClass::Retryable => self.retryable_failed += 1,
            FailureClass::NonRetryable => self.non_retryable_failed += 1,
            FailureClass::Blocked => self.blocked += 1,
        }
    }
}

fn classify_failure_for_execution(execution_id: Option<&str>) -> FailureInfo {
    let Some(exec_id) = execution_id else {
        return FailureInfo {
            class: FailureClass::NonRetryable,
            reason: "missing_execution_id".to_string(),
        };
    };
    let Some(log_file) = resolve_log_file() else {
        return FailureInfo {
            class: FailureClass::NonRetryable,
            reason: "missing_log_file".to_string(),
        };
    };
    let Ok(content) = fs::read_to_string(log_file) else {
        return FailureInfo {
            class: FailureClass::NonRetryable,
            reason: "unreadable_log_file".to_string(),
        };
    };
    for line in content.lines().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("execution_id").and_then(Value::as_str) != Some(exec_id) {
            continue;
        }
        if v.get("policy_blocked").and_then(Value::as_bool) == Some(true) {
            let reason = v
                .get("policy_reason")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "policy_blocked".to_string());
            return FailureInfo {
                class: FailureClass::Blocked,
                reason,
            };
        }
        if v.get("timed_out").and_then(Value::as_bool) == Some(true) {
            return FailureInfo {
                class: FailureClass::Retryable,
                reason: "timed_out".to_string(),
            };
        }
        return FailureInfo {
            class: FailureClass::NonRetryable,
            reason: "non_retryable_failure".to_string(),
        };
    }
    FailureInfo {
        class: FailureClass::NonRetryable,
        reason: "execution_not_found_in_log".to_string(),
    }
}

fn parse_execution_id(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("execution_id: ")
                .map(|v| v.trim().to_string())
        })
        .filter(|s| !s.is_empty())
}

fn retry_backoff_ms(retry_index: u32) -> u64 {
    let power = retry_index.min(4);
    250u64.saturating_mul(1u64 << power).min(2000)
}

fn with_retry_env<F, T>(
    attempt: u32,
    retry_max: u32,
    retry_reason: Option<&str>,
    retry_backoff_ms: Option<u64>,
    f: F,
) -> T
where
    F: FnOnce() -> T,
{
    let prev_attempt = env::var("CX_TASK_RETRY_ATTEMPT").ok();
    let prev_max = env::var("CX_TASK_RETRY_MAX").ok();
    let prev_reason = env::var("CX_TASK_RETRY_REASON").ok();
    let prev_backoff = env::var("CX_TASK_RETRY_BACKOFF_MS").ok();
    unsafe {
        env::set_var("CX_TASK_RETRY_ATTEMPT", attempt.to_string());
        env::set_var("CX_TASK_RETRY_MAX", retry_max.to_string());
    }
    match retry_reason {
        Some(v) if !v.trim().is_empty() => unsafe { env::set_var("CX_TASK_RETRY_REASON", v) },
        _ => unsafe { env::remove_var("CX_TASK_RETRY_REASON") },
    }
    match retry_backoff_ms {
        Some(v) => unsafe { env::set_var("CX_TASK_RETRY_BACKOFF_MS", v.to_string()) },
        None => unsafe { env::remove_var("CX_TASK_RETRY_BACKOFF_MS") },
    }
    let out = f();
    match prev_attempt {
        Some(v) => unsafe { env::set_var("CX_TASK_RETRY_ATTEMPT", v) },
        None => unsafe { env::remove_var("CX_TASK_RETRY_ATTEMPT") },
    }
    match prev_max {
        Some(v) => unsafe { env::set_var("CX_TASK_RETRY_MAX", v) },
        None => unsafe { env::remove_var("CX_TASK_RETRY_MAX") },
    }
    match prev_reason {
        Some(v) => unsafe { env::set_var("CX_TASK_RETRY_REASON", v) },
        None => unsafe { env::remove_var("CX_TASK_RETRY_REASON") },
    }
    match prev_backoff {
        Some(v) => unsafe { env::set_var("CX_TASK_RETRY_BACKOFF_MS", v) },
        None => unsafe { env::remove_var("CX_TASK_RETRY_BACKOFF_MS") },
    }
    out
}

fn should_retry(failure: FailureClass, attempt: u32, retry_max: u32) -> bool {
    matches!(failure, FailureClass::Retryable) && attempt <= retry_max
}

fn run_task_managed_subprocess(
    id: String,
    backend: String,
    queue_ms: u64,
    worker_id: String,
    task_parent_id: Option<String>,
    max_retries: u32,
) -> Result<(i32, Option<String>), String> {
    let mut retry_reason: Option<String> = None;
    let mut retry_backoff: Option<u64> = None;
    for attempt in 1..=(max_retries + 1) {
        let exe = std::env::current_exe().map_err(|e| format!("task run-all: current_exe: {e}"))?;
        let mut cmd = Command::new(exe);
        cmd.args(["task", "run", &id, "--managed-by-parent"]);
        cmd.args(["--backend", &backend]);
        cmd.env("CX_TASK_ID", &id);
        cmd.env("CX_TASK_QUEUE_MS", queue_ms.to_string());
        cmd.env("CX_TASK_WORKER_ID", &worker_id);
        cmd.env("CX_TASK_RETRY_ATTEMPT", attempt.to_string());
        cmd.env("CX_TASK_RETRY_MAX", max_retries.to_string());
        if let Some(reason) = retry_reason.as_deref() {
            cmd.env("CX_TASK_RETRY_REASON", reason);
        } else {
            cmd.env_remove("CX_TASK_RETRY_REASON");
        }
        if let Some(backoff_ms) = retry_backoff {
            cmd.env("CX_TASK_RETRY_BACKOFF_MS", backoff_ms.to_string());
        } else {
            cmd.env_remove("CX_TASK_RETRY_BACKOFF_MS");
        }
        if let Some(parent_id) = task_parent_id.as_deref() {
            cmd.env("CX_TASK_PARENT_ID", parent_id);
        }
        let output = run_command_output_with_timeout(cmd, "task run-all worker")?;
        let status = output.status.code().unwrap_or(1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let execution_id = parse_execution_id(&stdout);
        if status == 0 {
            return Ok((status, execution_id));
        }
        let failure = classify_failure_for_execution(execution_id.as_deref());
        if should_retry(failure.class, attempt, max_retries) {
            let next_backoff = retry_backoff_ms(attempt - 1);
            retry_reason = Some(failure.reason);
            retry_backoff = Some(next_backoff);
            thread::sleep(Duration::from_millis(next_backoff));
            continue;
        }
        return Ok((status, execution_id));
    }
    Ok((1, None))
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
            "run-all mode=mixed waves={} runnable={} backend_pool={} max_workers={} backend_caps={} fairness={}",
            plan.waves.len(),
            ids.len(),
            pool,
            options.max_workers,
            cap_notes,
            options.fairness
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

    let summary = if options.run_mode == "mixed" && options.max_workers > 1 {
        match run_schedule_parallel(&schedule, &task_index, &options) {
            Ok(v) => v,
            Err(e) => {
                crate::cx_eprintln!("cxrs task run-all: {e}");
                return 1;
            }
        }
    } else {
        let mut summary = RunAllSummary::default();
        for (idx, id) in schedule.iter().enumerate() {
            let task = task_index.get(id);
            let max_retries = task.and_then(|t| t.max_retries).unwrap_or(0);
            let backend_selected = fallback_backend(
                choose_backend_for_task(task, &options.backend_pool, idx),
                &available_pool(&options.backend_pool),
            );
            let mut retry_reason: Option<String> = None;
            let mut retry_backoff: Option<u64> = None;
            let mut finished = false;
            for attempt in 1..=(max_retries + 1) {
                let run_result = with_retry_env(
                    attempt,
                    max_retries,
                    retry_reason.as_deref(),
                    retry_backoff,
                    || {
                        (deps.run_task_by_id)(
                            &(deps.make_task_runner)(),
                            id,
                            None,
                            backend_selected.as_deref(),
                            false,
                        )
                    },
                );
                match run_result {
                    Ok((code, execution_id)) => {
                        if code == 0 {
                            summary.record_success();
                            finished = true;
                            break;
                        }
                        let failure = classify_failure_for_execution(execution_id.as_deref());
                        if should_retry(failure.class, attempt, max_retries) {
                            let next_backoff = retry_backoff_ms(attempt - 1);
                            retry_reason = Some(failure.reason);
                            retry_backoff = Some(next_backoff);
                            thread::sleep(Duration::from_millis(next_backoff));
                            continue;
                        }
                        summary.record_failure(failure.class);
                        crate::cx_eprintln!("cxrs task run-all: task failed: {id}");
                        finished = true;
                        break;
                    }
                    Err(e) => {
                        crate::cx_eprintln!("cxrs task run-all: critical error for {id}: {e}");
                        return 1;
                    }
                }
            }
            if !finished {
                summary.record_failure(FailureClass::NonRetryable);
                crate::cx_eprintln!("cxrs task run-all: task failed: {id}");
            }
        }
        summary
    };
    println!(
        "run-all summary: mode={}, complete={}, failed={}, blocked={}, retryable_failures={}, non_retryable_failures={}",
        options.run_mode,
        summary.ok,
        summary.failed,
        summary.blocked,
        summary.retryable_failed,
        summary.non_retryable_failed
    );
    if summary.failed > 0 { 1 } else { 0 }
}

#[derive(Debug, Clone)]
struct RunAllOptions {
    status_filter: String,
    run_mode: String,
    backend_pool: Vec<String>,
    backend_caps: HashMap<String, usize>,
    max_workers: usize,
    fairness: String,
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

fn backend_available(name: &str) -> bool {
    let disabled = match name {
        "codex" => std::env::var("CX_DISABLE_CODEX")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        "ollama" => std::env::var("CX_DISABLE_OLLAMA")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        _ => false,
    };
    if disabled {
        return false;
    }
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", &format!("command -v {name} >/dev/null 2>&1")]);
    run_command_status_with_timeout(cmd, "command -v")
        .ok()
        .is_some_and(|s| s.success())
}

fn available_pool(pool: &[String]) -> Vec<String> {
    pool.iter()
        .filter(|b| backend_available(b))
        .cloned()
        .collect::<Vec<String>>()
}

fn fallback_backend(selected: Option<String>, available: &[String]) -> Option<String> {
    if available.is_empty() {
        return None;
    }
    if let Some(s) = selected
        && available.contains(&s)
    {
        return Some(s);
    }
    Some(available[0].clone())
}

fn parse_run_all_options(app_name: &str, args: &[String]) -> Result<RunAllOptions, i32> {
    let usage = format!(
        "Usage: {app_name} task run-all [--status pending|in_progress|complete|failed] [--mode sequential|mixed] [--backend-pool codex,ollama] [--backend-cap backend=limit] [--max-workers N] [--fairness round_robin|least_loaded]"
    );
    let mut status_filter = "pending".to_string();
    let mut run_mode = "sequential".to_string();
    let mut backend_pool = default_backend_pool();
    let mut backend_caps: HashMap<String, usize> = HashMap::new();
    let mut max_workers = 1usize;
    let mut fairness = "round_robin".to_string();
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
            "--fairness" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                let fv = v.trim().to_lowercase();
                if !matches!(fv.as_str(), "round_robin" | "least_loaded") {
                    crate::cx_eprintln!("cxrs task run-all: invalid fairness '{fv}'");
                    return Err(2);
                }
                fairness = fv;
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
        fairness,
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

fn backend_cap_for(options: &RunAllOptions, backend: &str) -> usize {
    options
        .backend_caps
        .get(backend)
        .copied()
        .unwrap_or(options.max_workers)
        .max(1)
}

fn set_task_status_quiet(id: &str, status: &str) -> Result<(), String> {
    set_task_status(id, status)
}

fn run_schedule_parallel(
    schedule: &[String],
    task_index: &HashMap<String, TaskRecord>,
    options: &RunAllOptions,
) -> Result<RunAllSummary, String> {
    let available = available_pool(&options.backend_pool);
    if available.is_empty() {
        return Err("task run-all: no available backend from --backend-pool".to_string());
    }
    let mut pending: Vec<PendingLaunch> = schedule
        .iter()
        .enumerate()
        .map(|(idx, id)| PendingLaunch {
            id: id.clone(),
            backend: fallback_backend(
                choose_backend_for_task(task_index.get(id), &options.backend_pool, idx),
                &available,
            )
            .unwrap_or_else(|| available[0].clone()),
            queue_since: Instant::now(),
        })
        .collect();
    let mut active: Vec<ActiveLaunch> = Vec::new();
    let mut backend_active: HashMap<String, usize> = HashMap::new();
    let mut summary = RunAllSummary::default();
    let mut next_worker = 1usize;

    while !pending.is_empty() || !active.is_empty() {
        while active.len() < options.max_workers && !pending.is_empty() {
            let maybe_idx = if options.fairness == "least_loaded" {
                pending
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        let cur = backend_active.get(&p.backend).copied().unwrap_or(0);
                        cur < backend_cap_for(options, &p.backend)
                    })
                    .min_by_key(|(_, p)| backend_active.get(&p.backend).copied().unwrap_or(0))
                    .map(|(idx, _)| idx)
            } else {
                pending.iter().position(|p| {
                    let cur = backend_active.get(&p.backend).copied().unwrap_or(0);
                    cur < backend_cap_for(options, &p.backend)
                })
            };
            let Some(pos) = maybe_idx else {
                break;
            };
            let launch = pending.remove(pos);
            set_task_status_quiet(&launch.id, "in_progress")?;
            let queue_ms = launch.queue_since.elapsed().as_millis() as u64;
            let worker_id = format!("w{next_worker}");
            next_worker = if next_worker >= options.max_workers {
                1
            } else {
                next_worker + 1
            };
            *backend_active.entry(launch.backend.clone()).or_insert(0) += 1;
            let id = launch.id.clone();
            let backend = launch.backend.clone();
            let task_parent_id = task_index.get(&id).and_then(|t| t.parent_id.clone());
            let max_retries = task_index.get(&id).and_then(|t| t.max_retries).unwrap_or(0);
            let join = thread::spawn(move || {
                run_task_managed_subprocess(
                    id,
                    backend,
                    queue_ms,
                    worker_id,
                    task_parent_id,
                    max_retries,
                )
            });
            active.push(ActiveLaunch {
                id: launch.id,
                backend: launch.backend,
                join,
            });
        }

        if active.is_empty() && !pending.is_empty() {
            return Err("task run-all: scheduler deadlock (backend caps too strict)".to_string());
        }

        if !active.is_empty() {
            let done = active.remove(0);
            let join_out = done
                .join
                .join()
                .map_err(|_| format!("task run-all: worker thread panicked for {}", done.id))?;
            if let Some(v) = backend_active.get_mut(&done.backend)
                && *v > 0
            {
                *v -= 1;
            }
            match join_out {
                Ok((code, execution_id)) => {
                    if code == 0 {
                        summary.record_success();
                        let _ = set_task_status_quiet(&done.id, "complete");
                    } else {
                        summary.record_failure(
                            classify_failure_for_execution(execution_id.as_deref()).class,
                        );
                        let _ = set_task_status_quiet(&done.id, "failed");
                        crate::cx_eprintln!("cxrs task run-all: task failed: {}", done.id);
                    }
                }
                Err(e) => {
                    summary.record_failure(FailureClass::NonRetryable);
                    let _ = set_task_status_quiet(&done.id, "failed");
                    crate::cx_eprintln!("cxrs task run-all: critical error for {}: {e}", done.id);
                }
            }
        }
    }

    Ok(summary)
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
            "--fairness".to_string(),
            "least_loaded".to_string(),
        ];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert_eq!(opts.run_mode, "mixed");
        assert!(opts.backend_pool.iter().any(|b| b == "codex"));
        assert!(opts.backend_pool.iter().any(|b| b == "ollama"));
        assert_eq!(opts.backend_caps.get("codex"), Some(&2usize));
        assert_eq!(opts.max_workers, 3);
        assert_eq!(opts.fairness, "least_loaded");
    }

    #[test]
    fn choose_backend_prefers_task_backend_when_in_pool() {
        let task = mk_task("ollama");
        let selected =
            choose_backend_for_task(Some(&task), &["codex".to_string(), "ollama".to_string()], 0);
        assert_eq!(selected.as_deref(), Some("ollama"));
    }

    #[test]
    fn retry_backoff_is_bounded() {
        assert_eq!(retry_backoff_ms(0), 250);
        assert_eq!(retry_backoff_ms(1), 500);
        assert_eq!(retry_backoff_ms(2), 1000);
        assert_eq!(retry_backoff_ms(3), 2000);
        assert_eq!(retry_backoff_ms(4), 2000);
        assert_eq!(retry_backoff_ms(10), 2000);
    }

    #[test]
    fn should_retry_only_retryable_and_within_budget() {
        assert!(should_retry(FailureClass::Retryable, 1, 2));
        assert!(should_retry(FailureClass::Retryable, 2, 2));
        assert!(!should_retry(FailureClass::Retryable, 3, 2));
        assert!(!should_retry(FailureClass::NonRetryable, 1, 2));
        assert!(!should_retry(FailureClass::Blocked, 1, 2));
    }
}

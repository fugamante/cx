use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::cmdctx::CmdCtx;
use crate::config::{app_config, cli_app_name, command_with_cli};
use crate::contract_versions::{
    TASK_CHECK_JSON_CONTRACT_VERSION, TASK_LIST_JSON_CONTRACT_VERSION,
    TASK_RUN_ALL_JSON_CONTRACT_VERSION, TASK_RUN_JSON_CONTRACT_VERSION,
    TASK_RUN_PLAN_JSON_CONTRACT_VERSION, TASK_SANDBOX_READINESS_JSON_CONTRACT_VERSION,
};
use crate::doctor::{
    exec_context_value, exec_recommendations_value, latest_wave_sum, next_cost_class,
    next_quality_risk, next_reasoning_required, reasoning_gate_value,
};
use crate::execmeta::utc_now_iso;
use crate::json_mode::resolve_json_mode;
use crate::logs::load_values;
use crate::paths::resolve_log_file;
use crate::process::{run_command_output_with_timeout, run_command_status_with_timeout};
use crate::state::{current_task_id, set_state_path};
use crate::task_events::{TaskEvent, cmd_task_events, emit as emit_task_event};
use crate::taskrun::{TaskRunError, TaskRunner, task_sandbox_config, task_sandbox_readiness};
use crate::tasks::set_task_status;
use crate::tasks::task_run_state;
use crate::tasks::task_run_view;
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
    bool,
) -> Result<(i32, Option<String>), TaskRunError>;

struct TaskRunOverrides {
    mode_override: Option<String>,
    backend_override: Option<String>,
    managed_by_parent: bool,
    json_out: Option<bool>,
}

pub fn cmd_task_set_status(id: &str, new_status: &str) -> i32 {
    if let Err(e) = set_task_status(id, new_status) {
        crate::cx_eprintln!("{} task: {e}", cli_app_name());
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
    let usage = format!(
        "Usage: {app_name} task list [--status pending|in_progress|complete|failed] [--json|--text]"
    );
    let mut status_filter: Option<&str> = None;
    let mut json_out: Option<bool> = None;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return 2;
                };
                if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                    crate::cx_eprintln!("{} task list: invalid status '{v}'", cli_app_name());
                    return 2;
                }
                status_filter = Some(v);
                i += 2;
            }
            "--json" => {
                json_out = Some(true);
                i += 1;
            }
            "--text" => {
                json_out = Some(false);
                i += 1;
            }
            other => {
                crate::cx_eprintln!("{} task list: unknown flag '{other}'", cli_app_name());
                return 2;
            }
        }
    }
    if resolve_json_mode(json_out, false) {
        let tasks = match (deps.read_tasks)() {
            Ok(v) => v,
            Err(e) => {
                crate::cx_eprintln!("{e}");
                return 1;
            }
        };
        let filtered: Vec<TaskRecord> = match status_filter {
            Some(s) => tasks.iter().filter(|t| t.status == s).cloned().collect(),
            None => tasks.clone(),
        };
        let plan = build_task_run_plan(&tasks, "pending");
        let list_readiness = list_readiness_value(&tasks, &filtered, status_filter);
        let task_rows: Vec<Value> = filtered
            .iter()
            .map(|task| {
                let mut value =
                    serde_json::to_value(task).unwrap_or_else(|_| serde_json::json!({}));
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "run_readiness".to_string(),
                        task_run_view(task, &tasks, &plan),
                    );
                }
                value
            })
            .collect();
        let payload = serde_json::json!({
            "contract_version": TASK_LIST_JSON_CONTRACT_VERSION,
            "status_filter": status_filter,
            "count": task_rows.len(),
            "list_readiness": list_readiness,
            "tasks": task_rows
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("{} task list: failed to render json: {e}", cli_app_name());
                return 1;
            }
        }
        0
    } else {
        (deps.cmd_task_list)(status_filter)
    }
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

fn handle_task_sandbox(app_name: &str, args: &[String]) -> i32 {
    let usage = format!(
        "Usage: {app_name} task sandbox <show|check|enable|disable|set-image|clear-image> [--json|--text|<image>]"
    );
    let sub = args.get(1).map(String::as_str).unwrap_or("show");
    match sub {
        "show" | "check" => {
            let mut as_json = false;
            for arg in args.iter().skip(2) {
                match arg.as_str() {
                    "--json" => as_json = true,
                    "--text" => as_json = false,
                    other => {
                        crate::cx_eprintln!(
                            "{} task sandbox: unknown flag '{other}'",
                            cli_app_name()
                        );
                        return 2;
                    }
                }
            }
            if sub == "show" {
                let cfg = task_sandbox_config();
                let payload = serde_json::json!({
                    "contract_version": "task-sandbox.v1",
                    "enabled": cfg.enabled,
                    "image": cfg.image,
                    "active": std::env::var("CX_TASK_SANDBOX_ACTIVE").ok().as_deref() == Some("1")
                });
                if as_json {
                    match serde_json::to_string_pretty(&payload) {
                        Ok(s) => println!("{s}"),
                        Err(e) => {
                            crate::cx_eprintln!(
                                "{} task sandbox: failed to render json: {e}",
                                cli_app_name()
                            );
                            return 1;
                        }
                    }
                } else {
                    println!("task_sandbox_enabled: {}", cfg.enabled);
                    println!(
                        "task_sandbox_image: {}",
                        cfg.image.unwrap_or_else(|| "-".to_string())
                    );
                    println!(
                        "task_sandbox_active: {}",
                        std::env::var("CX_TASK_SANDBOX_ACTIVE").ok().as_deref() == Some("1")
                    );
                }
                return 0;
            }

            let readiness = task_sandbox_readiness();
            let payload = serde_json::json!({
                "contract_version": TASK_SANDBOX_READINESS_JSON_CONTRACT_VERSION,
                "enabled": readiness.enabled,
                "active": readiness.active,
                "image": readiness.image,
                "ready": readiness.ready,
                "docker_available": readiness.docker_available,
                "image_available": readiness.image_available,
                "repo_mount_writable": readiness.repo_mount_writable,
                "entrypoint_available": readiness.entrypoint_available,
                "issues": readiness.issues,
                "recommended_action": readiness.recommended_action,
            });
            if as_json {
                match serde_json::to_string_pretty(&payload) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        crate::cx_eprintln!(
                            "{} task sandbox: failed to render json: {e}",
                            cli_app_name()
                        );
                        return 1;
                    }
                }
            } else {
                println!(
                    "task_sandbox_ready: {}",
                    payload
                        .get("ready")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                );
                println!(
                    "task_sandbox_active: {}",
                    payload
                        .get("active")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                );
                println!(
                    "task_sandbox_image: {}",
                    payload.get("image").and_then(Value::as_str).unwrap_or("-")
                );
                println!(
                    "docker_available: {}",
                    payload
                        .get("docker_available")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                );
                println!(
                    "image_available: {}",
                    payload
                        .get("image_available")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                );
                println!(
                    "repo_mount_writable: {}",
                    payload
                        .get("repo_mount_writable")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                );
                println!(
                    "entrypoint_available: {}",
                    payload
                        .get("entrypoint_available")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                );
                let issues = payload
                    .get("issues")
                    .and_then(Value::as_array)
                    .map(|rows| {
                        rows.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default();
                println!(
                    "issues: {}",
                    if issues.is_empty() {
                        "-".to_string()
                    } else {
                        issues
                    }
                );
                println!(
                    "recommended_action: {}",
                    payload
                        .get("recommended_action")
                        .and_then(Value::as_str)
                        .unwrap_or("-")
                );
            }
            if payload.get("ready").and_then(Value::as_bool) == Some(true) {
                0
            } else {
                1
            }
        }
        "enable" => {
            if let Err(e) = set_state_path("preferences.task_sandbox.enabled", Value::Bool(true)) {
                crate::cx_eprintln!("{} task sandbox: {e}", cli_app_name());
                return 1;
            }
            println!("task_sandbox_enabled: true");
            0
        }
        "disable" => {
            if let Err(e) = set_state_path("preferences.task_sandbox.enabled", Value::Bool(false)) {
                crate::cx_eprintln!("{} task sandbox: {e}", cli_app_name());
                return 1;
            }
            println!("task_sandbox_enabled: false");
            0
        }
        "set-image" => {
            let Some(image) = args.get(2).map(String::as_str) else {
                crate::cx_eprintln!("{usage}");
                return 2;
            };
            if let Err(e) = set_state_path(
                "preferences.task_sandbox.image",
                Value::String(image.to_string()),
            ) {
                crate::cx_eprintln!("{} task sandbox: {e}", cli_app_name());
                return 1;
            }
            println!("task_sandbox_image: {image}");
            0
        }
        "clear-image" => {
            if let Err(e) = set_state_path("preferences.task_sandbox.image", Value::Null) {
                crate::cx_eprintln!("{} task sandbox: {e}", cli_app_name());
                return 1;
            }
            println!("task_sandbox_image: -");
            0
        }
        _ => {
            crate::cx_eprintln!("{usage}");
            2
        }
    }
}

fn parse_task_run_overrides(app_name: &str, args: &[String]) -> Result<TaskRunOverrides, i32> {
    let usage = format!(
        "Usage: {app_name} task run <id> [--mode lean|deterministic|verbose] [--backend primary|ollama] [--json|--text]"
    );
    let mut mode_override: Option<String> = None;
    let mut backend_override: Option<String> = None;
    let mut managed_by_parent = false;
    let mut json_out: Option<bool> = None;
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
            "--json" => {
                json_out = Some(true);
                i += 1;
            }
            "--text" => {
                json_out = Some(false);
                i += 1;
            }
            other => {
                crate::cx_eprintln!("{} task run: unknown flag '{other}'", cli_app_name());
                return Err(2);
            }
        }
    }
    Ok(TaskRunOverrides {
        mode_override,
        backend_override,
        managed_by_parent,
        json_out,
    })
}

fn handle_run(app_name: &str, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let Some(id) = args.get(1).cloned() else {
        crate::cx_eprintln!(
            "Usage: {app_name} task run <id> [--mode lean|deterministic|verbose] [--backend primary|ollama] [--json|--text]"
        );
        return 2;
    };
    let overrides = match parse_task_run_overrides(app_name, args) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let TaskRunOverrides {
        mode_override,
        backend_override,
        managed_by_parent,
        json_out,
    } = overrides;
    let as_json = resolve_json_mode(json_out, false);

    let mut preflight: Option<Value> = None;

    if !managed_by_parent
        && let Ok(tasks) = (deps.read_tasks)()
        && let Some(task) = tasks.iter().find(|task| task.id == id)
    {
        let readiness = task_run_state(task, &tasks);
        preflight = Some(readiness.clone());
        if !as_json && readiness.get("runnable_now").and_then(Value::as_bool) != Some(true) {
            println!(
                "task_run_preflight: runnable_now={} wave_index={} wave_mode={}",
                readiness
                    .get("runnable_now")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                readiness
                    .get("wave_index")
                    .and_then(Value::as_u64)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                readiness
                    .get("wave_mode")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
            );
            if let Some(reason) = readiness.get("blocked_reason").and_then(Value::as_str) {
                println!("task_run_preflight_reason: {reason}");
            }
            if let Some(command) = readiness.get("recommended_command").and_then(Value::as_str) {
                println!("task_run_preflight_recommended: {command}");
            }
        }
    }

    match (deps.run_task_by_id)(
        &(deps.make_task_runner)(),
        &id,
        mode_override.as_deref(),
        backend_override.as_deref(),
        managed_by_parent,
        !as_json,
    ) {
        Ok((code, execution_id)) => {
            if as_json {
                let payload = serde_json::json!({
                    "contract_version": TASK_RUN_JSON_CONTRACT_VERSION,
                    "task_id": id,
                    "mode_override": mode_override,
                    "backend_override": backend_override,
                    "managed_by_parent": managed_by_parent,
                    "preflight": preflight,
                    "status": if code == 0 { "complete" } else { "failed" },
                    "execution_id": execution_id,
                });
                match serde_json::to_string_pretty(&payload) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        crate::cx_eprintln!(
                            "{} task run: failed to render json: {e}",
                            cli_app_name()
                        );
                        return 1;
                    }
                }
                return code;
            }
            if let Some(eid) = execution_id {
                println!("task_id: {id}");
                println!("execution_id: {eid}");
            }
            println!("{id}: {}", if code == 0 { "complete" } else { "failed" });
            code
        }
        Err(e) => {
            if as_json {
                let payload = serde_json::json!({
                    "contract_version": TASK_RUN_JSON_CONTRACT_VERSION,
                    "task_id": id,
                    "mode_override": mode_override,
                    "backend_override": backend_override,
                    "managed_by_parent": managed_by_parent,
                    "preflight": preflight,
                    "status": "error",
                    "execution_id": Value::Null,
                    "error": e.to_string(),
                });
                match serde_json::to_string_pretty(&payload) {
                    Ok(s) => println!("{s}"),
                    Err(err) => {
                        crate::cx_eprintln!(
                            "{} task run: failed to render json: {err}",
                            cli_app_name()
                        );
                    }
                }
                return 1;
            }
            crate::cx_eprintln!("{e}");
            1
        }
    }
}

struct PendingLaunch {
    id: String,
    backend: String,
    requested_backend: Option<String>,
    queue_since: Instant,
    queue_started_at: String,
    wave_index: u64,
    wave_mode: String,
    wave_size: u64,
}

struct ActiveLaunch {
    id: String,
    backend: String,
    requested_backend: Option<String>,
    queue_ms: u64,
    wave_index: u64,
    wave_mode: String,
    wave_size: u64,
    join: thread::JoinHandle<Result<(i32, Option<String>), String>>,
}

#[derive(Debug, Clone)]
struct TaskWaveMeta {
    index: u64,
    mode: String,
    size: u64,
}

struct LaunchEnvMeta {
    queue_ms: u64,
    queue_started_at: String,
    worker_id: String,
    task_parent_id: Option<String>,
    max_retries: u32,
    wave: Option<TaskWaveMeta>,
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
    critical_errors: usize,
    halted_on_critical: bool,
    task_runs: Vec<Value>,
}

#[derive(Debug, Clone)]
struct TaskRunEvent {
    id: String,
    backend: String,
    requested_backend: Option<String>,
    status: String,
    execution_id: Option<String>,
    failure_class: Option<String>,
    queue_ms: u64,
    wave_index: u64,
    wave_mode: String,
    wave_size: u64,
}

#[derive(Debug, Clone)]
struct RunAllWavePressure {
    kind: &'static str,
    suggested_mode: Option<&'static str>,
    latest_wave_index: Option<u64>,
    max_queue_wave_index: Option<u64>,
    max_queue_wave_ms: u64,
}

#[derive(Debug, Clone, Default)]
struct RunAllConcurrencySummary {
    worker_count: u64,
    workers: Vec<String>,
    max_retry_attempt: u32,
    first_queue_started_at: Option<String>,
    first_task_started_at: Option<String>,
    last_task_finished_at: Option<String>,
}

fn runall_invariants_value(
    scheduled: u64,
    summary: &RunAllSummary,
    halted_remaining: usize,
    halt_on_critical: bool,
    concurrency: &RunAllConcurrencySummary,
) -> Value {
    let complete = summary.ok as u64;
    let failed = summary.failed as u64;
    let blocked = summary.blocked as u64;
    let retryable_failures = summary.retryable_failed as u64;
    let non_retryable_failures = summary.non_retryable_failed as u64;
    let critical_errors = summary.critical_errors as u64;
    let halted_remaining = halted_remaining as u64;

    let outcome_accounting_ok = scheduled == complete + failed + halted_remaining;
    let failure_accounting_ok = failed == blocked + retryable_failures + non_retryable_failures;
    let critical_halt_ok =
        critical_errors <= non_retryable_failures && (halted_remaining == 0 || halt_on_critical);
    let worker_summary_ok = concurrency.worker_count == concurrency.workers.len() as u64
        && !(complete + failed > 0 && concurrency.worker_count == 0);

    let timing_window_ok = match (
        concurrency.first_queue_started_at.as_deref(),
        concurrency.first_task_started_at.as_deref(),
        concurrency.last_task_finished_at.as_deref(),
    ) {
        (None, None, None) => true,
        (Some(queue), Some(start), Some(finish)) => queue <= start && start <= finish,
        _ => false,
    };

    let mut issues = Vec::new();
    if !outcome_accounting_ok {
        issues.push("outcome_accounting_mismatch");
    }
    if !failure_accounting_ok {
        issues.push("failure_accounting_mismatch");
    }
    if !critical_halt_ok {
        issues.push("critical_halt_mismatch");
    }
    if !worker_summary_ok {
        issues.push("worker_summary_mismatch");
    }
    if !timing_window_ok {
        issues.push("timing_window_mismatch");
    }

    serde_json::json!({
        "ok": issues.is_empty(),
        "status": if issues.is_empty() { "clean" } else { "violated" },
        "issues": issues,
        "outcome_accounting_ok": outcome_accounting_ok,
        "failure_accounting_ok": failure_accounting_ok,
        "critical_halt_ok": critical_halt_ok,
        "worker_summary_ok": worker_summary_ok,
        "timing_window_ok": timing_window_ok
    })
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

    fn record_critical_error(&mut self) {
        self.failed += 1;
        self.non_retryable_failed += 1;
        self.critical_errors += 1;
    }

    fn add_task_run(&mut self, ev: TaskRunEvent) {
        let used_backend_fallback = ev
            .requested_backend
            .as_deref()
            .is_some_and(|requested| requested != ev.backend);
        self.task_runs.push(serde_json::json!({
            "task_id": ev.id,
            "backend": ev.backend,
            "requested_backend": ev.requested_backend,
            "used_backend_fallback": used_backend_fallback,
            "status": ev.status,
            "execution_id": ev.execution_id,
            "failure_class": ev.failure_class,
            "queue_ms": ev.queue_ms,
            "wave_index": ev.wave_index,
            "wave_mode": ev.wave_mode,
            "wave_size": ev.wave_size
        }));
    }
}

fn runall_wave_pressure(summary: &RunAllSummary, mode: &str) -> RunAllWavePressure {
    let mut latest_wave_index: Option<u64> = None;
    let mut max_queue_wave_index: Option<u64> = None;
    let mut max_queue_wave_ms = 0u64;
    for task in &summary.task_runs {
        let wave_index = task.get("wave_index").and_then(Value::as_u64);
        if wave_index.is_some() {
            latest_wave_index = wave_index;
        }
        let queue_ms = task.get("queue_ms").and_then(Value::as_u64).unwrap_or(0);
        if queue_ms >= max_queue_wave_ms {
            max_queue_wave_ms = queue_ms;
            max_queue_wave_index = wave_index;
        }
    }
    if max_queue_wave_index.unwrap_or(0) > 1 && max_queue_wave_ms >= 2000 {
        let suggested_mode = match mode {
            "parallel" => Some("mixed"),
            "mixed" => Some("sequential"),
            _ => None,
        };
        return RunAllWavePressure {
            kind: "later_wave_queue",
            suggested_mode,
            latest_wave_index,
            max_queue_wave_index,
            max_queue_wave_ms,
        };
    }
    RunAllWavePressure {
        kind: "none",
        suggested_mode: None,
        latest_wave_index,
        max_queue_wave_index,
        max_queue_wave_ms,
    }
}

fn runall_reason_counts(summary: &RunAllSummary) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for task in &summary.task_runs {
        let status = task.get("status").and_then(Value::as_str).unwrap_or("");
        if status != "failed" && status != "critical_error" {
            continue;
        }
        let reason = task
            .get("failure_class")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        *counts.entry(reason.to_string()).or_insert(0) += 1;
    }
    counts
}

fn runall_failed_ids(summary: &RunAllSummary, limit: usize) -> Vec<String> {
    summary
        .task_runs
        .iter()
        .filter(|t| {
            matches!(
                t.get("status").and_then(Value::as_str),
                Some("failed" | "critical_error")
            )
        })
        .filter_map(|t| {
            t.get("task_id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .take(limit)
        .collect()
}

fn runall_invocation_command(options: &RunAllOptions) -> String {
    let mut parts = vec![
        command_with_cli("task run-all"),
        format!("--status {}", options.status_filter),
        format!("--mode {}", options.run_mode),
    ];
    if options.strict_plan {
        parts.push("--strict-plan".to_string());
    }
    parts.join(" ")
}

fn min_ts_slot(slot: &mut Option<String>, candidate: Option<&str>) {
    let Some(candidate) = candidate.map(str::trim).filter(|v| !v.is_empty()) else {
        return;
    };
    match slot {
        Some(current) if current.as_str() <= candidate => {}
        _ => *slot = Some(candidate.to_string()),
    }
}

fn max_ts_slot(slot: &mut Option<String>, candidate: Option<&str>) {
    let Some(candidate) = candidate.map(str::trim).filter(|v| !v.is_empty()) else {
        return;
    };
    match slot {
        Some(current) if current.as_str() >= candidate => {}
        _ => *slot = Some(candidate.to_string()),
    }
}

fn runall_concurrency_summary(summary: &RunAllSummary) -> RunAllConcurrencySummary {
    let execution_ids: HashSet<String> = summary
        .task_runs
        .iter()
        .filter_map(|task| {
            task.get("execution_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        })
        .collect();
    if execution_ids.is_empty() {
        return RunAllConcurrencySummary::default();
    }
    let Some(log_file) = resolve_log_file() else {
        return RunAllConcurrencySummary::default();
    };
    let row_limit = execution_ids.len().saturating_mul(8).max(200);
    let rows = load_values(&log_file, row_limit).unwrap_or_default();
    let mut workers: BTreeSet<String> = BTreeSet::new();
    let mut max_retry_attempt = 0u32;
    let mut first_queue_started_at: Option<String> = None;
    let mut first_task_started_at: Option<String> = None;
    let mut last_task_finished_at: Option<String> = None;

    for row in rows {
        let execution_id = row
            .get("execution_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty());
        if !execution_id.is_some_and(|v| execution_ids.contains(v)) {
            continue;
        }
        if let Some(worker_id) = row
            .get("worker_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            workers.insert(worker_id.to_string());
        }
        let retry_attempt = row
            .get("retry_attempt")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .min(u32::MAX as u64) as u32;
        max_retry_attempt = max_retry_attempt.max(retry_attempt);
        min_ts_slot(
            &mut first_queue_started_at,
            row.get("queue_started_at").and_then(Value::as_str),
        );
        min_ts_slot(
            &mut first_task_started_at,
            row.get("task_started_at").and_then(Value::as_str),
        );
        max_ts_slot(
            &mut last_task_finished_at,
            row.get("task_finished_at").and_then(Value::as_str),
        );
    }

    RunAllConcurrencySummary {
        worker_count: workers.len() as u64,
        workers: workers.into_iter().collect(),
        max_retry_attempt,
        first_queue_started_at,
        first_task_started_at,
        last_task_finished_at,
    }
}

fn runall_failure_pattern(
    summary: &RunAllSummary,
    halted_remaining: usize,
    backend_fallback_rows: u64,
    wave_pressure: &RunAllWavePressure,
    reason_counts: &HashMap<String, usize>,
) -> &'static str {
    if halted_remaining > 0 || summary.critical_errors > 0 {
        "critical_halt"
    } else if summary.blocked > 0 {
        "blocked_tasks"
    } else if reason_counts.contains_key("non_retryable_failure")
        || summary.non_retryable_failed > 0
    {
        "non_retryable_failure"
    } else if reason_counts.contains_key("retryable_failure") || summary.retryable_failed > 0 {
        "retryable_failure"
    } else if backend_fallback_rows > 0 {
        "backend_fallback"
    } else if wave_pressure.kind != "none" {
        wave_pressure.kind
    } else {
        "clean"
    }
}

fn runall_backend_fallbacks(summary: &RunAllSummary) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for task in &summary.task_runs {
        let Some(true) = task.get("used_backend_fallback").and_then(Value::as_bool) else {
            continue;
        };
        let requested = task
            .get("requested_backend")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let actual = task
            .get("backend")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let key = format!("{requested}->{actual}");
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn render_counts_compact(counts: &HashMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    let mut pairs: Vec<(String, usize)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<String>>()
        .join(", ")
}

fn runall_halted_remaining(summary: &RunAllSummary, scheduled_count: usize) -> usize {
    if !summary.halted_on_critical {
        return 0;
    }
    scheduled_count.saturating_sub(summary.task_runs.len())
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

fn progress_enabled() -> bool {
    env::var("CX_TASK_RUN_ALL_PROGRESS")
        .map(|v| !(v == "0" || v.eq_ignore_ascii_case("false")))
        .unwrap_or(true)
}

fn emit_progress(message: &str) {
    if progress_enabled() {
        crate::cx_eprintln!("{message}");
    }
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
    wave: Option<&TaskWaveMeta>,
    f: F,
) -> T
where
    F: FnOnce() -> T,
{
    let prev_attempt = env::var("CX_TASK_RETRY_ATTEMPT").ok();
    let prev_max = env::var("CX_TASK_RETRY_MAX").ok();
    let prev_reason = env::var("CX_TASK_RETRY_REASON").ok();
    let prev_backoff = env::var("CX_TASK_RETRY_BACKOFF_MS").ok();
    let prev_queue_started_at = env::var("CX_TASK_QUEUE_STARTED_AT").ok();
    let prev_task_started_at = env::var("CX_TASK_STARTED_AT").ok();
    let prev_task_finished_at = env::var("CX_TASK_FINISHED_AT").ok();
    let prev_wave_index = env::var("CX_TASK_WAVE_INDEX").ok();
    let prev_wave_mode = env::var("CX_TASK_WAVE_MODE").ok();
    let prev_wave_size = env::var("CX_TASK_WAVE_SIZE").ok();
    unsafe {
        env::set_var("CX_TASK_RETRY_ATTEMPT", attempt.to_string());
        env::set_var("CX_TASK_RETRY_MAX", retry_max.to_string());
        env::set_var("CX_TASK_QUEUE_STARTED_AT", utc_now_iso());
        env::set_var("CX_TASK_STARTED_AT", utc_now_iso());
        env::remove_var("CX_TASK_FINISHED_AT");
    }
    match wave {
        Some(meta) => unsafe {
            env::set_var("CX_TASK_WAVE_INDEX", meta.index.to_string());
            env::set_var("CX_TASK_WAVE_MODE", &meta.mode);
            env::set_var("CX_TASK_WAVE_SIZE", meta.size.to_string());
        },
        None => unsafe {
            env::remove_var("CX_TASK_WAVE_INDEX");
            env::remove_var("CX_TASK_WAVE_MODE");
            env::remove_var("CX_TASK_WAVE_SIZE");
        },
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
    match prev_queue_started_at {
        Some(v) => unsafe { env::set_var("CX_TASK_QUEUE_STARTED_AT", v) },
        None => unsafe { env::remove_var("CX_TASK_QUEUE_STARTED_AT") },
    }
    match prev_task_started_at {
        Some(v) => unsafe { env::set_var("CX_TASK_STARTED_AT", v) },
        None => unsafe { env::remove_var("CX_TASK_STARTED_AT") },
    }
    match prev_task_finished_at {
        Some(v) => unsafe { env::set_var("CX_TASK_FINISHED_AT", v) },
        None => unsafe { env::remove_var("CX_TASK_FINISHED_AT") },
    }
    match prev_wave_index {
        Some(v) => unsafe { env::set_var("CX_TASK_WAVE_INDEX", v) },
        None => unsafe { env::remove_var("CX_TASK_WAVE_INDEX") },
    }
    match prev_wave_mode {
        Some(v) => unsafe { env::set_var("CX_TASK_WAVE_MODE", v) },
        None => unsafe { env::remove_var("CX_TASK_WAVE_MODE") },
    }
    match prev_wave_size {
        Some(v) => unsafe { env::set_var("CX_TASK_WAVE_SIZE", v) },
        None => unsafe { env::remove_var("CX_TASK_WAVE_SIZE") },
    }
    out
}

fn should_retry(failure: FailureClass, attempt: u32, retry_max: u32) -> bool {
    matches!(failure, FailureClass::Retryable) && attempt <= retry_max
}

fn run_task_managed_subprocess(
    id: String,
    backend: String,
    meta: LaunchEnvMeta,
) -> Result<(i32, Option<String>), String> {
    let mut retry_reason: Option<String> = None;
    let mut retry_backoff: Option<u64> = None;
    for attempt in 1..=(meta.max_retries + 1) {
        let exe = std::env::current_exe().map_err(|e| format!("task run-all: current_exe: {e}"))?;
        let mut cmd = Command::new(exe);
        cmd.args(["task", "run", &id, "--managed-by-parent"]);
        cmd.args(["--backend", &backend]);
        cmd.env("CX_TASK_ID", &id);
        cmd.env("CX_TASK_QUEUE_MS", meta.queue_ms.to_string());
        cmd.env("CX_TASK_QUEUE_STARTED_AT", &meta.queue_started_at);
        cmd.env("CX_TASK_STARTED_AT", utc_now_iso());
        cmd.env_remove("CX_TASK_FINISHED_AT");
        cmd.env("CX_TASK_WORKER_ID", &meta.worker_id);
        if let Some(wave) = meta.wave.as_ref() {
            cmd.env("CX_TASK_WAVE_INDEX", wave.index.to_string());
            cmd.env("CX_TASK_WAVE_MODE", &wave.mode);
            cmd.env("CX_TASK_WAVE_SIZE", wave.size.to_string());
        } else {
            cmd.env_remove("CX_TASK_WAVE_INDEX");
            cmd.env_remove("CX_TASK_WAVE_MODE");
            cmd.env_remove("CX_TASK_WAVE_SIZE");
        }
        cmd.env("CX_TASK_RETRY_ATTEMPT", attempt.to_string());
        cmd.env("CX_TASK_RETRY_MAX", meta.max_retries.to_string());
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
        if let Some(parent_id) = meta.task_parent_id.as_deref() {
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
        if should_retry(failure.class, attempt, meta.max_retries) {
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
    let started = Instant::now();

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
    let readiness = task_readiness_value(&tasks, &options.status_filter);
    if selected_count == 0 {
        if options.plan_json {
            let empty_plan = build_task_run_plan(&tasks, &options.status_filter);
            return plan_json_out(&options, &empty_plan, true);
        }
        if options.dry_run {
            let empty_index: HashMap<String, TaskRecord> = HashMap::new();
            let empty_schedule: Vec<String> = Vec::new();
            return dry_run_out(&options, &empty_schedule, &empty_index, 0, true);
        }
        if options.as_json {
            let empty_summary = RunAllSummary::default();
            let empty_concurrency = RunAllConcurrencySummary::default();
            println!(
                "{}",
                serde_json::json!({
                    "contract_version": TASK_RUN_ALL_JSON_CONTRACT_VERSION,
                    "status_filter": options.status_filter,
                    "mode": options.run_mode,
                    "summary_format": options.summary_format,
                    "strict_plan": options.strict_plan,
                    "plan_json": options.plan_json,
                    "task_readiness": readiness,
                    "scheduled": 0,
                    "complete": 0,
                    "failed": 0,
                    "blocked": 0,
                    "retryable_failures": 0,
                    "non_retryable_failures": 0,
                    "critical_errors": 0,
                    "halted_on_critical": false,
                    "invariants": runall_invariants_value(
                        0,
                        &empty_summary,
                        0,
                        false,
                        &empty_concurrency
                    ),
                    "duration_ms": 0,
                    "tasks": []
                })
            );
        } else {
            println!("No tasks matched status '{}'.", options.status_filter);
        }
        return 0;
    }
    let task_index: HashMap<String, TaskRecord> =
        tasks.iter().map(|t| (t.id.clone(), t.clone())).collect();

    let maybe_plan = if matches!(options.run_mode.as_str(), "mixed" | "parallel")
        || options.plan_json
        || options.dry_run
        || options.strict_plan
    {
        Some(build_task_run_plan(&tasks, &options.status_filter))
    } else {
        None
    };
    let strict_issue = if options.run_mode == "parallel" {
        maybe_plan
            .as_ref()
            .and_then(strict_issue_parallel)
            .filter(|_| options.strict_plan || options.plan_json)
    } else {
        None
    };
    if options.plan_json {
        let plan = maybe_plan.as_ref().unwrap_or_else(|| unreachable!());
        return plan_json_out(&options, plan, strict_issue.is_none());
    }

    let blocked_count = maybe_plan.as_ref().map(|p| p.blocked.len()).unwrap_or(0);
    let mut wave_meta_map: HashMap<String, TaskWaveMeta> = HashMap::new();
    if let Some(plan) = maybe_plan.as_ref() {
        wave_meta_map = task_wave_map(plan);
    }
    let schedule: Vec<String> = if matches!(options.run_mode.as_str(), "mixed" | "parallel") {
        let plan = maybe_plan.as_ref().unwrap_or_else(|| unreachable!());
        if options.strict_plan
            && options.run_mode == "parallel"
            && let Some(reason) = strict_issue.as_deref()
        {
            if options.dry_run {
                let ids: Vec<String> = plan
                    .waves
                    .iter()
                    .flat_map(|wave| wave.task_ids.iter().cloned())
                    .collect();
                return dry_run_out(&options, &ids, &task_index, plan.blocked.len(), false);
            }
            crate::cx_eprintln!(
                "{} task run-all: strict-plan failed ({reason})",
                cli_app_name()
            );
            crate::cx_eprintln!(
                "preflight: recommended_mode={} can_run_mixed={} can_run_parallel={} waves={} parallel_waves={} largest_parallel_wave={}",
                readiness
                    .get("recommended_mode")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("sequential"),
                readiness
                    .get("can_run_mixed")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                readiness
                    .get("can_run_parallel")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                readiness
                    .get("waves")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                readiness
                    .get("parallel_waves")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                readiness
                    .get("largest_parallel_wave")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0)
            );
            if !plan.blocked.is_empty() {
                for b in &plan.blocked {
                    crate::cx_eprintln!(" - {}: {}", b.id, b.reason);
                }
            }
            crate::cx_eprintln!(
                "hint: run '{} --status {} --mode parallel --strict-plan --plan-json' for machine-readable diagnostics",
                command_with_cli("task run-all"),
                options.status_filter
            );
            return 1;
        }
        if !plan.blocked.is_empty() {
            crate::cx_eprintln!(
                "{} task run-all: blocked tasks prevent full schedule:",
                cli_app_name()
            );
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
        if !options.as_json {
            println!(
                "run-all preflight: requested_mode={} recommended_mode={} can_run_mixed={} can_run_parallel={} waves={} parallel_waves={} largest_parallel_wave={} runnable={} backend_pool={} max_workers={} backend_caps={} fairness={} halt_on_critical={}",
                options.run_mode,
                readiness
                    .get("recommended_mode")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("sequential"),
                readiness
                    .get("can_run_mixed")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                readiness
                    .get("can_run_parallel")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                readiness
                    .get("waves")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                readiness
                    .get("parallel_waves")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                readiness
                    .get("largest_parallel_wave")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                ids.len(),
                pool,
                options.max_workers,
                cap_notes,
                options.fairness,
                options.halt_on_critical
            );
            if options.run_mode == "parallel"
                && !readiness
                    .get("can_run_parallel")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                && let Some(reason) = readiness
                    .get("strict_plan_reason")
                    .and_then(serde_json::Value::as_str)
                    .filter(|v| !v.is_empty())
            {
                println!("run-all preflight_reason: {reason}");
            }
            for wave in &plan.waves {
                println!(
                    "wave {} [{}] -> {}",
                    wave.index,
                    wave.mode,
                    wave.task_ids.join(",")
                );
            }
        }
        ids
    } else {
        tasks
            .iter()
            .filter(|t| t.status == options.status_filter)
            .map(|t| t.id.clone())
            .collect()
    };
    if options.dry_run {
        return dry_run_out(
            &options,
            &schedule,
            &task_index,
            blocked_count,
            strict_issue.is_none(),
        );
    }

    let scheduled_count = schedule.len();
    let summary = if options.run_mode == "parallel"
        || (options.run_mode == "mixed" && options.max_workers > 1)
    {
        match run_schedule_parallel(&schedule, &task_index, &options, &wave_meta_map) {
            Ok(v) => v,
            Err(e) => {
                crate::cx_eprintln!("{} task run-all: {e}", cli_app_name());
                return 1;
            }
        }
    } else {
        let mut summary = RunAllSummary::default();
        let mut halt_all = false;
        let total = schedule.len();
        for (idx, id) in schedule.iter().enumerate() {
            if halt_all {
                break;
            }
            let task = task_index.get(id);
            let max_retries = task.and_then(|t| t.max_retries).unwrap_or(0);
            let task_parent_id = task.and_then(|t| t.parent_id.clone());
            let requested_backend = choose_backend_for_task(task, &options.backend_pool, idx);
            let backend_selected = fallback_backend(
                requested_backend.clone(),
                &available_pool(&options.backend_pool),
            );
            if !options.as_json {
                emit_progress(&format!(
                    "cxrs task run-all: start [{}/{}] task={} backend={} retries={}",
                    idx + 1,
                    total,
                    id,
                    backend_selected.as_deref().unwrap_or("auto"),
                    max_retries
                ));
            }
            let run_started = Instant::now();
            let wave_meta = wave_meta_map
                .get(id)
                .cloned()
                .unwrap_or_else(|| fallback_wave_meta(idx));
            let event_backend = backend_selected
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            emit_task_event(options.events_jsonl, {
                let mut event = TaskEvent::new("queued");
                event.task_id = Some(id);
                event.status = Some("pending");
                event.backend = Some(&event_backend);
                event.requested_backend = requested_backend.as_deref();
                event.wave_index = Some(wave_meta.index);
                event.wave_mode = Some(&wave_meta.mode);
                event.wave_size = Some(wave_meta.size);
                event
            });
            emit_start_event(
                &options,
                id,
                &event_backend,
                requested_backend.as_deref(),
                0,
                &wave_meta,
            );
            if options.as_json {
                let backend = event_backend.clone();
                let run_result = run_task_managed_subprocess(
                    id.clone(),
                    backend.clone(),
                    LaunchEnvMeta {
                        queue_ms: 0,
                        queue_started_at: utc_now_iso(),
                        worker_id: "w1".to_string(),
                        task_parent_id,
                        max_retries,
                        wave: Some(wave_meta.clone()),
                    },
                );
                match run_result {
                    Ok((code, execution_id)) => {
                        if code == 0 {
                            summary.record_success();
                            let event = TaskRunEvent {
                                id: id.clone(),
                                backend,
                                requested_backend: requested_backend.clone(),
                                status: "complete".to_string(),
                                execution_id,
                                failure_class: None,
                                queue_ms: 0,
                                wave_index: wave_meta.index,
                                wave_mode: wave_meta.mode.clone(),
                                wave_size: wave_meta.size,
                            };
                            emit_result_event(&options, &event);
                            summary.add_task_run(event);
                            continue;
                        }
                        let failure = classify_failure_for_execution(execution_id.as_deref());
                        summary.record_failure(failure.class);
                        let event = TaskRunEvent {
                            id: id.clone(),
                            backend,
                            requested_backend: requested_backend.clone(),
                            status: "failed".to_string(),
                            execution_id,
                            failure_class: Some(failure.reason),
                            queue_ms: 0,
                            wave_index: wave_meta.index,
                            wave_mode: wave_meta.mode.clone(),
                            wave_size: wave_meta.size,
                        };
                        emit_result_event(&options, &event);
                        summary.add_task_run(event);
                        continue;
                    }
                    Err(e) => {
                        crate::cx_eprintln!(
                            "{} task run-all: critical error for {id}: {e}",
                            cli_app_name()
                        );
                        summary.record_critical_error();
                        let event = TaskRunEvent {
                            id: id.clone(),
                            backend,
                            requested_backend: requested_backend.clone(),
                            status: "critical_error".to_string(),
                            execution_id: None,
                            failure_class: Some("critical_error".to_string()),
                            queue_ms: 0,
                            wave_index: wave_meta.index,
                            wave_mode: wave_meta.mode.clone(),
                            wave_size: wave_meta.size,
                        };
                        emit_result_event(&options, &event);
                        summary.add_task_run(event);
                        if options.halt_on_critical {
                            summary.halted_on_critical = true;
                            halt_all = true;
                        }
                        continue;
                    }
                }
            }
            let mut retry_reason: Option<String> = None;
            let mut retry_backoff: Option<u64> = None;
            let mut finished = false;
            for attempt in 1..=(max_retries + 1) {
                let run_result = with_retry_env(
                    attempt,
                    max_retries,
                    retry_reason.as_deref(),
                    retry_backoff,
                    Some(&wave_meta),
                    || {
                        (deps.run_task_by_id)(
                            &(deps.make_task_runner)(),
                            id,
                            None,
                            backend_selected.as_deref(),
                            false,
                            true,
                        )
                    },
                );
                match run_result {
                    Ok((code, execution_id)) => {
                        if code == 0 {
                            summary.record_success();
                            emit_progress(&format!(
                                "cxrs task run-all: done [{}/{}] task={} status=complete attempts={} duration_ms={}",
                                idx + 1,
                                total,
                                id,
                                attempt,
                                run_started.elapsed().as_millis()
                            ));
                            let event = TaskRunEvent {
                                id: id.clone(),
                                backend: backend_selected
                                    .clone()
                                    .unwrap_or_else(|| "unknown".to_string()),
                                requested_backend: requested_backend.clone(),
                                status: "complete".to_string(),
                                execution_id,
                                failure_class: None,
                                queue_ms: 0,
                                wave_index: wave_meta.index,
                                wave_mode: wave_meta.mode.clone(),
                                wave_size: wave_meta.size,
                            };
                            emit_result_event(&options, &event);
                            summary.add_task_run(event);
                            finished = true;
                            break;
                        }
                        let failure = classify_failure_for_execution(execution_id.as_deref());
                        if should_retry(failure.class, attempt, max_retries) {
                            let mut event = TaskEvent::new("retrying");
                            event.task_id = Some(id);
                            event.status = Some("in_progress");
                            event.backend = Some(&event_backend);
                            event.requested_backend = requested_backend.as_deref();
                            event.execution_id = execution_id.as_deref();
                            event.failure_class = Some(&failure.reason);
                            event.queue_ms = Some(0);
                            event.wave_index = Some(wave_meta.index);
                            event.wave_mode = Some(&wave_meta.mode);
                            event.wave_size = Some(wave_meta.size);
                            emit_runall_event(&options, event);
                            let next_backoff = retry_backoff_ms(attempt - 1);
                            retry_reason = Some(failure.reason);
                            retry_backoff = Some(next_backoff);
                            thread::sleep(Duration::from_millis(next_backoff));
                            continue;
                        }
                        summary.record_failure(failure.class);
                        emit_progress(&format!(
                            "cxrs task run-all: done [{}/{}] task={} status=failed reason={} attempts={} duration_ms={}",
                            idx + 1,
                            total,
                            id,
                            failure.reason,
                            attempt,
                            run_started.elapsed().as_millis()
                        ));
                        crate::cx_eprintln!("{} task run-all: task failed: {id}", cli_app_name());
                        let event = TaskRunEvent {
                            id: id.clone(),
                            backend: backend_selected
                                .clone()
                                .unwrap_or_else(|| "unknown".to_string()),
                            requested_backend: requested_backend.clone(),
                            status: "failed".to_string(),
                            execution_id,
                            failure_class: Some(failure.reason),
                            queue_ms: 0,
                            wave_index: wave_meta.index,
                            wave_mode: wave_meta.mode.clone(),
                            wave_size: wave_meta.size,
                        };
                        emit_result_event(&options, &event);
                        summary.add_task_run(event);
                        finished = true;
                        break;
                    }
                    Err(e) => {
                        crate::cx_eprintln!(
                            "{} task run-all: critical error for {id}: {e}",
                            cli_app_name()
                        );
                        summary.record_critical_error();
                        emit_progress(&format!(
                            "cxrs task run-all: done [{}/{}] task={} status=critical_error attempts={} duration_ms={}",
                            idx + 1,
                            total,
                            id,
                            attempt,
                            run_started.elapsed().as_millis()
                        ));
                        let event = TaskRunEvent {
                            id: id.clone(),
                            backend: backend_selected
                                .clone()
                                .unwrap_or_else(|| "unknown".to_string()),
                            requested_backend: requested_backend.clone(),
                            status: "critical_error".to_string(),
                            execution_id: None,
                            failure_class: Some("critical_error".to_string()),
                            queue_ms: 0,
                            wave_index: wave_meta.index,
                            wave_mode: wave_meta.mode.clone(),
                            wave_size: wave_meta.size,
                        };
                        emit_result_event(&options, &event);
                        summary.add_task_run(event);
                        if options.halt_on_critical {
                            summary.halted_on_critical = true;
                            halt_all = true;
                        }
                        finished = true;
                        break;
                    }
                }
            }
            if !finished {
                summary.record_failure(FailureClass::NonRetryable);
                emit_progress(&format!(
                    "cxrs task run-all: done [{}/{}] task={} status=failed reason=unknown duration_ms={}",
                    idx + 1,
                    total,
                    id,
                    run_started.elapsed().as_millis()
                ));
                crate::cx_eprintln!("{} task run-all: task failed: {id}", cli_app_name());
                let event = TaskRunEvent {
                    id: id.clone(),
                    backend: backend_selected
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    requested_backend: requested_backend.clone(),
                    status: "failed".to_string(),
                    execution_id: None,
                    failure_class: Some("non_retryable_failure".to_string()),
                    queue_ms: 0,
                    wave_index: wave_meta.index,
                    wave_mode: wave_meta.mode.clone(),
                    wave_size: wave_meta.size,
                };
                emit_result_event(&options, &event);
                summary.add_task_run(event);
            }
        }
        summary
    };
    let backend_fallbacks = runall_backend_fallbacks(&summary);
    let reason_counts = runall_reason_counts(&summary);
    let halted_remaining = runall_halted_remaining(&summary, scheduled_count);
    emit_summary_event(&options, scheduled_count, &summary, halted_remaining);
    let concurrency_summary = runall_concurrency_summary(&summary);
    let invariants = runall_invariants_value(
        scheduled_count as u64,
        &summary,
        halted_remaining,
        options.halt_on_critical,
        &concurrency_summary,
    );
    let preflight = runall_preflight_value(&options, &readiness, true);
    if options.as_json {
        let payload = serde_json::json!({
            "contract_version": TASK_RUN_ALL_JSON_CONTRACT_VERSION,
            "status_filter": options.status_filter,
            "mode": options.run_mode,
            "summary_format": options.summary_format,
            "strict_plan": options.strict_plan,
            "plan_json": options.plan_json,
            "task_readiness": readiness,
            "preflight": preflight,
            "scheduled": scheduled_count,
            "complete": summary.ok,
            "failed": summary.failed,
            "blocked": summary.blocked,
            "retryable_failures": summary.retryable_failed,
            "non_retryable_failures": summary.non_retryable_failed,
            "critical_errors": summary.critical_errors,
            "halted_on_critical": summary.halted_on_critical,
            "halted_remaining": halted_remaining,
            "backend_fallbacks": backend_fallbacks,
            "invariants": invariants.clone(),
            "concurrency_summary": {
                "worker_count": concurrency_summary.worker_count,
                "workers": concurrency_summary.workers,
                "max_retry_attempt": concurrency_summary.max_retry_attempt,
                "first_queue_started_at": concurrency_summary.first_queue_started_at,
                "first_task_started_at": concurrency_summary.first_task_started_at,
                "last_task_finished_at": concurrency_summary.last_task_finished_at
            },
            "duration_ms": started.elapsed().as_millis() as u64,
            "tasks": summary.task_runs
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!(
                    "{} task run-all: failed to render json: {e}",
                    cli_app_name()
                );
                return 1;
            }
        }
    } else {
        if options.summary_format == "json" {
            let payload = serde_json::json!({
                "contract_version": "task-run-all-summary.v1",
                "status_filter": options.status_filter,
                "mode": options.run_mode,
                "task_readiness": readiness,
                "preflight": preflight,
                "scheduled": scheduled_count,
                "complete": summary.ok,
                "failed": summary.failed,
                "blocked": summary.blocked,
                "retryable_failures": summary.retryable_failed,
                "non_retryable_failures": summary.non_retryable_failed,
                "critical_errors": summary.critical_errors,
                "halted_on_critical": summary.halted_on_critical,
                "halted_remaining": halted_remaining,
                "backend_fallbacks": backend_fallbacks,
                "invariants": invariants.clone(),
                "concurrency_summary": {
                    "worker_count": concurrency_summary.worker_count,
                    "workers": concurrency_summary.workers,
                    "max_retry_attempt": concurrency_summary.max_retry_attempt,
                    "first_queue_started_at": concurrency_summary.first_queue_started_at,
                    "first_task_started_at": concurrency_summary.first_task_started_at,
                    "last_task_finished_at": concurrency_summary.last_task_finished_at
                },
                "duration_ms": started.elapsed().as_millis() as u64,
                "failure_reasons": reason_counts,
                "failed_task_ids": runall_failed_ids(&summary, 25)
            });
            match serde_json::to_string_pretty(&payload) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    crate::cx_eprintln!(
                        "{} task run-all: failed to render summary json: {e}",
                        cli_app_name()
                    );
                    return 1;
                }
            }
        } else {
            println!(
                "run-all summary: mode={}, recommended_mode={}, strict_plan={}, complete={}, failed={}, blocked={}, retryable_failures={}, non_retryable_failures={}, critical_errors={}",
                options.run_mode,
                readiness
                    .get("recommended_mode")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("sequential"),
                options.strict_plan,
                summary.ok,
                summary.failed,
                summary.blocked,
                summary.retryable_failed,
                summary.non_retryable_failed,
                summary.critical_errors
            );
            if summary.halted_on_critical {
                println!("run-all halted_on_critical: true");
                println!("run-all halted_remaining: {halted_remaining}");
            }
            if let Some(status) = invariants.get("status").and_then(Value::as_str) {
                println!("run-all invariants_status: {status}");
            }
            if let Some(issues) = invariants.get("issues").and_then(Value::as_array) {
                for (idx, issue) in issues.iter().filter_map(Value::as_str).enumerate() {
                    println!("run-all invariant_issue_{}: {issue}", idx + 1);
                }
            }
            print_preflight_text(&preflight);
            if !backend_fallbacks.is_empty() {
                println!(
                    "run-all backend_fallbacks: {}",
                    render_counts_compact(&backend_fallbacks)
                );
            }
            if summary.failed > 0 {
                println!(
                    "run-all failure_reasons: {}",
                    render_counts_compact(&reason_counts)
                );
                let failed_ids = runall_failed_ids(&summary, 10);
                if !failed_ids.is_empty() {
                    println!("run-all failed_task_ids: {}", failed_ids.join(","));
                }
            }
            if concurrency_summary.worker_count > 0 {
                println!("run-all worker_count: {}", concurrency_summary.worker_count);
                if !concurrency_summary.workers.is_empty() {
                    println!("run-all workers: {}", concurrency_summary.workers.join(","));
                }
                println!(
                    "run-all max_retry_attempt: {}",
                    concurrency_summary.max_retry_attempt
                );
                if let Some(v) = &concurrency_summary.first_queue_started_at {
                    println!("run-all first_queue_started_at: {v}");
                }
                if let Some(v) = &concurrency_summary.first_task_started_at {
                    println!("run-all first_task_started_at: {v}");
                }
                if let Some(v) = &concurrency_summary.last_task_finished_at {
                    println!("run-all last_task_finished_at: {v}");
                }
            }
        }
    }
    let wave_pressure = runall_wave_pressure(&summary, &options.run_mode);
    let backend_fallback_rows = backend_fallbacks.values().copied().sum::<usize>() as u64;
    let invocation_command = runall_invocation_command(&options);
    let run_all_workers = if concurrency_summary.workers.is_empty() {
        None
    } else {
        Some(concurrency_summary.workers.join(","))
    };
    let current_summary = serde_json::json!({
        "run_all_mode": options.run_mode,
        "run_all_halted_remaining": halted_remaining as u64,
        "run_all_backend_fallback_rows": backend_fallback_rows,
        "run_all_failed": summary.failed as u64,
        "run_all_critical_errors": summary.critical_errors as u64,
        "run_all_blocked": summary.blocked as u64,
        "run_all_worker_count": concurrency_summary.worker_count,
        "run_all_workers": run_all_workers,
        "run_all_max_retry_attempt": concurrency_summary.max_retry_attempt,
        "run_all_first_queue_started_at": concurrency_summary.first_queue_started_at.clone(),
        "run_all_first_task_started_at": concurrency_summary.first_task_started_at.clone(),
        "run_all_last_task_finished_at": concurrency_summary.last_task_finished_at.clone()
    });
    let current_wave = serde_json::json!({
        "latest_wave_index": wave_pressure.latest_wave_index,
        "max_queue_wave_index": wave_pressure.max_queue_wave_index,
        "max_queue_wave_ms": wave_pressure.max_queue_wave_ms
    });
    let resume_recommendations =
        exec_recommendations_value(Some(&current_summary), Some(&current_wave));
    let recommended_resume_point = resume_recommendations
        .first()
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let failure_pattern = runall_failure_pattern(
        &summary,
        halted_remaining,
        backend_fallback_rows,
        &wave_pressure,
        &reason_counts,
    );
    let _ = crate::runlog::log_task_run_all_summary(crate::runlog::TaskRunAllSummaryLogInput {
        mode: &options.run_mode,
        halt_on_critical: options.halt_on_critical,
        scheduled: scheduled_count as u64,
        complete: summary.ok as u64,
        failed: summary.failed as u64,
        blocked: summary.blocked as u64,
        retryable_failures: summary.retryable_failed as u64,
        non_retryable_failures: summary.non_retryable_failed as u64,
        critical_errors: summary.critical_errors as u64,
        halted_remaining: halted_remaining as u64,
        backend_fallback_rows,
        backend_fallbacks: if backend_fallbacks.is_empty() {
            None
        } else {
            Some(render_counts_compact(&backend_fallbacks))
        },
        wave_pressure_kind: Some(wave_pressure.kind),
        wave_pressure_suggested_mode: wave_pressure.suggested_mode,
        latest_wave_index: wave_pressure.latest_wave_index,
        max_queue_wave_index: wave_pressure.max_queue_wave_index,
        max_queue_wave_ms: Some(wave_pressure.max_queue_wave_ms),
        worker_count: Some(concurrency_summary.worker_count),
        workers: run_all_workers.as_deref(),
        max_retry_attempt: Some(concurrency_summary.max_retry_attempt),
        first_queue_started_at: concurrency_summary.first_queue_started_at.as_deref(),
        first_task_started_at: concurrency_summary.first_task_started_at.as_deref(),
        last_task_finished_at: concurrency_summary.last_task_finished_at.as_deref(),
        invocation_command: Some(&invocation_command),
        failure_pattern: Some(failure_pattern),
        recommended_resume_point: recommended_resume_point.as_deref(),
        duration_ms: started.elapsed().as_millis() as u64,
    });
    if summary.failed > 0 { 1 } else { 0 }
}

#[derive(Debug, Clone)]
struct RunAllOptions {
    status_filter: String,
    run_mode: String,
    strict_plan: bool,
    plan_json: bool,
    dry_run: bool,
    backend_pool: Vec<String>,
    backend_caps: HashMap<String, usize>,
    max_workers: usize,
    fairness: String,
    halt_on_critical: bool,
    events_jsonl: bool,
    as_json: bool,
    summary_format: String,
}

fn strict_issue_parallel(plan: &crate::tasks_plan::TaskRunPlan) -> Option<String> {
    if !plan.blocked.is_empty() {
        return Some("blocked dependencies present".to_string());
    }
    let single_wave = plan.waves.len() == 1;
    let all_parallel = plan.waves.iter().all(|w| w.mode == "parallel");
    if single_wave && all_parallel {
        None
    } else {
        Some("parallel mode would serialize across waves".to_string())
    }
}

fn plan_json_out(
    options: &RunAllOptions,
    plan: &crate::tasks_plan::TaskRunPlan,
    strict_ok: bool,
) -> i32 {
    let wave_count = plan.waves.len() as u64;
    let parallel_task_count = plan
        .waves
        .iter()
        .filter(|w| w.mode == "parallel")
        .map(|w| w.task_ids.len() as u64)
        .sum::<u64>();
    let sequential_task_count = plan
        .waves
        .iter()
        .filter(|w| w.mode != "parallel")
        .map(|w| w.task_ids.len() as u64)
        .sum::<u64>();
    let blocked_count = plan.blocked.len() as u64;
    let strict_reason = if options.strict_plan && !strict_ok && options.run_mode == "parallel" {
        strict_issue_parallel(plan)
    } else {
        None
    };
    let can_execute = plan.blocked.is_empty() && (!options.strict_plan || strict_ok);
    let payload = serde_json::json!({
        "contract_version": TASK_RUN_PLAN_JSON_CONTRACT_VERSION,
        "status_filter": options.status_filter,
        "requested_mode": options.run_mode,
        "strict_plan": options.strict_plan,
        "strict_plan_ok": strict_ok,
        "strict_plan_reason": strict_reason,
        "can_execute": can_execute,
        "wave_count": wave_count,
        "parallel_task_count": parallel_task_count,
        "sequential_task_count": sequential_task_count,
        "blocked_count": blocked_count,
        "selected": plan.selected,
        "waves": plan.waves,
        "blocked": plan.blocked
    });
    match serde_json::to_string_pretty(&payload) {
        Ok(s) => {
            println!("{s}");
            if can_execute { 0 } else { 1 }
        }
        Err(e) => {
            crate::cx_eprintln!("cxrs task run-all: failed to render plan json: {e}");
            1
        }
    }
}

fn runall_preflight_value(options: &RunAllOptions, readiness: &Value, strict_ok: bool) -> Value {
    let recommended_mode = readiness
        .get("recommended_mode")
        .and_then(Value::as_str)
        .unwrap_or("sequential");
    let strict_reason = readiness
        .get("strict_plan_reason")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let blocked_total = readiness
        .get("blocked_total")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let latest_wave = latest_wave_sum();
    let latest_summary = crate::doctor::latest_run_all_sum();
    let latest_wave_index = latest_wave
        .as_ref()
        .and_then(|w| w.get("latest_wave_index"))
        .cloned()
        .unwrap_or(Value::Null);
    let max_queue_wave_index = latest_wave
        .as_ref()
        .and_then(|w| w.get("max_queue_wave_index"))
        .cloned()
        .unwrap_or(Value::Null);
    let max_queue_wave_ms = latest_wave
        .as_ref()
        .and_then(|w| w.get("max_queue_wave_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let advice;
    let recommendations: Vec<String>;
    let mut blockers: Vec<String> = Vec::new();
    if options.strict_plan && options.run_mode == "parallel" && !strict_ok {
        advice = "parallel strict-plan is not executable; switch to the recommended mode or inspect the plan json";
        blockers.push("strict_plan_blocked".to_string());
        recommendations = vec![
            format!(
                "{} --status {} --mode {}",
                command_with_cli("task run-all"),
                options.status_filter,
                recommended_mode
            ),
            format!(
                "{} --status {} --mode parallel --strict-plan --plan-json",
                command_with_cli("task run-all"),
                options.status_filter
            ),
        ];
    } else if options.run_mode != recommended_mode {
        advice = "requested mode differs from the recommended mode; review the recommendation before widening concurrency";
        blockers.push("mode_mismatch".to_string());
        recommendations = vec![
            format!(
                "{} --status {} --mode {}",
                command_with_cli("task run-all"),
                options.status_filter,
                recommended_mode
            ),
            command_with_cli("task check --json"),
        ];
    } else if blocked_total > 0 {
        advice = "blocked tasks remain in the selected set; inspect dependency and resource blockers before expecting a full schedule";
        blockers.push("blocked_tasks".to_string());
        recommendations = vec![
            command_with_cli("task check --json"),
            format!(
                "{} --status {} --dry-run --json",
                command_with_cli("task run-all"),
                options.status_filter
            ),
        ];
    } else if max_queue_wave_index.as_u64().unwrap_or(0) > 1 && max_queue_wave_ms >= 2000 {
        advice = "recent runs show queue pressure in later waves; prefer a narrower mode before widening concurrency again";
        blockers.push("later_wave_queue".to_string());
        let narrower = match options.run_mode.as_str() {
            "parallel" => "mixed",
            "mixed" => "sequential",
            _ => recommended_mode,
        };
        recommendations = vec![
            format!(
                "{} --status {} --mode {}",
                command_with_cli("task run-all"),
                options.status_filter,
                narrower
            ),
            command_with_cli("scheduler --json --window 20"),
        ];
    } else {
        advice = "preflight is operationally clean";
        recommendations = vec![
            command_with_cli("task check --json"),
            format!(
                "{} --status {}",
                command_with_cli("task run-all"),
                options.status_filter
            ),
        ];
    }
    let primary = recommendations.first().map(String::as_str);
    let recent_context = exec_context_value(latest_summary.as_ref(), primary);
    if recent_context
        .get("repeated_failure_pattern")
        .and_then(Value::as_str)
        .is_some()
    {
        blockers.push("repeated_failure_pattern".to_string());
    }
    let reasoning_gate = reasoning_gate_value(
        primary,
        primary.map(next_cost_class),
        primary.map(next_reasoning_required),
        primary.map(next_quality_risk),
        &blockers,
        advice == "preflight is operationally clean",
    );
    serde_json::json!({
        "requested_mode": options.run_mode,
        "recommended_mode": recommended_mode,
        "strict_plan": options.strict_plan,
        "strict_ok": strict_ok,
        "advice": advice,
        "recommendations": recommendations,
        "strict_plan_reason": strict_reason,
        "latest_wave_index": latest_wave_index,
        "max_queue_wave_index": max_queue_wave_index,
        "max_queue_wave_ms": max_queue_wave_ms,
        "recent_context": recent_context,
        "reasoning_gate": reasoning_gate
    })
}

fn print_preflight_text(preflight: &Value) {
    println!(
        "run-all preflight_advice: {}",
        preflight
            .get("advice")
            .and_then(Value::as_str)
            .unwrap_or("preflight is operationally clean")
    );
    if let Some(reason) = preflight
        .get("strict_plan_reason")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
    {
        println!("run-all preflight_reason: {reason}");
    }
    if let Some(gate) = preflight.get("reasoning_gate") {
        if let Some(mode) = gate.get("mode").and_then(Value::as_str) {
            println!("run-all preflight_reasoning_gate_mode: {mode}");
        }
        if let Some(why) = gate.get("why").and_then(Value::as_str) {
            println!("run-all preflight_reasoning_gate_why: {why}");
        }
        if let Some(blockers) = gate.get("blockers").and_then(Value::as_array) {
            for (idx, blocker) in blockers.iter().filter_map(Value::as_str).enumerate() {
                println!(
                    "run-all preflight_reasoning_gate_blocker_{}: {blocker}",
                    idx + 1
                );
            }
        }
    }
    if let Some(context) = preflight.get("recent_context") {
        if let Some(value) = context
            .get("last_successful_action")
            .and_then(Value::as_str)
        {
            println!("run-all preflight_context_last_successful_action: {value}");
        }
        if let Some(value) = context.get("last_failed_action").and_then(Value::as_str) {
            println!("run-all preflight_context_last_failed_action: {value}");
        }
        if let Some(value) = context.get("last_mode_used").and_then(Value::as_str) {
            println!("run-all preflight_context_last_mode_used: {value}");
        }
        if let Some(value) = context
            .get("repeated_failure_pattern")
            .and_then(Value::as_str)
        {
            println!("run-all preflight_context_repeated_failure_pattern: {value}");
        }
        if let Some(value) = context
            .get("recommended_resume_point")
            .and_then(Value::as_str)
        {
            println!("run-all preflight_context_recommended_resume_point: {value}");
        }
        println!(
            "run-all preflight_context_resume_reuses_prior_action: {}",
            context
                .get("resume_reuses_prior_action")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        );
    }
    if let Some(recs) = preflight.get("recommendations").and_then(Value::as_array) {
        for (idx, rec) in recs.iter().filter_map(Value::as_str).enumerate() {
            println!("run-all preflight_recommendation_{}: {}", idx + 1, rec);
        }
    }
}

fn dry_run_out(
    options: &RunAllOptions,
    schedule: &[String],
    task_index: &HashMap<String, TaskRecord>,
    blocked: usize,
    strict_ok: bool,
) -> i32 {
    let tasks: Vec<TaskRecord> = task_index.values().cloned().collect();
    let readiness = task_readiness_value(&tasks, &options.status_filter);
    let preflight = runall_preflight_value(options, &readiness, strict_ok);
    let dry_run_summary = RunAllSummary::default();
    let dry_run_concurrency = RunAllConcurrencySummary::default();
    let available = available_pool(&options.backend_pool);
    let mut task_runs: Vec<Value> = Vec::with_capacity(schedule.len());
    for (idx, id) in schedule.iter().enumerate() {
        let task = task_index.get(id);
        let requested_backend = choose_backend_for_task(task, &options.backend_pool, idx);
        let backend_selected = fallback_backend(requested_backend.clone(), &available)
            .unwrap_or_else(|| "unknown".to_string());
        task_runs.push(serde_json::json!({
            "task_id": id,
            "backend": backend_selected,
            "requested_backend": requested_backend,
            "used_backend_fallback": false,
            "status": "dry_run",
            "execution_id": Value::Null,
            "failure_class": Value::Null
        }));
    }
    let payload = serde_json::json!({
        "contract_version": TASK_RUN_ALL_JSON_CONTRACT_VERSION,
        "status_filter": options.status_filter,
        "mode": options.run_mode,
        "strict_plan": options.strict_plan,
        "plan_json": options.plan_json,
        "task_readiness": readiness,
        "preflight": preflight,
        "scheduled": schedule.len(),
        "complete": 0,
        "failed": 0,
        "blocked": blocked,
        "retryable_failures": 0,
        "non_retryable_failures": 0,
        "critical_errors": 0,
        "halted_on_critical": false,
        "halted_remaining": 0,
        "backend_fallbacks": serde_json::json!({}),
        "invariants": runall_invariants_value(
            schedule.len() as u64,
            &dry_run_summary,
            0,
            false,
            &dry_run_concurrency
        ),
        "duration_ms": 0,
        "tasks": task_runs
    });
    if options.as_json {
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!(
                    "{} task run-all: failed to render json: {e}",
                    cli_app_name()
                );
                return 1;
            }
        }
    } else {
        println!(
            "run-all dry-run: mode={}, recommended_mode={}, can_run_mixed={}, can_run_parallel={}, strict_plan={}, strict_ok={}, scheduled={}, blocked={}",
            options.run_mode,
            readiness
                .get("recommended_mode")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("sequential"),
            readiness
                .get("can_run_mixed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            readiness
                .get("can_run_parallel")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            options.strict_plan,
            strict_ok,
            schedule.len(),
            blocked
        );
        print_preflight_text(&preflight);
    }
    if blocked > 0 || (options.strict_plan && !strict_ok) {
        1
    } else {
        0
    }
}

fn normalize_backend(v: &str) -> Option<String> {
    let b = v.trim().to_lowercase();
    match b.as_str() {
        "primary" => Some("primary".to_string()),
        "ollama" | "llamacpp" | "mlx" => Some(b),
        "llama.cpp" | "llama_cpp" => Some("llamacpp".to_string()),
        _ => None,
    }
}

fn parse_backend_pool(raw: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = raw.split(',').filter_map(normalize_backend).collect();
    out.sort();
    out.dedup();
    if out.is_empty() {
        return Err(format!(
            "{} task run-all: --backend-pool requires primary, ollama, llamacpp, and/or mlx",
            cli_app_name()
        ));
    }
    Ok(out)
}

fn parse_backend_cap(raw: &str) -> Result<(String, usize), String> {
    let mut parts = raw.splitn(2, '=');
    let Some(name_raw) = parts.next() else {
        return Err(format!(
            "{} task run-all: invalid --backend-cap",
            cli_app_name()
        ));
    };
    let Some(limit_raw) = parts.next() else {
        return Err(format!(
            "{} task run-all: --backend-cap must use backend=limit",
            cli_app_name()
        ));
    };
    let Some(name) = normalize_backend(name_raw) else {
        return Err(format!(
            "{} task run-all: invalid backend in cap '{name_raw}'",
            cli_app_name()
        ));
    };
    let Ok(limit) = limit_raw.parse::<usize>() else {
        return Err(format!(
            "{} task run-all: invalid cap limit '{limit_raw}'",
            cli_app_name()
        ));
    };
    if limit == 0 {
        return Err(format!(
            "{} task run-all: backend cap must be >= 1",
            cli_app_name()
        ));
    }
    Ok((name, limit))
}

fn default_backend_pool() -> Vec<String> {
    let backend = app_config().llm_backend.to_lowercase();
    if matches!(backend.as_str(), "primary" | "ollama" | "llamacpp" | "mlx") {
        vec![backend]
    } else {
        vec!["primary".to_string()]
    }
}

fn backend_available(name: &str) -> bool {
    let disabled = match name {
        "primary" => std::env::var("CX_DISABLE_CODEX")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        "ollama" => std::env::var("CX_DISABLE_OLLAMA")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        "llamacpp" => std::env::var("CX_DISABLE_LLAMA_CPP")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        "mlx" => std::env::var("CX_DISABLE_MLX")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        _ => false,
    };
    if disabled {
        return false;
    }
    if name == "mlx" {
        let python = std::env::var("CX_MLX_PYTHON")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "python3".to_string());
        let mut cmd = Command::new(python);
        cmd.args(["-c", "import mlx_lm"]);
        return run_command_status_with_timeout(cmd, "mlx import check")
            .ok()
            .is_some_and(|s| s.success());
    }
    let bin = match name {
        "llamacpp" => std::env::var("CX_LLAMA_CPP_BIN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "llama-cli".to_string()),
        "primary" => concat!("co", "dex").to_string(),
        other => other.to_string(),
    };
    let mut cmd = Command::new("bash");
    cmd.args(["-c", &format!("command -v {bin} >/dev/null 2>&1")]);
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
        "Usage: {app_name} task run-all [--status pending|in_progress|complete|failed] [--mode sequential|mixed|parallel] [--strict-plan] [--plan-json] [--dry-run] [--backend-pool primary,ollama,llamacpp,mlx] [--backend-cap backend=limit] [--max-workers N] [--fairness round_robin|least_loaded] [--halt-on-critical|--continue-on-critical] [--events-jsonl] [--summary text|json] [--json|--text]"
    );
    let mut status_filter = "pending".to_string();
    let mut run_mode = "sequential".to_string();
    let mut strict_plan = false;
    let mut plan_json = false;
    let mut dry_run = false;
    let mut backend_pool = default_backend_pool();
    let mut backend_caps: HashMap<String, usize> = HashMap::new();
    let mut max_workers = 1usize;
    let mut fairness = "round_robin".to_string();
    let mut halt_on_critical = app_config().task_halt_on_critical;
    let mut events_jsonl = false;
    let mut as_json: Option<bool> = None;
    let mut summary_format = "text".to_string();
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                    crate::cx_eprintln!("{} task run-all: invalid status '{v}'", cli_app_name());
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
                if !matches!(v, "sequential" | "mixed" | "parallel") {
                    crate::cx_eprintln!("{} task run-all: invalid mode '{v}'", cli_app_name());
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
                    crate::cx_eprintln!(
                        "{} task run-all: --max-workers must be an integer",
                        cli_app_name()
                    );
                    return Err(2);
                };
                if n == 0 {
                    crate::cx_eprintln!(
                        "{} task run-all: --max-workers must be >= 1",
                        cli_app_name()
                    );
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
                    crate::cx_eprintln!("{} task run-all: invalid fairness '{fv}'", cli_app_name());
                    return Err(2);
                }
                fairness = fv;
                i += 2;
            }
            "--halt-on-critical" => {
                halt_on_critical = true;
                i += 1;
            }
            "--continue-on-critical" => {
                halt_on_critical = false;
                i += 1;
            }
            "--events-jsonl" => {
                events_jsonl = true;
                i += 1;
            }
            "--summary" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return Err(2);
                };
                let sv = v.trim().to_lowercase();
                if !matches!(sv.as_str(), "text" | "json") {
                    crate::cx_eprintln!(
                        "{} task run-all: --summary must be text|json",
                        cli_app_name()
                    );
                    return Err(2);
                }
                summary_format = sv;
                i += 2;
            }
            "--json" => {
                as_json = Some(true);
                i += 1;
            }
            "--text" => {
                as_json = Some(false);
                i += 1;
            }
            "--strict-plan" => {
                strict_plan = true;
                i += 1;
            }
            "--plan-json" => {
                plan_json = true;
                i += 1;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            other => {
                crate::cx_eprintln!("{} task run-all: unknown flag '{other}'", cli_app_name());
                return Err(2);
            }
        }
    }
    Ok(RunAllOptions {
        status_filter,
        run_mode,
        strict_plan,
        plan_json,
        dry_run,
        backend_pool,
        backend_caps,
        max_workers,
        fairness,
        halt_on_critical,
        events_jsonl,
        as_json: resolve_json_mode(as_json, false),
        summary_format,
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
            if pool.iter().any(|b| b == "primary") {
                Some("primary".to_string())
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
        "quota_saver" => {
            if pool.iter().any(|b| b == "ollama") {
                Some("ollama".to_string())
            } else if pool.iter().any(|b| b == "primary") {
                Some("primary".to_string())
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

fn task_wave_map(plan: &crate::tasks_plan::TaskRunPlan) -> HashMap<String, TaskWaveMeta> {
    let mut map: HashMap<String, TaskWaveMeta> = HashMap::new();
    for wave in &plan.waves {
        let size = wave.task_ids.len() as u64;
        for id in &wave.task_ids {
            map.insert(
                id.clone(),
                TaskWaveMeta {
                    index: wave.index as u64,
                    mode: wave.mode.clone(),
                    size,
                },
            );
        }
    }
    map
}

fn fallback_wave_meta(index: usize) -> TaskWaveMeta {
    TaskWaveMeta {
        index: (index + 1) as u64,
        mode: "sequential".to_string(),
        size: 1,
    }
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

fn emit_runall_event(options: &RunAllOptions, event: TaskEvent<'_>) {
    emit_task_event(options.events_jsonl, event);
}

fn emit_pending_event(options: &RunAllOptions, launch: &PendingLaunch) {
    let mut event = TaskEvent::new("queued");
    event.task_id = Some(&launch.id);
    event.status = Some("pending");
    event.backend = Some(&launch.backend);
    event.requested_backend = launch.requested_backend.as_deref();
    event.wave_index = Some(launch.wave_index);
    event.wave_mode = Some(&launch.wave_mode);
    event.wave_size = Some(launch.wave_size);
    emit_runall_event(options, event);
}

fn emit_start_event(
    options: &RunAllOptions,
    id: &str,
    backend: &str,
    requested_backend: Option<&str>,
    queue_ms: u64,
    wave: &TaskWaveMeta,
) {
    let mut event = TaskEvent::new("started");
    event.task_id = Some(id);
    event.status = Some("in_progress");
    event.backend = Some(backend);
    event.requested_backend = requested_backend;
    event.queue_ms = Some(queue_ms);
    event.wave_index = Some(wave.index);
    event.wave_mode = Some(&wave.mode);
    event.wave_size = Some(wave.size);
    emit_runall_event(options, event);
}

fn emit_result_event(options: &RunAllOptions, event: &TaskRunEvent) {
    let mut out = TaskEvent::new(match event.status.as_str() {
        "complete" => "completed",
        "critical_error" => "critical_error",
        "failed" => {
            if event.failure_class.as_deref() == Some("policy_blocked") {
                "blocked"
            } else {
                "failed"
            }
        }
        other => other,
    });
    out.task_id = Some(&event.id);
    out.status = Some(&event.status);
    out.backend = Some(&event.backend);
    out.requested_backend = event.requested_backend.as_deref();
    out.execution_id = event.execution_id.as_deref();
    out.failure_class = event.failure_class.as_deref();
    out.queue_ms = Some(event.queue_ms);
    out.wave_index = Some(event.wave_index);
    out.wave_mode = Some(&event.wave_mode);
    out.wave_size = Some(event.wave_size);
    emit_runall_event(options, out);
}

fn emit_summary_event(
    options: &RunAllOptions,
    scheduled_count: usize,
    summary: &RunAllSummary,
    halted_remaining: usize,
) {
    let mut event = TaskEvent::new("summary");
    event.scheduled = Some(scheduled_count as u64);
    event.complete = Some(summary.ok as u64);
    event.failed = Some(summary.failed as u64);
    event.blocked = Some(summary.blocked as u64);
    event.critical_errors = Some(summary.critical_errors as u64);
    event.halted_remaining = Some(halted_remaining as u64);
    emit_runall_event(options, event);
}

fn run_schedule_parallel(
    schedule: &[String],
    task_index: &HashMap<String, TaskRecord>,
    options: &RunAllOptions,
    wave_meta_map: &HashMap<String, TaskWaveMeta>,
) -> Result<RunAllSummary, String> {
    let available = available_pool(&options.backend_pool);
    if available.is_empty() {
        return Err("task run-all: no available backend from --backend-pool".to_string());
    }
    let mut pending: Vec<PendingLaunch> = schedule
        .iter()
        .enumerate()
        .map(|(idx, id)| {
            let wave = wave_meta_map
                .get(id)
                .cloned()
                .unwrap_or_else(|| fallback_wave_meta(idx));
            let requested_backend =
                choose_backend_for_task(task_index.get(id), &options.backend_pool, idx);
            PendingLaunch {
                id: id.clone(),
                backend: fallback_backend(requested_backend.clone(), &available)
                    .unwrap_or_else(|| available[0].clone()),
                requested_backend,
                queue_since: Instant::now(),
                queue_started_at: utc_now_iso(),
                wave_index: wave.index,
                wave_mode: wave.mode,
                wave_size: wave.size,
            }
        })
        .collect();
    for launch in &pending {
        emit_pending_event(options, launch);
    }
    let mut active: Vec<ActiveLaunch> = Vec::new();
    let mut backend_active: HashMap<String, usize> = HashMap::new();
    let mut summary = RunAllSummary::default();
    let mut next_worker = 1usize;
    let total = schedule.len();
    let mut launched = 0usize;
    let mut completed = 0usize;

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
            let start_wave = TaskWaveMeta {
                index: launch.wave_index,
                mode: launch.wave_mode.clone(),
                size: launch.wave_size,
            };
            emit_start_event(
                options,
                &launch.id,
                &launch.backend,
                launch.requested_backend.as_deref(),
                queue_ms,
                &start_wave,
            );
            let worker_id = format!("w{next_worker}");
            next_worker = if next_worker >= options.max_workers {
                1
            } else {
                next_worker + 1
            };
            launched += 1;
            emit_progress(&format!(
                "cxrs task run-all: launch [{launched}/{total}] task={} backend={} active={} pending={}",
                launch.id,
                launch.backend,
                active.len() + 1,
                pending.len()
            ));
            *backend_active.entry(launch.backend.clone()).or_insert(0) += 1;
            let id = launch.id.clone();
            let backend = launch.backend.clone();
            let wave_index = launch.wave_index;
            let wave_mode = launch.wave_mode.clone();
            let wave_size = launch.wave_size;
            let task_parent_id = task_index.get(&id).and_then(|t| t.parent_id.clone());
            let max_retries = task_index.get(&id).and_then(|t| t.max_retries).unwrap_or(0);
            let join = thread::spawn(move || {
                run_task_managed_subprocess(
                    id,
                    backend,
                    LaunchEnvMeta {
                        queue_ms,
                        queue_started_at: launch.queue_started_at,
                        worker_id,
                        task_parent_id,
                        max_retries,
                        wave: Some(TaskWaveMeta {
                            index: wave_index,
                            mode: wave_mode,
                            size: wave_size,
                        }),
                    },
                )
            });
            active.push(ActiveLaunch {
                id: launch.id,
                backend: launch.backend,
                requested_backend: launch.requested_backend,
                queue_ms,
                wave_index,
                wave_mode: launch.wave_mode,
                wave_size,
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
                    completed += 1;
                    if code == 0 {
                        summary.record_success();
                        let _ = set_task_status_quiet(&done.id, "complete");
                        emit_progress(&format!(
                            "cxrs task run-all: done [{completed}/{total}] task={} backend={} status=complete",
                            done.id, done.backend
                        ));
                        let event = TaskRunEvent {
                            id: done.id,
                            backend: done.backend,
                            requested_backend: done.requested_backend,
                            status: "complete".to_string(),
                            execution_id,
                            failure_class: None,
                            queue_ms: done.queue_ms,
                            wave_index: done.wave_index,
                            wave_mode: done.wave_mode,
                            wave_size: done.wave_size,
                        };
                        emit_result_event(options, &event);
                        summary.add_task_run(event);
                    } else {
                        let failure = classify_failure_for_execution(execution_id.as_deref());
                        summary.record_failure(failure.class);
                        let _ = set_task_status_quiet(&done.id, "failed");
                        emit_progress(&format!(
                            "cxrs task run-all: done [{completed}/{total}] task={} backend={} status=failed reason={}",
                            done.id, done.backend, failure.reason
                        ));
                        crate::cx_eprintln!(
                            "{} task run-all: task failed: {}",
                            cli_app_name(),
                            done.id
                        );
                        let event = TaskRunEvent {
                            id: done.id,
                            backend: done.backend,
                            requested_backend: done.requested_backend,
                            status: "failed".to_string(),
                            execution_id,
                            failure_class: Some(failure.reason),
                            queue_ms: done.queue_ms,
                            wave_index: done.wave_index,
                            wave_mode: done.wave_mode,
                            wave_size: done.wave_size,
                        };
                        emit_result_event(options, &event);
                        summary.add_task_run(event);
                    }
                }
                Err(e) => {
                    completed += 1;
                    summary.record_critical_error();
                    let _ = set_task_status_quiet(&done.id, "failed");
                    emit_progress(&format!(
                        "cxrs task run-all: done [{completed}/{total}] task={} backend={} status=critical_error",
                        done.id, done.backend
                    ));
                    crate::cx_eprintln!(
                        "{} task run-all: critical error for {}: {e}",
                        cli_app_name(),
                        done.id
                    );
                    let event = TaskRunEvent {
                        id: done.id,
                        backend: done.backend,
                        requested_backend: done.requested_backend,
                        status: "critical_error".to_string(),
                        execution_id: None,
                        failure_class: Some("critical_error".to_string()),
                        queue_ms: done.queue_ms,
                        wave_index: done.wave_index,
                        wave_mode: done.wave_mode,
                        wave_size: done.wave_size,
                    };
                    emit_result_event(options, &event);
                    summary.add_task_run(event);
                    if options.halt_on_critical {
                        summary.halted_on_critical = true;
                        return Ok(summary);
                    }
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
                    crate::cx_eprintln!("{} task run-plan: invalid status '{v}'", cli_app_name());
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
                crate::cx_eprintln!("{} task run-plan: unknown flag '{other}'", cli_app_name());
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
                crate::cx_eprintln!(
                    "{} task run-plan: failed to render json: {e}",
                    cli_app_name()
                );
                return 1;
            }
        }
        return if plan.blocked.is_empty() { 0 } else { 1 };
    }

    println!("== {} task run-plan ==", cli_app_name());
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

fn rec_mode(
    plan: &crate::tasks_plan::TaskRunPlan,
    strict_ok: bool,
) -> (&'static str, &'static str) {
    if !plan.blocked.is_empty() {
        return ("sequential", "blocked_tasks_present");
    }
    if strict_ok {
        return ("parallel", "single_parallel_wave_ready");
    }
    let has_parallel = plan.waves.iter().any(|w| w.mode == "parallel");
    if has_parallel && plan.waves.len() > 1 {
        return ("mixed", "multi_wave_or_lock_limited");
    }
    ("sequential", "sequential_or_small_scope")
}

fn plan_mode_counts(plan: &crate::tasks_plan::TaskRunPlan) -> (u64, u64, u64) {
    let sequential_waves = plan.waves.iter().filter(|w| w.mode != "parallel").count() as u64;
    let parallel_waves = plan.waves.iter().filter(|w| w.mode == "parallel").count() as u64;
    let largest_parallel_wave = plan
        .waves
        .iter()
        .filter(|w| w.mode == "parallel")
        .map(|w| w.task_ids.len() as u64)
        .max()
        .unwrap_or(0);
    (sequential_waves, parallel_waves, largest_parallel_wave)
}

pub(crate) fn task_readiness_value(tasks: &[TaskRecord], status_filter: &str) -> serde_json::Value {
    let plan = build_task_run_plan(tasks, status_filter);
    let strict_reason = strict_issue_parallel(&plan);
    let strict_ok = strict_reason.is_none();
    let (recommended_mode, recommended_reason) = rec_mode(&plan, strict_ok);
    let blocked_total = plan.blocked.len();
    let blocked_deps = plan
        .blocked
        .iter()
        .filter(|b| b.reason.starts_with("unresolved dependencies"))
        .count();
    let blocked_resources = plan
        .blocked
        .iter()
        .filter(|b| b.reason.to_lowercase().contains("resource"))
        .count();
    let can_run = blocked_total == 0;
    let can_run_mixed = can_run;
    let can_run_parallel = can_run && strict_ok;
    let (sequential_waves, parallel_waves, largest_parallel_wave) = plan_mode_counts(&plan);

    serde_json::json!({
        "status_filter": plan.status_filter,
        "selected": plan.selected,
        "waves": plan.waves.len(),
        "blocked_total": blocked_total,
        "blocked_dependencies": blocked_deps,
        "blocked_resources": blocked_resources,
        "can_run": can_run,
        "can_run_mixed": can_run_mixed,
        "can_run_parallel": can_run_parallel,
        "strict_plan_ok": strict_ok,
        "strict_plan_reason": strict_reason,
        "sequential_waves": sequential_waves,
        "parallel_waves": parallel_waves,
        "largest_parallel_wave": largest_parallel_wave,
        "recommended_mode": recommended_mode,
        "recommended_reason": recommended_reason,
        "blocked": plan.blocked
    })
}

fn list_readiness_value(
    tasks: &[TaskRecord],
    filtered: &[TaskRecord],
    status_filter: Option<&str>,
) -> serde_json::Value {
    let include_pending_plan = status_filter.is_none() || status_filter == Some("pending");
    let plan = if include_pending_plan {
        Some(build_task_run_plan(tasks, "pending"))
    } else {
        None
    };
    let mut runnable_now = 0usize;
    let mut blocked_now = 0usize;
    let mut inspect_only = 0usize;

    for task in filtered {
        let readiness = match plan.as_ref() {
            Some(plan) => task_run_view(task, tasks, plan),
            None => task_run_state(task, tasks),
        };
        match readiness.get("runnable_now").and_then(Value::as_bool) {
            Some(true) => runnable_now += 1,
            Some(false) if task.status == "pending" => blocked_now += 1,
            _ => inspect_only += 1,
        }
    }

    let next_wave = plan
        .as_ref()
        .and_then(|plan| plan.waves.first())
        .map(|wave| {
            serde_json::json!({
                "index": wave.index,
                "mode": wave.mode,
                "size": wave.task_ids.len()
            })
        });

    serde_json::json!({
        "selected_count": filtered.len(),
        "runnable_now_count": runnable_now,
        "blocked_now_count": blocked_now,
        "inspect_only_count": inspect_only,
        "wave_count": plan.as_ref().map(|plan| plan.waves.len()).unwrap_or(0),
        "blocked_count": plan.as_ref().map(|plan| plan.blocked.len()).unwrap_or(0),
        "next_wave": next_wave,
    })
}

fn handle_task_check(app_name: &str, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let usage = format!(
        "Usage: {app_name} task check [--status pending|in_progress|complete|failed] [--strict-plan] [--json|--text]"
    );
    let mut status_filter = "pending".to_string();
    let mut strict_plan = false;
    let mut as_json: Option<bool> = None;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return 2;
                };
                if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                    crate::cx_eprintln!("{} task check: invalid status '{v}'", cli_app_name());
                    return 2;
                }
                status_filter = v.to_string();
                i += 2;
            }
            "--strict-plan" => {
                strict_plan = true;
                i += 1;
            }
            "--json" => {
                as_json = Some(true);
                i += 1;
            }
            "--text" => {
                as_json = Some(false);
                i += 1;
            }
            other => {
                crate::cx_eprintln!("{} task check: unknown flag '{other}'", cli_app_name());
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
    let readiness = task_readiness_value(&tasks, &status_filter);
    let can_run = readiness
        .get("can_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let strict_ok = readiness
        .get("strict_plan_ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let out_json = resolve_json_mode(as_json, false);
    if out_json {
        let payload = serde_json::json!({
        "contract_version": TASK_CHECK_JSON_CONTRACT_VERSION,
            "strict_plan": strict_plan,
            "status_filter": readiness.get("status_filter").cloned().unwrap_or(serde_json::Value::String(status_filter.clone())),
            "selected": readiness.get("selected").cloned().unwrap_or(serde_json::Value::from(0)),
            "waves": readiness.get("waves").cloned().unwrap_or(serde_json::Value::from(0)),
            "blocked_total": readiness.get("blocked_total").cloned().unwrap_or(serde_json::Value::from(0)),
            "blocked_dependencies": readiness.get("blocked_dependencies").cloned().unwrap_or(serde_json::Value::from(0)),
            "blocked_resources": readiness.get("blocked_resources").cloned().unwrap_or(serde_json::Value::from(0)),
            "can_run": readiness.get("can_run").cloned().unwrap_or(serde_json::Value::Bool(false)),
            "can_run_mixed": readiness.get("can_run_mixed").cloned().unwrap_or(serde_json::Value::Bool(false)),
            "can_run_parallel": readiness.get("can_run_parallel").cloned().unwrap_or(serde_json::Value::Bool(false)),
            "strict_plan_ok": readiness.get("strict_plan_ok").cloned().unwrap_or(serde_json::Value::Bool(false)),
            "strict_plan_reason": readiness.get("strict_plan_reason").cloned().unwrap_or(serde_json::Value::Null),
            "sequential_waves": readiness.get("sequential_waves").cloned().unwrap_or(serde_json::Value::from(0)),
            "parallel_waves": readiness.get("parallel_waves").cloned().unwrap_or(serde_json::Value::from(0)),
            "largest_parallel_wave": readiness.get("largest_parallel_wave").cloned().unwrap_or(serde_json::Value::from(0)),
            "recommended_mode": readiness.get("recommended_mode").cloned().unwrap_or(serde_json::Value::String("sequential".to_string())),
            "recommended_reason": readiness.get("recommended_reason").cloned().unwrap_or(serde_json::Value::String("unknown".to_string())),
            "blocked": readiness.get("blocked").cloned().unwrap_or(serde_json::Value::Array(Vec::new()))
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("{} task check: failed to render json: {e}", cli_app_name());
                return 1;
            }
        }
    } else {
        println!("== {} task check ==", cli_app_name());
        println!(
            "status_filter: {}",
            readiness
                .get("status_filter")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&status_filter)
        );
        println!(
            "selected: {}",
            readiness
                .get("selected")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "waves: {}",
            readiness
                .get("waves")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "blocked_total: {}",
            readiness
                .get("blocked_total")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "blocked_dependencies: {}",
            readiness
                .get("blocked_dependencies")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "blocked_resources: {}",
            readiness
                .get("blocked_resources")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        );
        println!("can_run: {can_run}");
        println!(
            "can_run_mixed: {}",
            readiness
                .get("can_run_mixed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        );
        println!(
            "can_run_parallel: {}",
            readiness
                .get("can_run_parallel")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        );
        println!("strict_plan: {strict_plan}");
        println!("strict_plan_ok: {strict_ok}");
        if let Some(reason) = readiness
            .get("strict_plan_reason")
            .and_then(serde_json::Value::as_str)
        {
            println!("strict_plan_reason: {reason}");
        }
        println!(
            "sequential_waves: {}",
            readiness
                .get("sequential_waves")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "parallel_waves: {}",
            readiness
                .get("parallel_waves")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "largest_parallel_wave: {}",
            readiness
                .get("largest_parallel_wave")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "recommended_mode: {}",
            readiness
                .get("recommended_mode")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("sequential")
        );
        println!(
            "recommended_reason: {}",
            readiness
                .get("recommended_reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
        );
    }

    if strict_plan {
        if strict_ok && can_run { 0 } else { 1 }
    } else if can_run {
        0
    } else {
        1
    }
}

pub fn handler(ctx: &CmdCtx, args: &[String], deps: &TaskCmdDeps) -> i32 {
    let app_name = &ctx.app_name;
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "add" => (deps.cmd_task_add)(app_name, &args[1..]),
        "list" => handle_list(app_name, args, deps),
        "show" => {
            if args.len() == 1 || args.get(1).map(String::as_str) == Some("list") {
                let mut list_args = vec!["list".to_string()];
                list_args.extend(args.iter().skip(2).cloned());
                return handle_list(app_name, &list_args, deps);
            }
            match require_id(app_name, args, "show") {
                Ok(id) => (deps.cmd_task_show)(&id),
                Err(code) => code,
            }
        }
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
        "check" => handle_task_check(app_name, args, deps),
        "sandbox" => handle_task_sandbox(app_name, args),
        "events" => cmd_task_events(app_name, args),
        "run-plan" => handle_run_plan(app_name, args, deps),
        "run" => handle_run(app_name, args, deps),
        "run-all" => handle_run_all(app_name, args, deps),
        _ => {
            crate::cx_eprintln!(
                "Usage: {app_name} task <add|list|show|claim|complete|fail|fanout|check|sandbox|events|run-plan|run|run-all> ..."
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
            "primary,ollama".to_string(),
            "--backend-cap".to_string(),
            "primary=2".to_string(),
            "--max-workers".to_string(),
            "3".to_string(),
            "--fairness".to_string(),
            "least_loaded".to_string(),
            "--json".to_string(),
        ];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert_eq!(opts.run_mode, "mixed");
        assert!(opts.backend_pool.iter().any(|b| b == "primary"));
        assert!(opts.backend_pool.iter().any(|b| b == "ollama"));
        assert_eq!(opts.backend_caps.get("primary"), Some(&2usize));
        assert_eq!(opts.max_workers, 3);
        assert_eq!(opts.fairness, "least_loaded");
        assert!(opts.as_json);
        assert!(!opts.halt_on_critical);
        assert_eq!(opts.summary_format, "text");
    }

    #[test]
    fn parse_run_all_options_accepts_critical_policy_flags() {
        let args = vec![
            "run-all".to_string(),
            "--halt-on-critical".to_string(),
            "--continue-on-critical".to_string(),
        ];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert!(!opts.halt_on_critical);

        let args = vec!["run-all".to_string(), "--halt-on-critical".to_string()];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert!(opts.halt_on_critical);
    }

    #[test]
    fn parse_mode_parallel() {
        let args = vec![
            "run-all".to_string(),
            "--mode".to_string(),
            "parallel".to_string(),
        ];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert_eq!(opts.run_mode, "parallel");
    }

    #[test]
    fn parse_strict_plan() {
        let args = vec!["run-all".to_string(), "--strict-plan".to_string()];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert!(opts.strict_plan);
    }

    #[test]
    fn parse_plan_json() {
        let args = vec!["run-all".to_string(), "--plan-json".to_string()];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert!(opts.plan_json);
    }

    #[test]
    fn parse_dry_run() {
        let args = vec!["run-all".to_string(), "--dry-run".to_string()];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert!(opts.dry_run);
    }

    #[test]
    fn parse_json_text() {
        let prev = std::env::var("CX_JSON_DEFAULT").ok();
        unsafe { std::env::set_var("CX_JSON_DEFAULT", "1") };
        let args = vec!["run-all".to_string()];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert!(opts.as_json);

        let args = vec!["run-all".to_string(), "--text".to_string()];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert!(!opts.as_json);
        match prev {
            Some(v) => unsafe { std::env::set_var("CX_JSON_DEFAULT", v) },
            None => unsafe { std::env::remove_var("CX_JSON_DEFAULT") },
        }
    }

    #[test]
    fn parse_summary_json() {
        let args = vec![
            "run-all".to_string(),
            "--summary".to_string(),
            "json".to_string(),
        ];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert_eq!(opts.summary_format, "json");
    }

    #[test]
    fn parse_events_jsonl() {
        let args = vec!["run-all".to_string(), "--events-jsonl".to_string()];
        let opts = parse_run_all_options("cx", &args).expect("parse options");
        assert!(opts.events_jsonl);
    }

    #[test]
    fn choose_backend_prefers_task_backend_when_in_pool() {
        let task = mk_task("ollama");
        let selected = choose_backend_for_task(
            Some(&task),
            &["primary".to_string(), "ollama".to_string()],
            0,
        );
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

use std::fs::File;
use std::io::Read;

use crate::config::{cli_app_name, command_with_cli};
use crate::contract_versions::TASK_SHOW_JSON_CONTRACT_VERSION;
use crate::execmeta::utc_now_iso;
use crate::paths::{resolve_log_file, resolve_tasks_file};
use crate::state::write_json_atomic;
use crate::tasks_plan::{TaskRunPlan, build_task_run_plan};
use crate::types::TaskRecord;
use serde_json::Value;

#[path = "tasks_fanout.rs"]
mod tasks_fanout;
pub use tasks_fanout::cmd_task_fanout;

pub fn task_role_valid(role: &str) -> bool {
    matches!(
        role,
        "architect" | "implementer" | "reviewer" | "tester" | "doc"
    )
}

pub fn read_tasks() -> Result<Vec<TaskRecord>, String> {
    let path = resolve_tasks_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut s = String::new();
    File::open(&path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?
        .read_to_string(&mut s)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if s.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<TaskRecord>>(&s)
        .map_err(|e| format!("invalid JSON in {}: {e}", path.display()))
}

pub fn write_tasks(tasks: &[TaskRecord]) -> Result<(), String> {
    let path = resolve_tasks_file()?;
    let value = serde_json::to_value(tasks).map_err(|e| format!("failed to encode tasks: {e}"))?;
    write_json_atomic(&path, &value)
}

pub fn next_task_id(tasks: &[TaskRecord]) -> String {
    let mut max_id = 0u64;
    for t in tasks {
        if let Some(num) =
            t.id.strip_prefix("task_")
                .and_then(|v| v.parse::<u64>().ok())
            && num > max_id
        {
            max_id = num;
        }
    }
    format!("task_{:03}", max_id + 1)
}

struct AddArgs {
    objective: String,
    role: String,
    parent_id: Option<String>,
    context_ref: String,
    backend: String,
    model: Option<String>,
    profile: String,
    converge: String,
    replicas: u32,
    max_concurrency: Option<u32>,
    run_mode: String,
    depends_on: Vec<String>,
    resource_keys: Vec<String>,
    max_retries: Option<u32>,
    timeout_secs: Option<u64>,
}

fn parse_objective_prefix(app_name: &str, args: &[String]) -> Result<(String, usize), i32> {
    if args.is_empty() {
        crate::cx_eprintln!(
            "Usage: {app_name} task add <objective> [--role <role>] [--parent <id>] [--context <ref>] [--backend <auto|primary|ollama|llamacpp|mlx>] [--model <name>] [--profile <fast|balanced|quality|schema_strict>] [--converge <none|first_valid|majority|judge|score>] [--replicas <n>] [--max-concurrency <n>] [--mode <sequential|parallel>] [--depends-on <id1,id2>] [--resource <key>] [--resource-keys <k1,k2>] [--max-retries <n>] [--timeout-secs <n>]"
        );
        return Err(2);
    }
    let mut obj_parts: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < args.len() && !args[i].starts_with("--") {
        obj_parts.push(args[i].clone());
        i += 1;
    }
    let objective = obj_parts.join(" ").trim().to_string();
    if objective.is_empty() {
        crate::cx_eprintln!("{} task add: objective cannot be empty", cli_app_name());
        return Err(2);
    }
    Ok((objective, i))
}

fn parse_csv_list(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

type AddFlagsParsed = (
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    String,
    String,
    u32,
    Option<u32>,
    String,
    Vec<String>,
    Vec<String>,
    Option<u32>,
    Option<u64>,
);

fn parse_add_flags(args: &[String], mut i: usize) -> Result<AddFlagsParsed, i32> {
    let mut role = "implementer".to_string();
    let mut parent_id: Option<String> = None;
    let mut context_ref = String::new();
    let mut backend = "auto".to_string();
    let mut model: Option<String> = None;
    let mut profile = "balanced".to_string();
    let mut converge = "none".to_string();
    let mut replicas: u32 = 1;
    let mut max_concurrency: Option<u32> = None;
    let mut run_mode = "sequential".to_string();
    let mut depends_on: Vec<String> = Vec::new();
    let mut resource_keys: Vec<String> = Vec::new();
    let mut max_retries: Option<u32> = None;
    let mut timeout_secs: Option<u64> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--role" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{} task add: --role requires a value", cli_app_name());
                    return Err(2);
                };
                role = v.to_lowercase();
                i += 2;
            }
            "--parent" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{} task add: --parent requires a value", cli_app_name());
                    return Err(2);
                };
                parent_id = Some(v.to_string());
                i += 2;
            }
            "--context" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{} task add: --context requires a value", cli_app_name());
                    return Err(2);
                };
                context_ref = v.to_string();
                i += 2;
            }
            "--backend" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{} task add: --backend requires a value", cli_app_name());
                    return Err(2);
                };
                let b = match v.trim().to_lowercase().as_str() {
                    "llama.cpp" | "llama_cpp" => "llamacpp".to_string(),
                    other => other.to_string(),
                };
                if !matches!(
                    b.as_str(),
                    "auto" | "primary" | "ollama" | "llamacpp" | "mlx"
                ) {
                    crate::cx_eprintln!("{} task add: invalid --backend '{b}'", cli_app_name());
                    return Err(2);
                }
                backend = b;
                i += 2;
            }
            "--model" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{} task add: --model requires a value", cli_app_name());
                    return Err(2);
                };
                let trimmed = v.trim();
                model = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                i += 2;
            }
            "--profile" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{} task add: --profile requires a value", cli_app_name());
                    return Err(2);
                };
                let p = v.trim().to_lowercase();
                if !matches!(
                    p.as_str(),
                    "fast" | "balanced" | "quality" | "schema_strict"
                ) {
                    crate::cx_eprintln!("{} task add: invalid --profile '{p}'", cli_app_name());
                    return Err(2);
                }
                profile = p;
                i += 2;
            }
            "--converge" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{} task add: --converge requires a value", cli_app_name());
                    return Err(2);
                };
                let c = v.trim().to_lowercase();
                if !matches!(
                    c.as_str(),
                    "none" | "first_valid" | "majority" | "judge" | "score"
                ) {
                    crate::cx_eprintln!("{} task add: invalid --converge '{c}'", cli_app_name());
                    return Err(2);
                }
                converge = c;
                i += 2;
            }
            "--replicas" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{} task add: --replicas requires a value", cli_app_name());
                    return Err(2);
                };
                let Ok(n) = v.parse::<u32>() else {
                    crate::cx_eprintln!(
                        "{} task add: --replicas must be an integer",
                        cli_app_name()
                    );
                    return Err(2);
                };
                if n == 0 {
                    crate::cx_eprintln!("{} task add: --replicas must be >= 1", cli_app_name());
                    return Err(2);
                }
                replicas = n;
                i += 2;
            }
            "--max-concurrency" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!(
                        "{} task add: --max-concurrency requires a value",
                        cli_app_name()
                    );
                    return Err(2);
                };
                let Ok(n) = v.parse::<u32>() else {
                    crate::cx_eprintln!(
                        "{} task add: --max-concurrency must be an integer",
                        cli_app_name()
                    );
                    return Err(2);
                };
                if n == 0 {
                    crate::cx_eprintln!(
                        "{} task add: --max-concurrency must be >= 1",
                        cli_app_name()
                    );
                    return Err(2);
                }
                max_concurrency = Some(n);
                i += 2;
            }
            "--mode" => {
                let Some(v) = args.get(i + 1).map(|s| s.as_str()) else {
                    crate::cx_eprintln!("{} task add: --mode requires a value", cli_app_name());
                    return Err(2);
                };
                if !matches!(v, "sequential" | "parallel") {
                    crate::cx_eprintln!("{} task add: invalid --mode '{v}'", cli_app_name());
                    return Err(2);
                }
                run_mode = v.to_string();
                i += 2;
            }
            "--depends-on" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!(
                        "{} task add: --depends-on requires a value",
                        cli_app_name()
                    );
                    return Err(2);
                };
                depends_on.extend(parse_csv_list(v));
                i += 2;
            }
            "--resource" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{} task add: --resource requires a value", cli_app_name());
                    return Err(2);
                };
                if !v.trim().is_empty() {
                    resource_keys.push(v.trim().to_string());
                }
                i += 2;
            }
            "--resource-keys" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!(
                        "{} task add: --resource-keys requires a value",
                        cli_app_name()
                    );
                    return Err(2);
                };
                resource_keys.extend(parse_csv_list(v));
                i += 2;
            }
            "--max-retries" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!(
                        "{} task add: --max-retries requires a value",
                        cli_app_name()
                    );
                    return Err(2);
                };
                let Ok(n) = v.parse::<u32>() else {
                    crate::cx_eprintln!(
                        "{} task add: --max-retries must be an integer",
                        cli_app_name()
                    );
                    return Err(2);
                };
                max_retries = Some(n);
                i += 2;
            }
            "--timeout-secs" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!(
                        "{} task add: --timeout-secs requires a value",
                        cli_app_name()
                    );
                    return Err(2);
                };
                let Ok(n) = v.parse::<u64>() else {
                    crate::cx_eprintln!(
                        "{} task add: --timeout-secs must be an integer",
                        cli_app_name()
                    );
                    return Err(2);
                };
                timeout_secs = Some(n);
                i += 2;
            }
            other => {
                crate::cx_eprintln!("{} task add: unknown flag '{other}'", cli_app_name());
                return Err(2);
            }
        }
    }
    depends_on.sort();
    depends_on.dedup();
    resource_keys.sort();
    resource_keys.dedup();
    Ok((
        role,
        parent_id,
        context_ref,
        backend,
        model,
        profile,
        converge,
        replicas,
        max_concurrency,
        run_mode,
        depends_on,
        resource_keys,
        max_retries,
        timeout_secs,
    ))
}

fn parse_task_add_args(app_name: &str, args: &[String]) -> Result<AddArgs, i32> {
    let (objective, i) = parse_objective_prefix(app_name, args)?;
    let (
        role,
        parent_id,
        context_ref,
        backend,
        model,
        profile,
        converge,
        replicas,
        max_concurrency,
        run_mode,
        depends_on,
        resource_keys,
        max_retries,
        timeout_secs,
    ) = parse_add_flags(args, i)?;
    if !task_role_valid(&role) {
        crate::cx_eprintln!("{} task add: invalid role '{role}'", cli_app_name());
        return Err(2);
    }
    Ok(AddArgs {
        objective,
        role,
        parent_id,
        context_ref,
        backend,
        model,
        profile,
        converge,
        replicas,
        max_concurrency,
        run_mode,
        depends_on,
        resource_keys,
        max_retries,
        timeout_secs,
    })
}

pub fn cmd_task_add(app_name: &str, args: &[String]) -> i32 {
    let parsed = match parse_task_add_args(app_name, args) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let mut tasks = match read_tasks() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            return 1;
        }
    };
    let id = next_task_id(&tasks);
    let now = utc_now_iso();
    tasks.push(TaskRecord {
        id: id.clone(),
        parent_id: parsed.parent_id,
        role: parsed.role,
        objective: parsed.objective,
        context_ref: parsed.context_ref,
        backend: parsed.backend,
        model: parsed.model,
        profile: parsed.profile,
        converge: parsed.converge,
        replicas: parsed.replicas,
        max_concurrency: parsed.max_concurrency,
        run_mode: parsed.run_mode,
        depends_on: parsed.depends_on,
        resource_keys: parsed.resource_keys,
        max_retries: parsed.max_retries,
        timeout_secs: parsed.timeout_secs,
        status: "pending".to_string(),
        created_at: now.clone(),
        updated_at: now,
    });
    if let Err(e) = write_tasks(&tasks) {
        crate::cx_eprintln!("{} task add: {e}", cli_app_name());
        return 1;
    }
    println!("{id}");
    0
}

pub fn cmd_task_list(status_filter: Option<&str>) -> i32 {
    let tasks = match read_tasks() {
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
    if filtered.is_empty() {
        println!("No tasks.");
        return 0;
    }
    let plan = build_task_run_plan(&tasks, "pending");
    let readiness_rows: Vec<Value> = filtered
        .iter()
        .map(|task| task_run_view(task, &tasks, &plan))
        .collect();
    let runnable_now = readiness_rows
        .iter()
        .filter(|row| row.get("runnable_now").and_then(Value::as_bool) == Some(true))
        .count();
    let blocked_now = filtered
        .iter()
        .zip(readiness_rows.iter())
        .filter(|(task, row)| {
            task.status == "pending"
                && row.get("runnable_now").and_then(Value::as_bool) == Some(false)
        })
        .count();
    let inspect_only = filtered.len().saturating_sub(runnable_now + blocked_now);
    let next_wave = plan.waves.first();
    println!(
        "list_readiness: selected={} runnable_now={} blocked_now={} inspect_only={} waves={} blocked={}",
        filtered.len(),
        runnable_now,
        blocked_now,
        inspect_only,
        plan.waves.len(),
        plan.blocked.len()
    );
    if let Some(wave) = next_wave {
        println!(
            "list_readiness_next_wave: index={} mode={} size={}",
            wave.index,
            wave.mode,
            wave.task_ids.len()
        );
    }
    println!("id | role | status | parent_id | objective");
    println!("---|---|---|---|---");
    for t in filtered {
        println!(
            "{} | {} | {} | {} | {}",
            t.id,
            t.role,
            t.status,
            t.parent_id.unwrap_or_else(|| "-".to_string()),
            t.objective
        );
    }
    0
}

pub fn cmd_task_show(id: &str) -> i32 {
    let tasks = match read_tasks() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            return 1;
        }
    };
    let Some(task) = tasks.iter().find(|t| t.id == id).cloned() else {
        crate::cx_eprintln!("{} task show: task not found: {id}", cli_app_name());
        return 1;
    };
    let plan = build_task_run_plan(&tasks, "pending");
    let mut out = match serde_json::to_value(&task) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} task show: render failed: {e}", cli_app_name());
            return 1;
        }
    };
    if let Some(obj) = out.as_object_mut() {
        obj.insert(
            "contract_version".to_string(),
            Value::String(TASK_SHOW_JSON_CONTRACT_VERSION.to_string()),
        );
        obj.insert("latest_run".to_string(), task_run_latest(id));
        obj.insert(
            "run_readiness".to_string(),
            task_run_view(&task, &tasks, &plan),
        );
    }
    match serde_json::to_string_pretty(&out) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!("{} task show: render failed: {e}", cli_app_name());
            1
        }
    }
}

fn task_effective_dependencies(task: &TaskRecord) -> Vec<String> {
    if !task.depends_on.is_empty() {
        return task.depends_on.clone();
    }
    task.parent_id
        .as_ref()
        .map(|v| vec![v.clone()])
        .unwrap_or_default()
}

fn task_lock_keys(task: &TaskRecord) -> Vec<String> {
    if !task.resource_keys.is_empty() {
        return task.resource_keys.clone();
    }
    if task.run_mode == "parallel" {
        return vec!["repo:write".to_string()];
    }
    Vec::new()
}

pub fn task_run_view(task: &TaskRecord, tasks: &[TaskRecord], plan: &TaskRunPlan) -> Value {
    let dependencies = task_effective_dependencies(task);
    let resource_keys = task_lock_keys(task);
    if task.status != "pending" {
        return serde_json::json!({
            "status_filter": "pending",
            "selected_status_count": tasks.iter().filter(|t| t.status == "pending").count(),
            "runnable_now": false,
            "wave_index": Value::Null,
            "wave_mode": Value::Null,
            "blocked_reason": format!("task status is {}", task.status),
            "dependencies": dependencies,
            "resource_keys": resource_keys,
            "recommended_command": format!("{} {}", command_with_cli("task show"), task.id),
            "recommended_reason": "inspect_non_pending_task"
        });
    }

    let complete_ids = tasks
        .iter()
        .filter(|t| t.status == "complete")
        .map(|t| t.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let unresolved_dependencies: Vec<String> = dependencies
        .iter()
        .filter(|dep| !complete_ids.contains(dep.as_str()))
        .cloned()
        .collect();
    let wave = plan.waves.iter().find_map(|wave| {
        if wave.task_ids.iter().any(|tid| tid == &task.id) {
            Some((wave.index, wave.mode.clone()))
        } else {
            None
        }
    });
    let blocked_reason = if !unresolved_dependencies.is_empty() {
        Some(format!(
            "unresolved dependencies: {}",
            unresolved_dependencies.join(", ")
        ))
    } else {
        plan.blocked
            .iter()
            .find(|blocked| blocked.id == task.id)
            .map(|blocked| blocked.reason.clone())
    };

    let runnable_now =
        wave.as_ref().map(|(idx, _)| *idx == 1).unwrap_or(false) && blocked_reason.is_none();
    let (recommended_command, recommended_reason) = if let Some(reason) = blocked_reason.as_ref() {
        let _ = reason;
        (
            command_with_cli("task check --json"),
            "inspect_blockers".to_string(),
        )
    } else if let Some((wave_index, _)) = wave.as_ref() {
        if *wave_index == 1 {
            (
                format!("{} {}", command_with_cli("task run"), task.id),
                "task_is_ready_now".to_string(),
            )
        } else {
            (
                command_with_cli("task run-all --status pending --dry-run --json"),
                "task_is_scheduled_in_later_wave".to_string(),
            )
        }
    } else {
        (
            command_with_cli("task check --json"),
            "inspect_schedule_gap".to_string(),
        )
    };

    serde_json::json!({
        "status_filter": "pending",
        "selected_status_count": plan.selected,
        "runnable_now": runnable_now,
        "wave_index": wave.as_ref().map(|(idx, _)| *idx),
        "wave_mode": wave.as_ref().map(|(_, mode)| mode.clone()),
        "blocked_reason": blocked_reason,
        "dependencies": dependencies,
        "resource_keys": resource_keys,
        "recommended_command": recommended_command,
        "recommended_reason": recommended_reason
    })
}

pub fn task_run_state(task: &TaskRecord, tasks: &[TaskRecord]) -> Value {
    let plan = build_task_run_plan(tasks, "pending");
    task_run_view(task, tasks, &plan)
}

fn task_run_latest(id: &str) -> Value {
    let Some(path) = resolve_log_file() else {
        return Value::Null;
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return Value::Null;
    };
    for line in content.lines().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("task_id").and_then(Value::as_str) != Some(id) {
            continue;
        }
        return serde_json::json!({
            "execution_id": v.get("execution_id").and_then(Value::as_str),
            "timestamp": v.get("timestamp").and_then(Value::as_str),
            "tool": v.get("tool").and_then(Value::as_str),
            "backend_used": v.get("backend_used").and_then(Value::as_str),
            "execution_lane": v.get("execution_lane").and_then(Value::as_str),
            "execution_lane_detail": v.get("execution_lane_detail").and_then(Value::as_str),
            "execution_mode": v.get("execution_mode").and_then(Value::as_str),
            "duration_ms": v.get("duration_ms").and_then(Value::as_u64),
            "schema_valid": v.get("schema_valid").and_then(Value::as_bool),
            "timed_out": v.get("timed_out").and_then(Value::as_bool),
            "policy_blocked": v.get("policy_blocked").and_then(Value::as_bool)
        });
    }
    Value::Null
}

pub fn set_task_status(id: &str, new_status: &str) -> Result<(), String> {
    let mut tasks = read_tasks()?;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return Err(format!("{} task: task not found: {id}", cli_app_name()));
    };
    task.status = new_status.to_string();
    task.updated_at = utc_now_iso();
    write_tasks(&tasks)
}

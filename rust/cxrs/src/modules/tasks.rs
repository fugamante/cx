use std::fs::File;
use std::io::Read;

use crate::execmeta::utc_now_iso;
use crate::paths::resolve_tasks_file;
use crate::state::write_json_atomic;
use crate::types::TaskRecord;

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
    run_mode: String,
    depends_on: Vec<String>,
    resource_keys: Vec<String>,
    max_retries: Option<u32>,
    timeout_secs: Option<u64>,
}

fn parse_objective_prefix(app_name: &str, args: &[String]) -> Result<(String, usize), i32> {
    if args.is_empty() {
        crate::cx_eprintln!(
            "Usage: {app_name} task add <objective> [--role <role>] [--parent <id>] [--context <ref>] [--mode <sequential|parallel>] [--depends-on <id1,id2>] [--resource <key>] [--resource-keys <k1,k2>] [--max-retries <n>] [--timeout-secs <n>]"
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
        crate::cx_eprintln!("cxrs task add: objective cannot be empty");
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

fn parse_add_flags(
    args: &[String],
    mut i: usize,
) -> Result<
    (
        String,
        Option<String>,
        String,
        String,
        Vec<String>,
        Vec<String>,
        Option<u32>,
        Option<u64>,
    ),
    i32,
> {
    let mut role = "implementer".to_string();
    let mut parent_id: Option<String> = None;
    let mut context_ref = String::new();
    let mut run_mode = "sequential".to_string();
    let mut depends_on: Vec<String> = Vec::new();
    let mut resource_keys: Vec<String> = Vec::new();
    let mut max_retries: Option<u32> = None;
    let mut timeout_secs: Option<u64> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--role" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("cxrs task add: --role requires a value");
                    return Err(2);
                };
                role = v.to_lowercase();
                i += 2;
            }
            "--parent" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("cxrs task add: --parent requires a value");
                    return Err(2);
                };
                parent_id = Some(v.to_string());
                i += 2;
            }
            "--context" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("cxrs task add: --context requires a value");
                    return Err(2);
                };
                context_ref = v.to_string();
                i += 2;
            }
            "--mode" => {
                let Some(v) = args.get(i + 1).map(|s| s.as_str()) else {
                    crate::cx_eprintln!("cxrs task add: --mode requires a value");
                    return Err(2);
                };
                if !matches!(v, "sequential" | "parallel") {
                    crate::cx_eprintln!("cxrs task add: invalid --mode '{v}'");
                    return Err(2);
                }
                run_mode = v.to_string();
                i += 2;
            }
            "--depends-on" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("cxrs task add: --depends-on requires a value");
                    return Err(2);
                };
                depends_on.extend(parse_csv_list(v));
                i += 2;
            }
            "--resource" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("cxrs task add: --resource requires a value");
                    return Err(2);
                };
                if !v.trim().is_empty() {
                    resource_keys.push(v.trim().to_string());
                }
                i += 2;
            }
            "--resource-keys" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("cxrs task add: --resource-keys requires a value");
                    return Err(2);
                };
                resource_keys.extend(parse_csv_list(v));
                i += 2;
            }
            "--max-retries" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("cxrs task add: --max-retries requires a value");
                    return Err(2);
                };
                let Ok(n) = v.parse::<u32>() else {
                    crate::cx_eprintln!("cxrs task add: --max-retries must be an integer");
                    return Err(2);
                };
                max_retries = Some(n);
                i += 2;
            }
            "--timeout-secs" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("cxrs task add: --timeout-secs requires a value");
                    return Err(2);
                };
                let Ok(n) = v.parse::<u64>() else {
                    crate::cx_eprintln!("cxrs task add: --timeout-secs must be an integer");
                    return Err(2);
                };
                timeout_secs = Some(n);
                i += 2;
            }
            other => {
                crate::cx_eprintln!("cxrs task add: unknown flag '{other}'");
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
        run_mode,
        depends_on,
        resource_keys,
        max_retries,
        timeout_secs,
    ) = parse_add_flags(args, i)?;
    if !task_role_valid(&role) {
        crate::cx_eprintln!("cxrs task add: invalid role '{role}'");
        return Err(2);
    }
    Ok(AddArgs {
        objective,
        role,
        parent_id,
        context_ref,
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
        backend: "auto".to_string(),
        model: None,
        profile: "balanced".to_string(),
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
        crate::cx_eprintln!("cxrs task add: {e}");
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
        Some(s) => tasks.into_iter().filter(|t| t.status == s).collect(),
        None => tasks,
    };
    if filtered.is_empty() {
        println!("No tasks.");
        return 0;
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
    let Some(task) = tasks.into_iter().find(|t| t.id == id) else {
        crate::cx_eprintln!("cxrs task show: task not found: {id}");
        return 1;
    };
    match serde_json::to_string_pretty(&task) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!("cxrs task show: render failed: {e}");
            1
        }
    }
}

pub fn set_task_status(id: &str, new_status: &str) -> Result<(), String> {
    let mut tasks = read_tasks()?;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return Err(format!("cxrs task: task not found: {id}"));
    };
    task.status = new_status.to_string();
    task.updated_at = utc_now_iso();
    write_tasks(&tasks)
}

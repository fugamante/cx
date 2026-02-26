use std::fs;
use std::process::Command;

use crate::capture::chunk_text_by_budget;
use crate::config::app_config;
use crate::execmeta::utc_now_iso;
use crate::types::TaskRecord;

use super::{next_task_id, read_tasks, write_tasks};

fn collect_source_text(source: &str) -> Result<String, i32> {
    let out = match source {
        "staged-diff" => Command::new("git")
            .args(["diff", "--staged", "--no-color"])
            .output(),
        "worktree" => Command::new("git").args(["diff", "--no-color"]).output(),
        "log" => Command::new("git")
            .args(["log", "--oneline", "-n", "200"])
            .output(),
        x if x.starts_with("file:") => {
            let p = x.trim_start_matches("file:");
            return Ok(fs::read_to_string(p).unwrap_or_default());
        }
        _ => {
            eprintln!("cxrs task fanout: unsupported --from source '{source}'");
            return Err(2);
        }
    };
    Ok(out
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).to_string())
            } else {
                None
            }
        })
        .unwrap_or_default())
}

fn make_subtask(
    role: &str,
    index: usize,
    total: usize,
    objective: &str,
    parent_id: &str,
    has_chunks: bool,
    tasks: &mut Vec<TaskRecord>,
) -> TaskRecord {
    let id = next_task_id(tasks);
    let context_ref = if has_chunks {
        format!("diff_chunk_{}/{}", index + 1, total)
    } else {
        format!("objective:{objective}")
    };
    let sub_obj = match role {
        "architect" => format!("Define implementation plan for: {objective}"),
        "implementer" => format!("Implement chunk {} for: {objective}", index + 1),
        "reviewer" => format!(
            "Review chunk {} changes for correctness/safety: {objective}",
            index + 1
        ),
        "tester" => format!("Create/execute tests for chunk {}: {objective}", index + 1),
        _ => format!("Document chunk {} outcomes: {objective}", index + 1),
    };
    TaskRecord {
        id,
        parent_id: Some(parent_id.to_string()),
        role: role.to_string(),
        objective: sub_obj,
        context_ref,
        status: "pending".to_string(),
        created_at: utc_now_iso(),
        updated_at: utc_now_iso(),
    }
}

fn ensure_min_created(
    created: &mut Vec<TaskRecord>,
    parent_id: &str,
    objective: &str,
    tasks: &mut Vec<TaskRecord>,
) {
    let roles_cycle = ["architect", "implementer", "reviewer", "tester", "doc"];
    while created.len() < 3 {
        let role = roles_cycle[(created.len() + 1) % roles_cycle.len()].to_string();
        let id = next_task_id(tasks);
        let rec = TaskRecord {
            id,
            parent_id: Some(parent_id.to_string()),
            role: role.clone(),
            objective: format!("{} workstream for: {}", role, objective),
            context_ref: "objective".to_string(),
            status: "pending".to_string(),
            created_at: utc_now_iso(),
            updated_at: utc_now_iso(),
        };
        tasks.push(rec.clone());
        created.push(rec);
    }
}

fn add_fanout_parent(tasks: &mut Vec<TaskRecord>, obj: &str) -> String {
    let parent_id = next_task_id(tasks);
    let now = utc_now_iso();
    tasks.push(TaskRecord {
        id: parent_id.clone(),
        parent_id: None,
        role: "architect".to_string(),
        objective: obj.to_string(),
        context_ref: "fanout_parent".to_string(),
        status: "pending".to_string(),
        created_at: now.clone(),
        updated_at: now,
    });
    parent_id
}

fn create_fanout_children(
    tasks: &mut Vec<TaskRecord>,
    parent_id: &str,
    objective: &str,
    has_chunks: bool,
    chunk_count: usize,
) -> Vec<TaskRecord> {
    let roles_cycle = ["architect", "implementer", "reviewer", "tester", "doc"];
    let mut created: Vec<TaskRecord> = Vec::new();
    for i in 0..chunk_count {
        let role = roles_cycle[(i + 1) % roles_cycle.len()];
        let rec = make_subtask(
            role,
            i,
            chunk_count,
            objective,
            parent_id,
            has_chunks,
            tasks,
        );
        tasks.push(rec.clone());
        created.push(rec);
    }
    ensure_min_created(&mut created, parent_id, objective, tasks);
    if created.len() > 8 {
        created.truncate(8);
    }
    created
}

fn print_fanout_table(parent_id: &str, created: Vec<TaskRecord>) {
    println!("parent: {parent_id}");
    println!("id | role | status | context_ref | objective");
    println!("---|---|---|---|---");
    for t in created {
        println!(
            "{} | {} | {} | {} | {}",
            t.id, t.role, t.status, t.context_ref, t.objective
        );
    }
}

pub fn cmd_task_fanout(app_name: &str, objective: &str, from: Option<&str>) -> i32 {
    let obj = objective.trim();
    if obj.is_empty() {
        eprintln!("Usage: {app_name} task fanout <objective>");
        return 2;
    }
    let mut tasks = match read_tasks() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let parent_id = add_fanout_parent(&mut tasks, obj);
    let source = from.unwrap_or("worktree");
    let diff = match collect_source_text(source) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let chunks = if diff.trim().is_empty() {
        Vec::new()
    } else {
        chunk_text_by_budget(&diff, app_config().budget_chars)
    };

    let created = create_fanout_children(
        &mut tasks,
        &parent_id,
        obj,
        !chunks.is_empty(),
        chunks.len().clamp(1, 6),
    );

    if let Err(e) = write_tasks(&tasks) {
        eprintln!("cxrs task fanout: {e}");
        return 1;
    }
    print_fanout_table(&parent_id, created);
    0
}

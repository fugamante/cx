use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::types::TaskRecord;

#[derive(Debug, Clone, Serialize)]
pub struct PlanWave {
    pub index: usize,
    pub mode: String,
    pub task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockedTask {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskRunPlan {
    pub status_filter: String,
    pub selected: usize,
    pub waves: Vec<PlanWave>,
    pub blocked: Vec<BlockedTask>,
}

fn normalize_run_mode(task: &TaskRecord) -> &str {
    match task.run_mode.as_str() {
        "parallel" => "parallel",
        _ => "sequential",
    }
}

fn effective_dependencies(task: &TaskRecord) -> Vec<String> {
    if !task.depends_on.is_empty() {
        return task.depends_on.clone();
    }
    task.parent_id
        .as_ref()
        .map(|v| vec![v.clone()])
        .unwrap_or_default()
}

fn effective_resource_keys(task: &TaskRecord) -> Vec<String> {
    if !task.resource_keys.is_empty() {
        return task.resource_keys.clone();
    }
    if normalize_run_mode(task) == "parallel" {
        return vec!["repo:write".to_string()];
    }
    Vec::new()
}

fn parse_lock_key(key: &str) -> (&str, &str) {
    match key.rsplit_once(':') {
        Some((domain, mode @ ("read" | "write"))) => (domain, mode),
        _ => (key, "write"),
    }
}

fn lock_conflicts(held: &str, candidate: &str) -> bool {
    let (held_domain, held_mode) = parse_lock_key(held);
    let (cand_domain, cand_mode) = parse_lock_key(candidate);
    if held_domain != cand_domain {
        return false;
    }
    held_mode == "write" || cand_mode == "write"
}

fn unresolved_dependencies(
    task: &TaskRecord,
    done_ids: &HashSet<String>,
    complete_ids: &HashSet<String>,
) -> Vec<String> {
    effective_dependencies(task)
        .into_iter()
        .filter(|d| !done_ids.contains(d) && !complete_ids.contains(d))
        .collect()
}

pub fn build_task_run_plan(tasks: &[TaskRecord], status_filter: &str) -> TaskRunPlan {
    let selected_tasks: Vec<TaskRecord> = tasks
        .iter()
        .filter(|t| t.status == status_filter)
        .cloned()
        .collect();
    let selected = selected_tasks.len();
    let complete_ids: HashSet<String> = tasks
        .iter()
        .filter(|t| t.status == "complete")
        .map(|t| t.id.clone())
        .collect();

    if selected == 0 {
        return TaskRunPlan {
            status_filter: status_filter.to_string(),
            selected,
            waves: Vec::new(),
            blocked: Vec::new(),
        };
    }

    let mut remaining: HashMap<String, TaskRecord> = selected_tasks
        .into_iter()
        .map(|t| (t.id.clone(), t))
        .collect();
    let mut done_ids: HashSet<String> = HashSet::new();
    let mut waves: Vec<PlanWave> = Vec::new();

    loop {
        if remaining.is_empty() {
            break;
        }

        let mut ready_ids: Vec<String> = remaining
            .iter()
            .filter_map(|(id, task)| {
                let unresolved = unresolved_dependencies(task, &done_ids, &complete_ids);
                if unresolved.is_empty() {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        ready_ids.sort();

        if ready_ids.is_empty() {
            let mut blocked: Vec<BlockedTask> = remaining
                .values()
                .map(|task| {
                    let unresolved = unresolved_dependencies(task, &done_ids, &complete_ids);
                    let reason = if unresolved.is_empty() {
                        "unknown scheduler stall".to_string()
                    } else {
                        format!("unresolved dependencies: {}", unresolved.join(", "))
                    };
                    BlockedTask {
                        id: task.id.clone(),
                        reason,
                    }
                })
                .collect();
            blocked.sort_by(|a, b| a.id.cmp(&b.id));
            return TaskRunPlan {
                status_filter: status_filter.to_string(),
                selected,
                waves,
                blocked,
            };
        }

        let mut ready_seq: Vec<String> = ready_ids
            .iter()
            .filter_map(|id| {
                remaining.get(id).and_then(|t| {
                    if normalize_run_mode(t) == "sequential" {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();
        ready_seq.sort();

        for id in ready_seq {
            let wave_index = waves.len() + 1;
            waves.push(PlanWave {
                index: wave_index,
                mode: "sequential".to_string(),
                task_ids: vec![id.clone()],
            });
            done_ids.insert(id.clone());
            remaining.remove(&id);
        }

        let mut ready_parallel: Vec<String> = ready_ids
            .iter()
            .filter_map(|id| {
                remaining.get(id).and_then(|t| {
                    if normalize_run_mode(t) == "parallel" {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();
        ready_parallel.sort();

        if ready_parallel.is_empty() {
            continue;
        }

        let mut selected_parallel: Vec<String> = Vec::new();
        let mut held_keys: Vec<String> = Vec::new();
        for id in &ready_parallel {
            let Some(task) = remaining.get(id) else {
                continue;
            };
            let keys = effective_resource_keys(task);
            let conflicts = keys
                .iter()
                .any(|k| held_keys.iter().any(|h| lock_conflicts(h, k)));
            if conflicts {
                continue;
            }
            for k in keys {
                held_keys.push(k);
            }
            selected_parallel.push(id.clone());
        }

        if selected_parallel.is_empty() {
            selected_parallel.push(ready_parallel[0].clone());
        }

        let wave_index = waves.len() + 1;
        waves.push(PlanWave {
            index: wave_index,
            mode: "parallel".to_string(),
            task_ids: selected_parallel.clone(),
        });
        for id in selected_parallel {
            done_ids.insert(id.clone());
            remaining.remove(&id);
        }
    }

    TaskRunPlan {
        status_filter: status_filter.to_string(),
        selected,
        waves,
        blocked: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(
        id: &str,
        status: &str,
        run_mode: &str,
        depends_on: &[&str],
        resource_keys: &[&str],
    ) -> TaskRecord {
        TaskRecord {
            id: id.to_string(),
            parent_id: None,
            role: "implementer".to_string(),
            objective: format!("obj-{id}"),
            context_ref: String::new(),
            backend: "auto".to_string(),
            model: None,
            profile: "balanced".to_string(),
            run_mode: run_mode.to_string(),
            depends_on: depends_on.iter().map(|v| (*v).to_string()).collect(),
            resource_keys: resource_keys.iter().map(|v| (*v).to_string()).collect(),
            max_retries: None,
            timeout_secs: None,
            status: status.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn run_plan_orders_by_dependencies() {
        let tasks = vec![
            mk("task_001", "pending", "sequential", &[], &["repo:write"]),
            mk(
                "task_002",
                "pending",
                "parallel",
                &["task_001"],
                &["repo:read"],
            ),
            mk(
                "task_003",
                "pending",
                "parallel",
                &["task_001"],
                &["repo:read"],
            ),
        ];
        let plan = build_task_run_plan(&tasks, "pending");
        assert!(plan.blocked.is_empty());
        assert_eq!(plan.waves.len(), 2);
        assert_eq!(plan.waves[0].task_ids, vec!["task_001".to_string()]);
        assert_eq!(
            plan.waves[1].task_ids,
            vec!["task_002".to_string(), "task_003".to_string()]
        );
    }

    #[test]
    fn run_plan_respects_parallel_resource_locks() {
        let tasks = vec![
            mk("task_001", "pending", "parallel", &[], &["repo:write"]),
            mk("task_002", "pending", "parallel", &[], &["repo:write"]),
            mk("task_003", "pending", "parallel", &[], &["repo:read"]),
        ];
        let plan = build_task_run_plan(&tasks, "pending");
        assert!(plan.blocked.is_empty());
        assert_eq!(plan.waves.len(), 3);
        assert_eq!(plan.waves[0].task_ids, vec!["task_001".to_string()]);
        assert_eq!(plan.waves[1].task_ids, vec!["task_002".to_string()]);
        assert_eq!(plan.waves[2].task_ids, vec!["task_003".to_string()]);
    }

    #[test]
    fn run_plan_reports_blocked_cycle() {
        let tasks = vec![
            mk("task_001", "pending", "sequential", &["task_002"], &[]),
            mk("task_002", "pending", "sequential", &["task_001"], &[]),
        ];
        let plan = build_task_run_plan(&tasks, "pending");
        assert!(plan.waves.is_empty());
        assert_eq!(plan.blocked.len(), 2);
    }
}

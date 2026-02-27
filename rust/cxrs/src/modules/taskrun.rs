use serde_json::Value;
use std::env;
use std::fmt;
use std::process::Command;

use crate::runlog::{RunLogInput, log_codex_run};
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

#[derive(Debug, Clone)]
struct ReplicaOutcome {
    index: u32,
    status_code: i32,
    execution_id: Option<String>,
    error: Option<String>,
}

fn parse_words(input: &str) -> Vec<String> {
    match shell_words::split(input) {
        Ok(v) => v,
        Err(_) => input
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
    }
}

fn command_status_or_usage(run: fn(&[String]) -> i32, args: &[String]) -> i32 {
    if args.is_empty() { 2 } else { run(args) }
}

fn task_prompt(task: &TaskRecord) -> String {
    if task.context_ref.trim().is_empty() {
        return format!(
            "Task Objective:\n{}\n\nRespond with concise execution notes and next actions.",
            task.objective
        );
    }
    format!(
        "Task Objective:\n{}\n\nContext Ref:\n{}\n\nRespond with concise execution notes and next actions.",
        task.objective, task.context_ref
    )
}

fn task_backend_override(task: &TaskRecord) -> Option<String> {
    let backend = task.backend.trim().to_lowercase();
    if matches!(backend.as_str(), "codex" | "ollama") {
        Some(backend)
    } else {
        None
    }
}

fn task_mode_override(task: &TaskRecord) -> Option<String> {
    match task.profile.trim().to_lowercase().as_str() {
        "fast" => Some("lean".to_string()),
        "quality" => Some("verbose".to_string()),
        "schema_strict" => Some("deterministic".to_string()),
        _ => None,
    }
}

fn task_model_override(task: &TaskRecord) -> Option<String> {
    task.model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn set_optional_env(name: &str, value: Option<String>) {
    match value {
        Some(v) => unsafe { env::set_var(name, v) },
        None => unsafe { env::remove_var(name) },
    }
}

fn run_task_prompt(
    runner: &TaskRunner,
    task: &TaskRecord,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<(i32, Option<String>), String> {
    let prev_mode = env::var("CX_MODE").ok();
    let prev_backend = env::var("CX_LLM_BACKEND").ok();
    let prev_ollama_model = env::var("CX_OLLAMA_MODEL").ok();
    if let Some(mode) = mode_override {
        // scoped overrides for prompt-based task execution.
        unsafe { env::set_var("CX_MODE", mode) };
    }
    if let Some(backend) = backend_override {
        unsafe { env::set_var("CX_LLM_BACKEND", backend) };
    }
    if let Some(model) = model_override {
        unsafe { env::set_var("CX_OLLAMA_MODEL", model) };
    }
    let exec_result = (runner.execute_task)(TaskSpec {
        command_name: "cxtask_run".to_string(),
        input: TaskInput::Prompt(task_prompt(task)),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    });
    set_optional_env("CX_MODE", prev_mode);
    set_optional_env("CX_LLM_BACKEND", prev_backend);
    set_optional_env("CX_OLLAMA_MODEL", prev_ollama_model);
    let res = exec_result?;
    println!("{}", res.stdout);
    Ok((0, Some(res.execution_id)))
}

fn run_objective_subprocess(
    objective_words: &[String],
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<i32, String> {
    if objective_words.is_empty() {
        return Ok(2);
    }
    let exe = env::current_exe().map_err(|e| format!("cxrs task run: current_exe failed: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.args(objective_words);
    if let Some(mode) = mode_override {
        cmd.env("CX_MODE", mode);
    }
    if let Some(backend) = backend_override {
        cmd.env("CX_LLM_BACKEND", backend);
    }
    if let Some(model) = model_override {
        cmd.env("CX_OLLAMA_MODEL", model);
    }
    let status = crate::process::run_command_status_with_timeout(cmd, "cxtask_run subprocess")?;
    Ok(status.code().unwrap_or(1))
}

fn dispatch_task_command(
    runner: &TaskRunner,
    words: &[String],
    task: &TaskRecord,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
) -> Result<(i32, Option<String>), String> {
    let Some(cmd0) = words.first().map(String::as_str) else {
        return run_task_prompt(runner, task, mode_override, backend_override, None);
    };
    let args: Vec<String> = words.iter().skip(1).cloned().collect();
    let model_override = task_model_override(task);
    if mode_override.is_some() || backend_override.is_some() {
        match cmd0 {
            "cxcommitjson" | "commitjson" | "cxcommitmsg" | "commitmsg" | "cxdiffsum"
            | "diffsum" | "cxdiffsum_staged" | "diffsum-staged" | "cxnext" | "next"
            | "cxfix_run" | "fix-run" | "cxfix" | "fix" | "cx" | "cxj" | "cxo" => {
                let code = run_objective_subprocess(
                    words,
                    mode_override,
                    backend_override,
                    model_override.as_deref(),
                )?;
                return Ok((code, None));
            }
            _ => {}
        }
    }
    let status = match cmd0 {
        "cxcommitjson" | "commitjson" => (runner.cmd_commitjson)(),
        "cxcommitmsg" | "commitmsg" => (runner.cmd_commitmsg)(),
        "cxdiffsum" | "diffsum" => (runner.cmd_diffsum)(false),
        "cxdiffsum_staged" | "diffsum-staged" => (runner.cmd_diffsum)(true),
        "cxnext" | "next" => command_status_or_usage(runner.cmd_next, &args),
        "cxfix_run" | "fix-run" => command_status_or_usage(runner.cmd_fix_run, &args),
        "cxfix" | "fix" => command_status_or_usage(runner.cmd_fix, &args),
        "cx" => command_status_or_usage(runner.cmd_cx, &args),
        "cxj" => command_status_or_usage(runner.cmd_cxj, &args),
        "cxo" => command_status_or_usage(runner.cmd_cxo, &args),
        _ => {
            return run_task_prompt(
                runner,
                task,
                mode_override,
                backend_override,
                model_override.as_deref(),
            );
        }
    };
    Ok((status, None))
}

fn run_task_objective(
    runner: &TaskRunner,
    task: &TaskRecord,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
) -> Result<(i32, Option<String>), String> {
    let words = parse_words(&task.objective);
    dispatch_task_command(runner, &words, task, mode_override, backend_override)
}

fn normalize_converge_mode(raw: &str) -> String {
    let m = raw.trim().to_lowercase();
    if matches!(
        m.as_str(),
        "none" | "first_valid" | "majority" | "judge" | "score"
    ) {
        m
    } else {
        "none".to_string()
    }
}

fn effective_replica_count(task: &TaskRecord, mode: &str) -> u32 {
    let n = task.replicas.max(1);
    if mode == "none" { 1 } else { n }
}

fn select_winner(mode: &str, outcomes: &[ReplicaOutcome]) -> ReplicaOutcome {
    if outcomes.is_empty() {
        return ReplicaOutcome {
            index: 1,
            status_code: 1,
            execution_id: None,
            error: Some("no replica outcomes".to_string()),
        };
    }
    match mode {
        "first_valid" => outcomes
            .iter()
            .find(|o| o.status_code == 0)
            .cloned()
            .unwrap_or_else(|| outcomes[0].clone()),
        "majority" => {
            let ok = outcomes.iter().filter(|o| o.status_code == 0).count();
            let fail = outcomes.len().saturating_sub(ok);
            if ok >= fail {
                outcomes
                    .iter()
                    .find(|o| o.status_code == 0)
                    .cloned()
                    .unwrap_or_else(|| outcomes[0].clone())
            } else {
                outcomes
                    .iter()
                    .find(|o| o.status_code != 0)
                    .cloned()
                    .unwrap_or_else(|| outcomes[0].clone())
            }
        }
        "judge" | "score" => {
            let mut scored: Vec<(i64, u32, ReplicaOutcome)> = outcomes
                .iter()
                .cloned()
                .map(|o| (score_outcome(&o), o.index, o))
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            scored
                .first()
                .map(|(_, _, o)| o.clone())
                .unwrap_or_else(|| outcomes[0].clone())
        }
        _ => outcomes[0].clone(),
    }
}

fn score_outcome(outcome: &ReplicaOutcome) -> i64 {
    let success_score = if outcome.status_code == 0 { 1000 } else { 0 };
    let execution_id_bonus = if outcome.execution_id.is_some() {
        100
    } else {
        0
    };
    let error_penalty = outcome.error.as_ref().map(|e| e.len() as i64).unwrap_or(0);
    success_score + execution_id_bonus - error_penalty.min(200)
}

fn run_replica(
    runner: &TaskRunner,
    task: &TaskRecord,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    replica_index: u32,
    replica_count: u32,
    converge_mode: &str,
) -> ReplicaOutcome {
    set_optional_env("CX_TASK_REPLICA_INDEX", Some(replica_index.to_string()));
    set_optional_env("CX_TASK_REPLICA_COUNT", Some(replica_count.to_string()));
    set_optional_env("CX_TASK_CONVERGE_MODE", Some(converge_mode.to_string()));
    set_optional_env("CX_TASK_CONVERGE_WINNER", None);
    match run_task_objective(runner, task, mode_override, backend_override) {
        Ok((code, execution_id)) => ReplicaOutcome {
            index: replica_index,
            status_code: code,
            execution_id,
            error: None,
        },
        Err(e) => ReplicaOutcome {
            index: replica_index,
            status_code: 1,
            execution_id: None,
            error: Some(e),
        },
    }
}

fn convergence_votes_json(
    converge_mode: &str,
    outcomes: &[ReplicaOutcome],
    winner: &ReplicaOutcome,
) -> String {
    let ok = outcomes.iter().filter(|o| o.status_code == 0).count() as u64;
    let fail = outcomes.len() as u64 - ok;
    let candidates = outcomes
        .iter()
        .map(|o| {
            serde_json::json!({
                "index": o.index,
                "status_code": o.status_code,
                "score": score_outcome(o),
                "execution_id": o.execution_id,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    serde_json::json!({
        "mode": converge_mode,
        "winner": winner.index,
        "ok": ok,
        "fail": fail,
        "replicas_executed": outcomes.len() as u64,
        "replicas_target": outcomes.iter().map(|o| o.index).max().unwrap_or(0) as u64,
        "candidates": candidates,
    })
    .to_string()
}

fn log_convergence_summary(
    task: &TaskRecord,
    converge_mode: &str,
    outcomes: &[ReplicaOutcome],
    winner: &ReplicaOutcome,
) {
    let votes_json = convergence_votes_json(converge_mode, outcomes, winner);
    let prev_votes = env::var("CX_TASK_CONVERGE_VOTES").ok();
    set_optional_env("CX_TASK_CONVERGE_WINNER", Some(winner.index.to_string()));
    set_optional_env("CX_TASK_CONVERGE_VOTES", Some(votes_json));
    let usage = crate::types::UsageStats::default();
    let capture = crate::types::CaptureStats::default();
    let _ = log_codex_run(RunLogInput {
        tool: "cxtask_converge",
        prompt: &task.objective,
        schema_prompt: None,
        schema_raw: None,
        schema_attempt: None,
        timed_out: None,
        timeout_secs: None,
        command_label: Some("task_converge"),
        duration_ms: 0,
        usage: Some(&usage),
        capture: Some(&capture),
        schema_ok: true,
        schema_reason: None,
        schema_name: None,
        quarantine_id: None,
        policy_blocked: None,
        policy_reason: None,
    });
    set_optional_env("CX_TASK_CONVERGE_VOTES", prev_votes);
}

fn set_runtime_task_state(runner: &TaskRunner, id: &str, parent_id: Option<&String>) {
    let _ = (runner.set_state_path)("runtime.current_task_id", Value::String(id.to_string()));
    let _ = (runner.set_state_path)(
        "runtime.current_task_parent_id",
        match parent_id {
            Some(v) => Value::String(v.clone()),
            None => Value::Null,
        },
    );
}

fn restore_runtime_task_state(
    runner: &TaskRunner,
    prev_task_id: Option<String>,
    prev_parent_id: Option<String>,
) {
    let _ = (runner.set_state_path)(
        "runtime.current_task_id",
        prev_task_id.map_or(Value::Null, Value::String),
    );
    let _ = (runner.set_state_path)(
        "runtime.current_task_parent_id",
        prev_parent_id.map_or(Value::Null, Value::String),
    );
}

fn finalize_task_status(
    runner: &TaskRunner,
    id: &str,
    status_code: i32,
) -> Result<(), TaskRunError> {
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
    Ok(())
}

pub fn run_task_by_id(
    runner: &TaskRunner,
    id: &str,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    managed_by_parent: bool,
) -> Result<(i32, Option<String>), TaskRunError> {
    let mut tasks = (runner.read_tasks)().map_err(TaskRunError::Critical)?;
    let idx = tasks
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| TaskRunError::Critical(format!("cxrs task run: task not found: {id}")))?;
    if matches!(tasks[idx].status.as_str(), "complete" | "failed") {
        return Ok((0, None));
    }
    if !managed_by_parent {
        tasks[idx].status = "in_progress".to_string();
        tasks[idx].updated_at = (runner.utc_now_iso)();
        (runner.write_tasks)(&tasks).map_err(TaskRunError::Critical)?;
    }
    let prev_task_id = if managed_by_parent {
        None
    } else {
        (runner.current_task_id)()
    };
    let prev_parent_id = if managed_by_parent {
        None
    } else {
        (runner.current_task_parent_id)()
    };
    let prev_replica_index = env::var("CX_TASK_REPLICA_INDEX").ok();
    let prev_replica_count = env::var("CX_TASK_REPLICA_COUNT").ok();
    let prev_converge_mode = env::var("CX_TASK_CONVERGE_MODE").ok();
    let prev_converge_winner = env::var("CX_TASK_CONVERGE_WINNER").ok();
    if !managed_by_parent {
        set_runtime_task_state(runner, id, tasks[idx].parent_id.as_ref());
    }

    let effective_mode = mode_override
        .map(ToOwned::to_owned)
        .or_else(|| task_mode_override(&tasks[idx]));
    let effective_backend = backend_override
        .map(ToOwned::to_owned)
        .or_else(|| task_backend_override(&tasks[idx]));
    let converge_mode = normalize_converge_mode(&tasks[idx].converge);
    let replica_count = effective_replica_count(&tasks[idx], &converge_mode);
    if tasks[idx].converge == "none" && tasks[idx].replicas > 1 {
        crate::cx_eprintln!(
            "cxrs task run: task {} replicas={} ignored because converge=none",
            id,
            tasks[idx].replicas
        );
    }
    let mut outcomes: Vec<ReplicaOutcome> = Vec::new();
    for replica_index in 1..=replica_count {
        let outcome = run_replica(
            runner,
            &tasks[idx],
            effective_mode.as_deref(),
            effective_backend.as_deref(),
            replica_index,
            replica_count,
            &converge_mode,
        );
        let should_stop = converge_mode == "first_valid" && outcome.status_code == 0;
        outcomes.push(outcome);
        if should_stop {
            break;
        }
    }
    let winner = select_winner(&converge_mode, &outcomes);
    set_optional_env("CX_TASK_CONVERGE_WINNER", Some(winner.index.to_string()));
    if replica_count > 1 || converge_mode != "none" {
        log_convergence_summary(&tasks[idx], &converge_mode, &outcomes, &winner);
    }
    if !managed_by_parent {
        restore_runtime_task_state(runner, prev_task_id, prev_parent_id);
    }
    set_optional_env("CX_TASK_REPLICA_INDEX", prev_replica_index);
    set_optional_env("CX_TASK_REPLICA_COUNT", prev_replica_count);
    set_optional_env("CX_TASK_CONVERGE_MODE", prev_converge_mode);
    set_optional_env("CX_TASK_CONVERGE_WINNER", prev_converge_winner);

    let status_code = winner.status_code;
    let execution_id = winner.execution_id.clone();
    let objective_err = winner.error.clone();

    if !managed_by_parent {
        finalize_task_status(runner, id, status_code)?;
    }
    if let Some(e) = objective_err {
        crate::cx_eprintln!("cxrs task run: objective failed for {id}: {e}");
    }
    Ok((status_code, execution_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(index: u32, status_code: i32) -> ReplicaOutcome {
        ReplicaOutcome {
            index,
            status_code,
            execution_id: None,
            error: None,
        }
    }

    #[test]
    fn winner_first_valid_picks_first_success() {
        let winner = select_winner("first_valid", &[out(1, 1), out(2, 0), out(3, 0)]);
        assert_eq!(winner.index, 2);
    }

    #[test]
    fn winner_majority_prefers_success_when_tied_or_better() {
        let winner = select_winner("majority", &[out(1, 1), out(2, 0)]);
        assert_eq!(winner.status_code, 0);
        assert_eq!(winner.index, 2);
    }

    #[test]
    fn winner_score_prefers_success() {
        let winner = select_winner("score", &[out(1, 1), out(2, 0)]);
        assert_eq!(winner.index, 2);
    }

    #[test]
    fn winner_judge_breaks_tie_by_lowest_index() {
        let winner = select_winner("judge", &[out(2, 1), out(1, 1)]);
        assert_eq!(winner.index, 1);
    }
}

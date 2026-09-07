mod common;

use common::*;
use serde_json::Value;
use std::fs;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[test]
fn run_all_enforces_backend_cap_records_queue() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
sleep 1
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    for i in 1..=3 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo cap-test-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
            "--mode",
            "parallel",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let started = Instant::now();
    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "mixed",
        "--backend-pool",
        "primary",
        "--backend-cap",
        "primary=1",
        "--max-workers",
        "3",
    ]);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let progress = stderr_str(&out);
    assert!(progress.contains("launch [1/3]"), "{progress}");
    assert!(progress.contains("done [3/3]"), "{progress}");
    assert!(!stdout_str(&out).contains("launch ["));
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        elapsed_ms >= 2800,
        "backend cap likely not enforced; elapsed_ms={elapsed_ms}"
    );

    let runs = common::parse_jsonl(&repo.runs_log());
    let task_rows: Vec<&Value> = runs
        .iter()
        .filter(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .collect();
    assert!(
        task_rows.len() >= 3,
        "expected at least 3 cxo rows in runs log, got {}",
        task_rows.len()
    );
    for row in task_rows {
        assert!(row.get("worker_id").is_some(), "missing worker_id: {row}");
        assert!(row.get("queue_ms").is_some(), "missing queue_ms: {row}");
        assert!(
            row.get("queue_started_at")
                .and_then(Value::as_str)
                .is_some(),
            "missing queue_started_at: {row}"
        );
        assert!(
            row.get("task_started_at").and_then(Value::as_str).is_some(),
            "missing task_started_at: {row}"
        );
        assert!(
            row.get("task_finished_at")
                .and_then(Value::as_str)
                .is_some(),
            "missing task_finished_at: {row}"
        );
    }
}

#[test]
fn parallel_lane_runs() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
sleep 2
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo parallel-lane-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
            "--mode",
            "parallel",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let started = Instant::now();
    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "parallel",
        "--backend-pool",
        "primary",
        "--max-workers",
        "2",
        "--json",
    ]);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("run-all json");
    assert_eq!(
        payload.get("mode").and_then(Value::as_str),
        Some("parallel")
    );
    assert_eq!(payload.get("complete").and_then(Value::as_u64), Some(2));
    let runs = common::parse_jsonl(&repo.runs_log());
    let task_rows: Vec<&Value> = runs
        .iter()
        .filter(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .collect();
    assert_eq!(
        task_rows.len(),
        2,
        "expected exactly 2 cxo task rows for parallel run: {runs:#?}"
    );
    let workers: std::collections::BTreeSet<String> = task_rows
        .iter()
        .filter_map(|row| {
            row.get("worker_id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect();
    assert!(
        workers.len() >= 2,
        "parallel lane did not use multiple workers; workers={workers:?}"
    );
    assert!(
        elapsed_ms < 9000,
        "parallel lane appears stalled; elapsed_ms={elapsed_ms}"
    );
}

#[test]
fn strict_plan_blocks() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    let root = repo.run(&[
        "task",
        "add",
        "cxo echo strict-root",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "parallel",
    ]);
    assert!(root.status.success(), "stderr={}", stderr_str(&root));
    let root_id = stdout_str(&root).trim().to_string();

    let child = repo.run(&[
        "task",
        "add",
        "cxo echo strict-child",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "parallel",
        "--depends-on",
        &root_id,
    ]);
    assert!(child.status.success(), "stderr={}", stderr_str(&child));

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "parallel",
        "--strict-plan",
        "--backend-pool",
        "primary",
        "--max-workers",
        "2",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("strict-plan failed"),
        "stderr={}",
        stderr_str(&out)
    );
}

#[test]
fn strict_plan_allows() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo strict-ok-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
            "--mode",
            "parallel",
            "--resource-keys",
            "repo:read",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "parallel",
        "--strict-plan",
        "--backend-pool",
        "primary",
        "--max-workers",
        "2",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("run-all json");
    assert_eq!(
        payload.get("strict_plan").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(payload.get("complete").and_then(Value::as_u64), Some(2));
}

#[test]
fn run_all_summary_includes_failure_taxonomy_fields() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
prompt="$(cat)"
if printf '%s' "$prompt" | grep -q "fail-case"; then
  exit 1
fi
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );
    for objective in ["cxo echo ok-case", "cxo echo fail-case"] {
        let add = repo.run(&[
            "task",
            "add",
            objective,
            "--role",
            "implementer",
            "--backend",
            "primary",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run(&["task", "run-all", "--status", "pending"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected one task failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let stdout = stdout_str(&out);
    assert!(stdout.contains("run-all summary:"), "{stdout}");
    assert!(stdout.contains("blocked="), "{stdout}");
    assert!(stdout.contains("retryable_failures="), "{stdout}");
    assert!(stdout.contains("non_retryable_failures="), "{stdout}");
    assert!(stdout.contains("critical_errors="), "{stdout}");
}

#[cfg(unix)]
#[test]
fn run_all_halt_on_critical_first_failure() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
sleep 2
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );
    for objective in ["cxo echo halt-critical-a", "cxo echo halt-critical-b"] {
        let add = repo.run(&[
            "task",
            "add",
            objective,
            "--role",
            "implementer",
            "--backend",
            "primary",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let tasks_file = repo.tasks_file();
    let tasks_file_for_breaker = tasks_file.clone();
    let breaker = std::thread::spawn(move || {
        sleep(Duration::from_millis(400));
        let _ = fs::remove_file(&tasks_file_for_breaker);
        let _ = fs::create_dir_all(&tasks_file_for_breaker);
    });
    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--halt-on-critical",
    ]);
    breaker.join().expect("join breaker thread");
    if tasks_file.is_dir() {
        let _ = fs::remove_dir_all(&tasks_file);
    }

    assert_eq!(
        out.status.code(),
        Some(1),
        "expected non-zero on critical halt; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let stderr = stderr_str(&out);
    let critical_count = stderr.matches("critical error for task_").count();
    assert_eq!(
        critical_count, 1,
        "expected one critical error before halt; stderr={stderr}"
    );
    let stdout = stdout_str(&out);
    assert!(
        stdout.contains("run-all halted_on_critical: true"),
        "expected halt summary line; stdout={stdout}"
    );
    assert!(
        stdout.contains("run-all halted_remaining: 1"),
        "expected halted remaining count; stdout={stdout}"
    );
}

#[cfg(unix)]
#[test]
fn run_all_continue_on_critical_remaining_tasks() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
sleep 2
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );
    for objective in [
        "cxo echo continue-critical-a",
        "cxo echo continue-critical-b",
    ] {
        let add = repo.run(&[
            "task",
            "add",
            objective,
            "--role",
            "implementer",
            "--backend",
            "primary",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let tasks_file = repo.tasks_file();
    let tasks_file_for_breaker = tasks_file.clone();
    let breaker = std::thread::spawn(move || {
        sleep(Duration::from_millis(400));
        let _ = fs::remove_file(&tasks_file_for_breaker);
        let _ = fs::create_dir_all(&tasks_file_for_breaker);
    });
    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--continue-on-critical",
    ]);
    breaker.join().expect("join breaker thread");
    if tasks_file.is_dir() {
        let _ = fs::remove_dir_all(&tasks_file);
    }

    assert_eq!(
        out.status.code(),
        Some(1),
        "expected non-zero with critical failures; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let stderr = stderr_str(&out);
    let critical_count = stderr.matches("critical error for task_").count();
    assert_eq!(
        critical_count, 2,
        "expected two critical errors in continue mode; stderr={stderr}"
    );
    let stdout = stdout_str(&out);
    assert!(
        stdout.contains("critical_errors=2"),
        "expected summary to include critical_errors=2; stdout={stdout}"
    );
}

#[test]
fn run_all_respects_dependency_waves_concurrency() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
sleep 1
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    let t1 = repo.run(&[
        "task",
        "add",
        "cxo echo dep-root",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "sequential",
    ]);
    assert!(t1.status.success(), "stderr={}", stderr_str(&t1));
    let id1 = stdout_str(&t1).trim().to_string();

    for label in ["dep-child-a", "dep-child-b"] {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo {label}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
            "--mode",
            "parallel",
            "--depends-on",
            &id1,
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let started = Instant::now();
    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "mixed",
        "--backend-pool",
        "primary",
        "--backend-cap",
        "primary=2",
        "--max-workers",
        "2",
    ]);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        (1800..=7000).contains(&elapsed_ms),
        "expected two-wave runtime envelope, got elapsed_ms={elapsed_ms}"
    );

    let tasks = read_json(&repo.tasks_file());
    let statuses: Vec<String> = tasks
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|t| {
            t.get("status")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .collect();
    assert!(
        statuses.iter().all(|s| s == "complete"),
        "not all tasks completed: {statuses:?}"
    );

    let runs = common::parse_jsonl(&repo.runs_log());
    let cxo_rows: Vec<&Value> = runs
        .iter()
        .filter(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .collect();
    let wave_modes: std::collections::BTreeSet<String> = cxo_rows
        .iter()
        .filter_map(|v| {
            v.get("wave_mode")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect();
    let wave_indexes: Vec<u64> = cxo_rows
        .iter()
        .filter_map(|v| v.get("wave_index").and_then(Value::as_u64))
        .collect();
    let wave_sizes: Vec<u64> = cxo_rows
        .iter()
        .filter_map(|v| v.get("wave_size").and_then(Value::as_u64))
        .collect();
    assert!(
        wave_indexes.len() >= 3,
        "expected wave_index on all task rows; got {cxo_rows:?}"
    );
    assert!(
        wave_sizes.len() >= 3,
        "expected wave_size on all task rows; got {cxo_rows:?}"
    );
    assert!(
        wave_modes.contains("parallel"),
        "expected parallel wave mode in mixed run; got {wave_modes:?}"
    );
    assert!(
        wave_modes.contains("sequential"),
        "expected sequential wave mode in mixed run; got {wave_modes:?}"
    );
    let mut queue_ms_values: Vec<u64> = cxo_rows
        .iter()
        .filter_map(|v| v.get("queue_ms").and_then(Value::as_u64))
        .collect();
    queue_ms_values.sort();
    assert!(
        queue_ms_values.len() >= 3,
        "expected queue_ms on all task rows; got {queue_ms_values:?}"
    );
    assert!(
        queue_ms_values.last().copied().unwrap_or(0) >= 900,
        "expected deferred wave queue_ms >= 900ms, got {queue_ms_values:?}"
    );
}

#[test]
fn run_all_queue_increases_for_later_tasks() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
sleep 1
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    for i in 1..=4 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo queue-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
            "--mode",
            "parallel",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "mixed",
        "--backend-pool",
        "primary",
        "--backend-cap",
        "primary=1",
        "--max-workers",
        "4",
    ]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let runs = common::parse_jsonl(&repo.runs_log());
    let mut queue_values: Vec<u64> = runs
        .iter()
        .filter(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .filter_map(|v| v.get("queue_ms").and_then(Value::as_u64))
        .collect();
    queue_values.sort();
    assert_eq!(
        queue_values.len(),
        4,
        "expected queue_ms for each cxo run, got {queue_values:?}"
    );
    assert!(
        queue_values.first().copied().unwrap_or(0) < 300,
        "first task should have near-zero queue, got {queue_values:?}"
    );
    assert!(
        queue_values.last().copied().unwrap_or(0) >= 2500,
        "last task should have significant queue delay, got {queue_values:?}"
    );
}

#[test]
fn run_all_json() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "ollama",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    for i in 1..=2 {
        let backend = if i == 1 { "primary" } else { "ollama" };
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo json-{i}"),
            "--role",
            "implementer",
            "--backend",
            backend,
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let mock_path = format!("{}:/usr/bin:/bin", repo.mock_bin.to_string_lossy());
    let out = repo.run_with_env(
        &[
            "task",
            "run-all",
            "--status",
            "pending",
            "--backend-pool",
            "primary,ollama",
            "--json",
        ],
        &[("PATH", mock_path.as_str())],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected non-zero because mocked ollama execution is not guaranteed clean; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("valid json");
    assert_eq!(
        v.get("contract_version").and_then(Value::as_str),
        Some("task-run-all.v1")
    );
    assert_eq!(v.get("scheduled").and_then(Value::as_u64), Some(2));
    assert_eq!(v.get("halted_remaining").and_then(Value::as_u64), Some(0));
    assert_eq!(
        v.get("task_readiness")
            .and_then(|t| t.get("can_run_mixed"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        v.get("backend_fallbacks")
            .and_then(|v| v.get("primary->ollama"))
            .and_then(Value::as_u64),
        Some(1)
    );
    assert!(
        v.get("preflight")
            .and_then(|v| v.get("recommendations"))
            .and_then(Value::as_array)
            .is_some(),
        "{v}"
    );
    let tasks = v
        .get("tasks")
        .and_then(Value::as_array)
        .expect("tasks array");
    assert_eq!(tasks.len(), 2, "{v}");
    assert!(
        tasks.iter().any(|t| {
            t.get("used_backend_fallback").and_then(Value::as_bool) == Some(true)
                && t.get("requested_backend").and_then(Value::as_str) == Some("primary")
                && t.get("backend").and_then(Value::as_str) == Some("ollama")
        }),
        "{v}"
    );
}

#[test]
fn run_all_dry() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo dry-run-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--dry-run",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("run-all json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("task-run-all.v1")
    );
    assert_eq!(payload.get("scheduled").and_then(Value::as_u64), Some(2));
    assert_eq!(payload.get("complete").and_then(Value::as_u64), Some(0));
    assert_eq!(payload.get("failed").and_then(Value::as_u64), Some(0));
    assert_eq!(
        payload
            .get("task_readiness")
            .and_then(|v| v.get("recommended_mode"))
            .and_then(Value::as_str),
        Some("sequential")
    );
    assert_eq!(
        payload
            .get("preflight")
            .and_then(|v| v.get("advice"))
            .and_then(Value::as_str),
        Some("preflight is operationally clean")
    );
    assert_eq!(
        payload
            .get("preflight")
            .and_then(|v| v.get("reasoning_gate"))
            .and_then(|v| v.get("mode"))
            .and_then(Value::as_str),
        Some("no_reasoning_needed")
    );
    assert_eq!(
        payload
            .get("preflight")
            .and_then(|v| v.get("recent_context"))
            .and_then(|v| v.get("resume_reuses_prior_action"))
            .and_then(Value::as_bool),
        Some(false)
    );
    let tasks = payload
        .get("tasks")
        .and_then(Value::as_array)
        .expect("tasks array");
    assert_eq!(tasks.len(), 2, "{payload}");
    assert!(
        tasks
            .iter()
            .all(|t| t.get("status").and_then(Value::as_str) == Some("dry_run")),
        "{payload}"
    );

    let task_rows = read_json(&repo.tasks_file())
        .as_array()
        .expect("tasks array")
        .to_vec();
    assert!(
        task_rows
            .iter()
            .all(|t| t.get("status").and_then(Value::as_str) == Some("pending")),
        "dry run should not mutate task status"
    );
    assert!(
        !repo.runs_log().exists(),
        "dry run should not execute tasks"
    );
}

#[test]
fn run_strict_dry() {
    let repo = TempRepo::new("cxrs-it");
    let root = repo.run(&[
        "task",
        "add",
        "cxo echo dry-strict-root",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "parallel",
    ]);
    assert!(root.status.success(), "stderr={}", stderr_str(&root));
    let root_id = stdout_str(&root).trim().to_string();

    let child = repo.run(&[
        "task",
        "add",
        "cxo echo dry-strict-child",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "parallel",
        "--depends-on",
        &root_id,
    ]);
    assert!(child.status.success(), "stderr={}", stderr_str(&child));

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "parallel",
        "--strict-plan",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("run-all json");
    assert_eq!(payload.get("scheduled").and_then(Value::as_u64), Some(2));
    assert_eq!(payload.get("blocked").and_then(Value::as_u64), Some(0));
    assert_eq!(
        payload
            .get("task_readiness")
            .and_then(|v| v.get("recommended_mode"))
            .and_then(Value::as_str),
        Some("mixed")
    );
    assert_eq!(
        payload
            .get("task_readiness")
            .and_then(|v| v.get("can_run_parallel"))
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(
        payload
            .get("preflight")
            .and_then(|v| v.get("advice"))
            .and_then(Value::as_str)
            .is_some_and(|v| v.contains("parallel strict-plan is not executable")),
        "{payload}"
    );
    let tasks = payload
        .get("tasks")
        .and_then(Value::as_array)
        .expect("tasks array");
    assert!(
        tasks
            .iter()
            .all(|t| t.get("status").and_then(Value::as_str) == Some("dry_run")),
        "{payload}"
    );
    assert!(
        !repo.runs_log().exists(),
        "dry run should not execute tasks"
    );
}

#[test]
fn run_all_contract() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo run-all-contract-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run(&["task", "run-all", "--status", "pending", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("run-all json");
    let fixture = load_fixture_json("run_all_contract.json");
    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "task_run_all.top");
    let readiness_keys = fixture_keys(&fixture, "task_readiness_keys");
    assert_has_keys(
        payload.get("task_readiness").expect("task_readiness"),
        &readiness_keys,
        "task_run_all.task_readiness",
    );
    let preflight_keys = fixture_keys(&fixture, "preflight_keys");
    assert_has_keys(
        payload.get("preflight").expect("preflight"),
        &preflight_keys,
        "task_run_all.preflight",
    );
    let preflight_gate_keys = fixture_keys(&fixture, "preflight_reasoning_gate_keys");
    assert_has_keys(
        payload
            .get("preflight")
            .and_then(|v| v.get("reasoning_gate"))
            .expect("preflight.reasoning_gate"),
        &preflight_gate_keys,
        "task_run_all.preflight.reasoning_gate",
    );
    let preflight_context_keys = fixture_keys(&fixture, "preflight_recent_context_keys");
    assert_has_keys(
        payload
            .get("preflight")
            .and_then(|v| v.get("recent_context"))
            .expect("preflight.recent_context"),
        &preflight_context_keys,
        "task_run_all.preflight.recent_context",
    );
    let concurrency_keys = fixture_keys(&fixture, "concurrency_summary_keys");
    assert_has_keys(
        payload
            .get("concurrency_summary")
            .expect("concurrency_summary"),
        &concurrency_keys,
        "task_run_all.concurrency_summary",
    );
    let invariant_keys = fixture_keys(&fixture, "invariants_keys");
    assert_has_keys(
        payload.get("invariants").expect("invariants"),
        &invariant_keys,
        "task_run_all.invariants",
    );
    assert_eq!(
        payload
            .get("invariants")
            .and_then(|v| v.get("status"))
            .and_then(Value::as_str),
        Some("clean")
    );

    let task_keys = fixture_keys(&fixture, "task_keys");
    for task in payload
        .get("tasks")
        .and_then(Value::as_array)
        .expect("tasks array")
    {
        assert_has_keys(task, &task_keys, "task_run_all.tasks.item");
    }

    let runs = common::parse_jsonl(&repo.runs_log());
    let summary_row = runs
        .iter()
        .rev()
        .find(|v| v.get("tool").and_then(Value::as_str) == Some("cxtask_runall"))
        .expect("cxtask_runall summary row");
    assert!(
        summary_row
            .get("run_all_invocation_command")
            .and_then(Value::as_str)
            .is_some(),
        "{summary_row}"
    );
    assert!(
        summary_row
            .get("run_all_failure_pattern")
            .and_then(Value::as_str)
            .is_some(),
        "{summary_row}"
    );
    assert!(
        summary_row
            .get("run_all_recommended_resume_point")
            .and_then(Value::as_str)
            .is_some(),
        "{summary_row}"
    );
    assert!(
        summary_row
            .get("run_all_worker_count")
            .and_then(Value::as_u64)
            .is_some(),
        "{summary_row}"
    );
    assert!(
        summary_row
            .get("run_all_max_retry_attempt")
            .and_then(Value::as_u64)
            .is_some(),
        "{summary_row}"
    );
}

#[test]
fn run_events_contract() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        concat!("co", "dex"),
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo run-all-events-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--events-jsonl",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("run-all json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("task-run-all.v1")
    );

    let events: Vec<Value> = stderr_str(&out)
        .lines()
        .filter(|line| line.trim_start().starts_with('{'))
        .map(|line| serde_json::from_str::<Value>(line).expect("valid event jsonl"))
        .collect();
    assert!(
        events.len() >= 5,
        "expected queued/started/result/summary events, got {events:?}"
    );
    assert!(
        events.iter().all(
            |event| event.get("contract_version").and_then(Value::as_str) == Some("task-events.v1")
        ),
        "{events:?}"
    );
    let event_names: Vec<&str> = events
        .iter()
        .filter_map(|event| event.get("event").and_then(Value::as_str))
        .collect();
    assert!(event_names.contains(&"queued"), "{event_names:?}");
    assert!(event_names.contains(&"started"), "{event_names:?}");
    assert!(event_names.contains(&"completed"), "{event_names:?}");
    assert_eq!(event_names.last().copied(), Some("summary"));
    let summary = events.last().expect("summary event");
    assert_eq!(summary.get("scheduled").and_then(Value::as_u64), Some(2));
    assert_eq!(summary.get("complete").and_then(Value::as_u64), Some(2));
    assert_eq!(summary.get("failed").and_then(Value::as_u64), Some(0));

    let persisted = common::parse_jsonl(&repo.task_events_log());
    assert_eq!(persisted.len(), events.len());
    assert_eq!(
        persisted
            .last()
            .and_then(|event| event.get("event"))
            .and_then(Value::as_str),
        Some("summary")
    );

    let events_out = repo.run(&["task", "events", "--limit", "3", "--json"]);
    assert!(
        events_out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&events_out),
        stderr_str(&events_out)
    );
    let recent: Value = serde_json::from_str(&stdout_str(&events_out)).expect("events json");
    let recent_rows = recent.as_array().expect("events array");
    assert_eq!(recent_rows.len(), 3, "{recent}");
    assert_eq!(
        recent_rows
            .last()
            .and_then(|event| event.get("event"))
            .and_then(Value::as_str),
        Some("summary")
    );
}

#[test]
fn run_wave_preflight() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"pw1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":12,"schema_enforced":false,"schema_valid":true,
            "task_id":"t1","wave_index":1,"wave_mode":"parallel","wave_size":2,"queue_ms":250
        }),
        serde_json::json!({
            "execution_id":"pw2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":14,"schema_enforced":false,"schema_valid":true,
            "task_id":"t2","wave_index":3,"wave_mode":"mixed","wave_size":1,"queue_ms":2400
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo wave-preflight-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
            "--mode",
            "parallel",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--dry-run",
        "--mode",
        "mixed",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("run-all json");
    let preflight = payload.get("preflight").expect("preflight");
    assert!(
        preflight
            .get("advice")
            .and_then(Value::as_str)
            .is_some_and(|v| v.contains("queue pressure in later waves")),
        "{payload}"
    );
    assert_eq!(
        preflight.get("latest_wave_index").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        preflight
            .get("max_queue_wave_index")
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        preflight.get("max_queue_wave_ms").and_then(Value::as_u64),
        Some(2400)
    );
    assert_eq!(
        preflight
            .get("reasoning_gate")
            .and_then(|v| v.get("mode"))
            .and_then(Value::as_str),
        Some("cheap_structured_action")
    );
    assert_eq!(
        preflight
            .get("recent_context")
            .and_then(|v| v.get("resume_reuses_prior_action"))
            .and_then(Value::as_bool),
        Some(false)
    );
    let recs = preflight
        .get("recommendations")
        .and_then(Value::as_array)
        .expect("recommendations");
    assert!(
        recs.iter()
            .any(|v| v.as_str() == Some("xshelf task run-all --status pending --mode sequential")),
        "{payload}"
    );
}

#[test]
fn plan_json_dry() {
    let repo = TempRepo::new("cxrs-it");
    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo plan-dry-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
            "--mode",
            "parallel",
            "--resource-keys",
            "repo:read",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "parallel",
        "--plan-json",
    ]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("plan json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("task-run-plan.v1")
    );
    assert_eq!(
        payload.get("requested_mode").and_then(Value::as_str),
        Some("parallel")
    );
    assert_eq!(
        payload.get("can_execute").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(payload.get("wave_count").and_then(Value::as_u64), Some(1));
    assert_eq!(
        payload.get("parallel_task_count").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        payload.get("sequential_task_count").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        payload.get("blocked_count").and_then(Value::as_u64),
        Some(0)
    );

    let tasks = read_json(&repo.tasks_file());
    let arr = tasks.as_array().expect("tasks array");
    assert!(
        arr.iter()
            .all(|t| t.get("status").and_then(Value::as_str) == Some("pending")),
        "dry run should not mutate task status: {tasks}"
    );
    assert!(
        !repo.runs_log().exists(),
        "dry run should not execute tasks"
    );
}

#[test]
fn plan_json_strict() {
    let repo = TempRepo::new("cxrs-it");
    let root = repo.run(&[
        "task",
        "add",
        "cxo echo plan-root",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "parallel",
    ]);
    assert!(root.status.success(), "stderr={}", stderr_str(&root));
    let root_id = stdout_str(&root).trim().to_string();

    let child = repo.run(&[
        "task",
        "add",
        "cxo echo plan-child",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "parallel",
        "--depends-on",
        &root_id,
    ]);
    assert!(child.status.success(), "stderr={}", stderr_str(&child));

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "parallel",
        "--strict-plan",
        "--plan-json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("plan json");
    assert_eq!(
        payload.get("strict_plan").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload.get("strict_plan_ok").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload.get("can_execute").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload.get("strict_plan_reason").and_then(Value::as_str),
        Some("parallel mode would serialize across waves")
    );
    assert_eq!(payload.get("wave_count").and_then(Value::as_u64), Some(2));
    assert_eq!(
        payload.get("blocked_count").and_then(Value::as_u64),
        Some(0)
    );
}

#[test]
fn plan_json_contract() {
    let repo = TempRepo::new("cxrs-it");
    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo plan-contract-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
            "--mode",
            "parallel",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        "parallel",
        "--plan-json",
    ]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("plan json");
    let fixture = load_fixture_json("plan_json_contract.json");
    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "task_run_plan.top");

    let wave_keys = fixture_keys(&fixture, "wave_keys");
    for wave in payload
        .get("waves")
        .and_then(Value::as_array)
        .expect("waves array")
    {
        assert_has_keys(wave, &wave_keys, "task_run_plan.waves.item");
    }

    let blocked_keys = fixture_keys(&fixture, "blocked_keys");
    for blocked in payload
        .get("blocked")
        .and_then(Value::as_array)
        .expect("blocked array")
    {
        assert_has_keys(blocked, &blocked_keys, "task_run_plan.blocked.item");
    }
}

#[test]
fn task_check_json() {
    let repo = TempRepo::new("cxrs-it");
    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo check-json-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
            "--mode",
            "parallel",
            "--resource-keys",
            "repo:read",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }
    let out = repo.run(&["task", "check", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("check json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("task-check.v1")
    );
    assert_eq!(payload.get("selected").and_then(Value::as_u64), Some(2));
    assert_eq!(
        payload.get("recommended_mode").and_then(Value::as_str),
        Some("parallel")
    );
    assert_eq!(
        payload.get("can_run_parallel").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload.get("parallel_waves").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        payload.get("largest_parallel_wave").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        payload.get("strict_plan_ok").and_then(Value::as_bool),
        Some(true)
    );
    assert!(
        payload
            .get("strict_plan_reason")
            .is_some_and(Value::is_null),
        "{payload}"
    );
}

#[test]
fn task_check_strict() {
    let repo = TempRepo::new("cxrs-it");
    let root = repo.run(&[
        "task",
        "add",
        "cxo echo check-root",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "parallel",
    ]);
    assert!(root.status.success(), "stderr={}", stderr_str(&root));
    let root_id = stdout_str(&root).trim().to_string();

    let child = repo.run(&[
        "task",
        "add",
        "cxo echo check-child",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "parallel",
        "--depends-on",
        &root_id,
    ]);
    assert!(child.status.success(), "stderr={}", stderr_str(&child));

    let out = repo.run(&["task", "check", "--strict-plan", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("check json");
    assert_eq!(
        payload.get("strict_plan_ok").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload.get("strict_plan_reason").and_then(Value::as_str),
        Some("parallel mode would serialize across waves")
    );
    assert_eq!(
        payload.get("recommended_mode").and_then(Value::as_str),
        Some("mixed")
    );
    assert_eq!(
        payload.get("can_run_mixed").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload.get("can_run_parallel").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload.get("sequential_waves").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        payload.get("parallel_waves").and_then(Value::as_u64),
        Some(2)
    );
}

#[test]
fn task_check_contract() {
    let repo = TempRepo::new("cxrs-it");
    let add = repo.run(&[
        "task",
        "add",
        "cxo echo check-contract",
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        "parallel",
        "--resource-keys",
        "repo:read",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));

    let out = repo.run(&["task", "check", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("check json");
    let fixture = load_fixture_json("task_check_contract.json");
    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "task_check.top");
    let mode = payload
        .get("recommended_mode")
        .and_then(Value::as_str)
        .expect("recommended_mode");
    let allowed_modes: Vec<String> = fixture_keys(&fixture, "allowed_modes");
    assert!(
        allowed_modes.iter().any(|m| m == mode),
        "unexpected recommended_mode: {mode}"
    );
    let strict_ok = payload
        .get("strict_plan_ok")
        .and_then(Value::as_bool)
        .expect("strict_plan_ok");
    assert_eq!(
        payload.get("can_run_mixed").and_then(Value::as_bool),
        payload.get("can_run").and_then(Value::as_bool)
    );
    let can_run_parallel = payload
        .get("can_run_parallel")
        .and_then(Value::as_bool)
        .expect("can_run_parallel");
    let sequential_waves = payload
        .get("sequential_waves")
        .and_then(Value::as_u64)
        .expect("sequential_waves");
    let parallel_waves = payload
        .get("parallel_waves")
        .and_then(Value::as_u64)
        .expect("parallel_waves");
    let largest_parallel_wave = payload
        .get("largest_parallel_wave")
        .and_then(Value::as_u64)
        .expect("largest_parallel_wave");
    if strict_ok {
        assert_eq!(
            can_run_parallel,
            payload
                .get("can_run")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        );
    }
    if parallel_waves == 0 {
        assert_eq!(largest_parallel_wave, 0, "{payload}");
    } else {
        assert!(largest_parallel_wave >= 1, "{payload}");
    }
    assert!(sequential_waves + parallel_waves >= 1, "{payload}");
    let rules = fixture
        .get("strict_reason_rules")
        .expect("strict_reason_rules");
    let null_when_ok = rules
        .get("null_when_ok")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let non_empty_when_not_ok = rules
        .get("non_empty_when_not_ok")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if strict_ok && null_when_ok {
        assert!(
            payload
                .get("strict_plan_reason")
                .is_some_and(Value::is_null),
            "{payload}"
        );
    } else if !strict_ok && non_empty_when_not_ok {
        assert!(
            payload
                .get("strict_plan_reason")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.trim().is_empty()),
            "{payload}"
        );
    }

    let blocked_keys = fixture_keys(&fixture, "blocked_keys");
    for blocked in payload
        .get("blocked")
        .and_then(Value::as_array)
        .expect("blocked array")
    {
        assert_has_keys(blocked, &blocked_keys, "task_check.blocked.item");
    }
}

#[test]
fn check_no_mutation() {
    let repo = TempRepo::new("cxrs-it");
    for i in 1..=2 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo check-nomut-{i}"),
            "--role",
            "implementer",
            "--backend",
            "primary",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let before = read_json(&repo.tasks_file());
    let out = repo.run(&["task", "check", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let after = read_json(&repo.tasks_file());
    assert_eq!(before, after, "task check must not mutate tasks");
    assert!(
        !repo.runs_log().exists(),
        "task check must not execute tasks"
    );
}

mod common;

use common::*;
use serde_json::Value;
use std::fs;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[test]
fn mixed_run_all_enforces_backend_cap_and_records_queue_ms() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
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
            "codex",
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
        "codex",
        "--backend-cap",
        "codex=1",
        "--max-workers",
        "3",
    ]);
    let elapsed_ms = started.elapsed().as_millis() as u64;
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
    }
}

#[test]
fn run_all_summary_includes_failure_taxonomy_fields() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
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
            "codex",
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
fn run_all_halt_on_critical_stops_after_first_critical_failure() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
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
            "codex",
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
}

#[cfg(unix)]
#[test]
fn run_all_continue_on_critical_processes_remaining_tasks() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
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
            "codex",
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
fn mixed_run_all_respects_dependency_waves_with_concurrency() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
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
        "codex",
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
            "codex",
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
        "codex",
        "--backend-cap",
        "codex=2",
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
fn mixed_run_all_queue_ms_increases_for_later_tasks_under_caps() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
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
            "codex",
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
        "codex",
        "--backend-cap",
        "codex=1",
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

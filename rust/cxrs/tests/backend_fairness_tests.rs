mod common;

use common::*;
use serde_json::Value;
use std::fs;
use std::time::Instant;

#[test]
fn mixed_run_all_balanced_pool_uses_both_backends_without_starvation() {
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
    repo.write_mock(
        "ollama",
        r#"#!/usr/bin/env bash
if [ "$1" = "list" ]; then
  printf '%s\n' "NAME ID SIZE MODIFIED"
  printf '%s\n' "llama3.1 abc 4GB now"
  exit 0
fi
sleep 1
printf '%s\n' "ok"
"#,
    );

    for i in 1..=8 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo fairness-{i}"),
            "--role",
            "implementer",
            "--backend",
            "auto",
            "--mode",
            "parallel",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run_with_env(
        &[
            "task",
            "run-all",
            "--status",
            "pending",
            "--mode",
            "mixed",
            "--backend-pool",
            "codex,ollama",
            "--backend-cap",
            "codex=1",
            "--backend-cap",
            "ollama=1",
            "--max-workers",
            "4",
        ],
        &[("CX_OLLAMA_MODEL", "llama3.1")],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let tasks = read_json(&repo.tasks_file());
    assert!(
        tasks
            .as_array()
            .expect("tasks array")
            .iter()
            .all(|t| t.get("status").and_then(Value::as_str) == Some("complete")),
        "not all tasks completed"
    );

    let runs = common::parse_jsonl(&repo.runs_log());
    let cxo_rows: Vec<&Value> = runs
        .iter()
        .filter(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .collect();
    let codex_count = cxo_rows
        .iter()
        .filter(|v| v.get("backend_used").and_then(Value::as_str) == Some("codex"))
        .count();
    let ollama_count = cxo_rows
        .iter()
        .filter(|v| v.get("backend_used").and_then(Value::as_str) == Some("ollama"))
        .count();
    assert!(codex_count > 0, "expected at least one codex run");
    assert!(ollama_count > 0, "expected at least one ollama run");
}

#[test]
fn mixed_run_all_falls_back_when_pool_backend_unavailable() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );
    // Intentionally do not provide ollama mock; scheduler should fallback to codex.
    for i in 1..=3 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo fallback-{i}"),
            "--role",
            "implementer",
            "--backend",
            "auto",
            "--mode",
            "parallel",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let out = repo.run_with_env(
        &[
            "task",
            "run-all",
            "--status",
            "pending",
            "--mode",
            "mixed",
            "--backend-pool",
            "codex,ollama",
            "--max-workers",
            "2",
        ],
        &[("CX_DISABLE_OLLAMA", "1")],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let runs = common::parse_jsonl(&repo.runs_log());
    let cxo_rows: Vec<&Value> = runs
        .iter()
        .filter(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .collect();
    assert!(!cxo_rows.is_empty(), "expected cxo run rows");
    assert!(
        cxo_rows
            .iter()
            .all(|v| v.get("backend_used").and_then(Value::as_str) == Some("codex")),
        "expected codex fallback for all rows: {cxo_rows:?}"
    );
}

#[test]
fn mixed_run_all_least_loaded_stress_balances_backends_and_workers() {
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
    repo.write_mock(
        "ollama",
        r#"#!/usr/bin/env bash
if [ "$1" = "list" ]; then
  printf '%s\n' "NAME ID SIZE MODIFIED"
  printf '%s\n' "llama3.1 abc 4GB now"
  exit 0
fi
sleep 1
printf '%s\n' "ok"
"#,
    );

    for i in 1..=10 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo stress-fairness-{i}"),
            "--role",
            "implementer",
            "--backend",
            "auto",
            "--mode",
            "parallel",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }

    let started = Instant::now();
    let out = repo.run_with_env(
        &[
            "task",
            "run-all",
            "--status",
            "pending",
            "--mode",
            "mixed",
            "--backend-pool",
            "codex,ollama",
            "--backend-cap",
            "codex=1",
            "--backend-cap",
            "ollama=1",
            "--max-workers",
            "4",
            "--fairness",
            "least_loaded",
        ],
        &[("CX_OLLAMA_MODEL", "llama3.1")],
    );
    let elapsed_ms = started.elapsed().as_millis() as u64;
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        elapsed_ms >= 4500,
        "expected cap-limited mixed runtime envelope, got elapsed_ms={elapsed_ms}"
    );

    let runs = common::parse_jsonl(&repo.runs_log());
    let cxo_rows: Vec<&Value> = runs
        .iter()
        .filter(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .collect();
    assert_eq!(cxo_rows.len(), 10, "expected 10 cxo rows");

    let codex_count = cxo_rows
        .iter()
        .filter(|v| v.get("backend_used").and_then(Value::as_str) == Some("codex"))
        .count();
    let ollama_count = cxo_rows
        .iter()
        .filter(|v| v.get("backend_used").and_then(Value::as_str) == Some("ollama"))
        .count();
    assert!(
        codex_count >= 3 && ollama_count >= 3,
        "expected both backends to carry load, got codex={codex_count} ollama={ollama_count}"
    );

    let mut workers = std::collections::BTreeSet::new();
    let mut queue_values: Vec<u64> = Vec::new();
    for row in &cxo_rows {
        if let Some(w) = row.get("worker_id").and_then(Value::as_str) {
            workers.insert(w.to_string());
        }
        if let Some(q) = row.get("queue_ms").and_then(Value::as_u64) {
            queue_values.push(q);
        }
    }
    assert!(
        workers.len() >= 2,
        "expected multiple workers, got {workers:?}"
    );
    queue_values.sort_unstable();
    assert_eq!(
        queue_values.len(),
        10,
        "expected queue telemetry for all rows, got {queue_values:?}"
    );
    assert!(
        queue_values.last().copied().unwrap_or(0) >= 3000,
        "expected late queued tasks under cap pressure, got {queue_values:?}"
    );
}

#[test]
fn mixed_run_all_round_robin_assigns_backends_deterministically() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );
    repo.write_mock(
        "ollama",
        r#"#!/usr/bin/env bash
if [ "$1" = "list" ]; then
  printf '%s\n' "NAME ID SIZE MODIFIED"
  printf '%s\n' "llama3.1 abc 4GB now"
  exit 0
fi
printf '%s\n' "ok"
"#,
    );

    let mut ids: Vec<String> = Vec::new();
    for i in 1..=6 {
        let add = repo.run(&[
            "task",
            "add",
            &format!("cxo echo rr-{i}"),
            "--role",
            "implementer",
            "--backend",
            "auto",
            "--mode",
            "parallel",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
        ids.push(stdout_str(&add).trim().to_string());
    }

    let run = repo.run_with_env(
        &[
            "task",
            "run-all",
            "--status",
            "pending",
            "--mode",
            "mixed",
            "--backend-pool",
            "codex,ollama",
            "--backend-cap",
            "codex=1",
            "--backend-cap",
            "ollama=1",
            "--max-workers",
            "2",
            "--fairness",
            "round_robin",
        ],
        &[
            ("CX_OLLAMA_MODEL", "llama3.1"),
            ("CX_BROKER_POLICY", "balanced"),
        ],
    );
    assert!(
        run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&run),
        stderr_str(&run)
    );

    let runs = common::parse_jsonl(&repo.runs_log());
    for (idx, id) in ids.iter().enumerate() {
        let row = runs
            .iter()
            .find(|v| {
                v.get("tool").and_then(Value::as_str) == Some("cxo")
                    && v.get("task_id").and_then(Value::as_str) == Some(id.as_str())
            })
            .unwrap_or_else(|| panic!("missing run row for task {id}"));
        let backend = row
            .get("backend_used")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let expected = if idx % 2 == 0 { "codex" } else { "ollama" };
        assert_eq!(
            backend, expected,
            "unexpected backend for task {} (idx={}): row={row:?}",
            id, idx
        );
    }
}

#[test]
fn mixed_run_all_errors_when_pool_has_no_available_backends() {
    let repo = TempRepo::new("cxrs-it");
    let add = repo.run(&[
        "task",
        "add",
        "cxo echo unavailable-backends",
        "--role",
        "implementer",
        "--backend",
        "auto",
        "--mode",
        "parallel",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));

    let out = repo.run_with_env(
        &[
            "task",
            "run-all",
            "--status",
            "pending",
            "--mode",
            "mixed",
            "--backend-pool",
            "codex,ollama",
            "--max-workers",
            "2",
        ],
        &[("CX_DISABLE_CODEX", "1"), ("CX_DISABLE_OLLAMA", "1")],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected failure when all backends unavailable; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("no available backend from --backend-pool"),
        "stderr={}",
        stderr_str(&out)
    );
}

#[test]
fn broker_benchmark_json_reports_backend_stats() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let mut text = String::new();
    let rows = vec![
        serde_json::json!({
            "execution_id":"b1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1000,"schema_enforced":false,"schema_valid":true,"effective_input_tokens":100,"output_tokens":20
        }),
        serde_json::json!({
            "execution_id":"b2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":3000,"schema_enforced":false,"schema_valid":true,"effective_input_tokens":300,"output_tokens":60
        }),
        serde_json::json!({
            "execution_id":"b3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"ollama","backend_selected":"ollama","capture_provider":"native","execution_mode":"lean",
            "duration_ms":2000,"schema_enforced":false,"schema_valid":true,"effective_input_tokens":50,"output_tokens":10
        }),
    ];
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&[
        "broker",
        "benchmark",
        "--backend",
        "codex",
        "--backend",
        "ollama",
        "--window",
        "10",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("benchmark json");
    assert_eq!(v.get("window").and_then(Value::as_u64), Some(10));
    let summary = v
        .get("summary")
        .and_then(Value::as_array)
        .expect("summary array");
    assert_eq!(summary.len(), 2, "summary={summary:?}");

    let codex = summary
        .iter()
        .find(|row| row.get("backend").and_then(Value::as_str) == Some("codex"))
        .expect("codex summary row");
    assert_eq!(codex.get("runs").and_then(Value::as_u64), Some(2));
    assert_eq!(
        codex.get("avg_duration_ms").and_then(Value::as_u64),
        Some(2000)
    );
    assert_eq!(
        codex
            .get("avg_effective_input_tokens")
            .and_then(Value::as_u64),
        Some(200)
    );
    assert_eq!(
        codex.get("avg_output_tokens").and_then(Value::as_u64),
        Some(40)
    );
}

#[test]
fn broker_benchmark_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"bb1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1200,"schema_enforced":false,"schema_valid":true,"effective_input_tokens":100,"output_tokens":20
        }),
        serde_json::json!({
            "execution_id":"bb2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"ollama","backend_selected":"ollama","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1400,"schema_enforced":false,"schema_valid":true,"effective_input_tokens":90,"output_tokens":18
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["broker", "benchmark", "--window", "10", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("broker benchmark json");
    let fixture = load_fixture_json("broker_benchmark_json_contract.json");

    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "broker.benchmark");
    let item_keys = fixture_keys(&fixture, "summary_item_keys");
    let summary = payload
        .get("summary")
        .and_then(Value::as_array)
        .expect("summary array");
    assert!(!summary.is_empty(), "summary array is empty");
    assert_eq!(payload.get("strict").and_then(Value::as_bool), Some(false));
    assert_eq!(payload.get("min_runs").and_then(Value::as_u64), Some(1));
    assert_eq!(
        payload.get("severity").and_then(Value::as_str),
        Some("critical")
    );
    let count_keys = fixture_keys(&fixture, "violation_counts_keys");
    assert_has_keys(
        payload.get("violation_counts").expect("violation_counts"),
        &count_keys,
        "broker.benchmark.violation_counts",
    );
    assert_eq!(
        payload
            .get("violations")
            .and_then(Value::as_array)
            .map(|v| v.len()),
        Some(0)
    );
    for item in summary {
        assert_has_keys(item, &item_keys, "broker.benchmark.summary[*]");
    }
}

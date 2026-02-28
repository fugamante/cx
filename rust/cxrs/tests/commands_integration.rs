mod common;

use common::{TempRepo, read_json, stderr_str, stdout_str};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

fn parse_labeled_u64(s: &str, label: &str) -> Option<u64> {
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix(label) {
            return rest.trim().parse::<u64>().ok();
        }
    }
    None
}

#[derive(Clone, Debug, Default)]
struct FixtureHttpRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    body: String,
}

fn run_fixture_http_server_once(
    response_json: &str,
) -> (
    String,
    Arc<Mutex<Option<FixtureHttpRequest>>>,
    JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture http server");
    let addr = listener.local_addr().expect("fixture local addr");
    let captured = Arc::new(Mutex::new(None));
    let captured_bg = Arc::clone(&captured);
    let response = response_json.to_string();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("fixture accept");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("fixture set read timeout");
        let mut buf = vec![0u8; 64 * 1024];
        let mut req = Vec::new();
        loop {
            let n = stream.read(&mut buf).expect("fixture read");
            if n == 0 {
                break;
            }
            req.extend_from_slice(&buf[..n]);
            if req.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let headers_end = req
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| i + 4)
            .expect("fixture request headers");
        let head = String::from_utf8_lossy(&req[..headers_end]).to_string();
        let mut lines = head.lines();
        let req_line = lines.next().unwrap_or_default();
        let mut req_parts = req_line.split_whitespace();
        let method = req_parts.next().unwrap_or_default().to_string();
        let path = req_parts.next().unwrap_or_default().to_string();
        let mut content_len = 0usize;
        let mut auth = None;
        for line in lines {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("content-length:")
                && let Some(v) = line.split(':').nth(1)
            {
                content_len = v.trim().parse::<usize>().unwrap_or(0);
            }
            if lower.starts_with("authorization:")
                && let Some(v) = line.split(':').nth(1)
            {
                auth = Some(v.trim().to_string());
            }
        }
        let mut body = req[headers_end..].to_vec();
        while body.len() < content_len {
            let n = stream.read(&mut buf).expect("fixture read body");
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n]);
        }
        let body = String::from_utf8_lossy(&body).to_string();
        if let Ok(mut slot) = captured_bg.lock() {
            *slot = Some(FixtureHttpRequest {
                method,
                path,
                authorization: auth,
                body,
            });
        }
        let response_bytes = response.as_bytes();
        let http = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_bytes.len(),
            response
        );
        stream
            .write_all(http.as_bytes())
            .expect("fixture write response");
        let _ = stream.flush();
    });
    (format!("http://{}/infer", addr), captured, handle)
}

fn load_fixture_json(name: &str) -> Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push(name);
    let content = fs::read_to_string(path).expect("read fixture file");
    serde_json::from_str(&content).expect("parse fixture json")
}

fn fixture_keys(fixture: &Value, key: &str) -> Vec<String> {
    fixture
        .get(key)
        .and_then(Value::as_array)
        .expect("fixture key array")
        .iter()
        .map(|v| v.as_str().expect("fixture key string").to_string())
        .collect()
}

fn assert_has_keys(obj: &Value, keys: &[String], context: &str) {
    for key in keys {
        assert!(
            obj.get(key).is_some(),
            "{context} missing key '{key}' in payload: {obj}"
        );
    }
}

fn assert_fixture_contract(
    payload: &Value,
    fixture: &Value,
    top_level_key_field: &str,
    sections: &[(&str, &str, &str)],
) {
    let top_keys = fixture_keys(fixture, top_level_key_field);
    assert_has_keys(payload, &top_keys, "contract.top");
    for (payload_section, fixture_keys_field, context) in sections {
        let keys = fixture_keys(fixture, fixture_keys_field);
        assert_has_keys(
            payload.get(payload_section).expect("fixture section"),
            &keys,
            context,
        );
    }
}

#[test]
fn task_lifecycle_add_claim_complete() {
    let repo = TempRepo::new("cxrs-it");

    let add = repo.run(&[
        "task",
        "add",
        "Implement parser hardening",
        "--role",
        "implementer",
    ]);
    assert!(
        add.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add),
        stderr_str(&add)
    );
    let id = stdout_str(&add).trim().to_string();
    assert!(id.starts_with("task_"), "unexpected task id: {id}");

    let claim = repo.run(&["task", "claim", &id]);
    assert!(claim.status.success(), "stderr={}", stderr_str(&claim));
    assert!(stdout_str(&claim).contains("in_progress"));

    let complete = repo.run(&["task", "complete", &id]);
    assert!(
        complete.status.success(),
        "stderr={}",
        stderr_str(&complete)
    );
    assert!(stdout_str(&complete).contains("complete"));

    let tasks = read_json(&repo.tasks_file());
    let task = tasks
        .as_array()
        .expect("tasks array")
        .iter()
        .find(|t| t.get("id").and_then(Value::as_str) == Some(id.as_str()))
        .expect("task exists");
    assert_eq!(task.get("status").and_then(Value::as_str), Some("complete"));
}

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
fn run_all_retries_timeout_then_succeeds_and_logs_attempts() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
cat >/dev/null
attempt="${CX_TASK_RETRY_ATTEMPT:-1}"
if [ "$attempt" = "1" ]; then
  sleep 2
  exit 0
fi
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    let add = repo.run(&[
        "task",
        "add",
        "cxo echo retry-timeout-once",
        "--role",
        "implementer",
        "--backend",
        "codex",
        "--max-retries",
        "1",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let task_id = stdout_str(&add).trim().to_string();

    let out = repo.run_with_env(
        &["task", "run-all", "--status", "pending"],
        &[("CX_TIMEOUT_LLM_SECS", "1")],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let summary = stdout_str(&out);
    assert!(summary.contains("complete=1"), "{summary}");
    assert!(summary.contains("failed=0"), "{summary}");

    let runs = common::parse_jsonl(&repo.runs_log());
    let rows: Vec<&Value> = runs
        .iter()
        .filter(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .collect();
    assert!(
        rows.len() >= 2,
        "expected at least two cxo attempts for task {task_id}, got {} rows: {rows:?}",
        rows.len(),
    );

    let attempts: std::collections::BTreeSet<u64> = rows
        .iter()
        .filter_map(|v| v.get("retry_attempt").and_then(Value::as_u64))
        .collect();
    assert!(
        attempts.contains(&1),
        "missing retry attempt 1 in rows: {rows:?}"
    );
    assert!(
        attempts.contains(&2),
        "missing retry attempt 2 in rows: {rows:?}"
    );

    let first_timeout = rows.iter().any(|v| {
        v.get("retry_attempt").and_then(Value::as_u64) == Some(1)
            && v.get("timed_out").and_then(Value::as_bool) == Some(true)
    });
    assert!(first_timeout, "expected timed_out=true on attempt 1");
}

#[test]
fn judge_convergence_uses_model_path_and_logs_decision_source() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
prompt="$(cat)"
if printf '%s' "$prompt" | grep -q "Select the best candidate index"; then
  printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"{\"winner_index\":1,\"reason\":\"prefer success\"}"}}'
  printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":40,"cached_input_tokens":4,"output_tokens":8}}'
else
  printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
  printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
fi
"#,
    );

    let add = repo.run(&[
        "task",
        "add",
        "cxo echo judge-path",
        "--role",
        "implementer",
        "--backend",
        "codex",
        "--converge",
        "judge",
        "--replicas",
        "2",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let id = stdout_str(&add).trim().to_string();

    let run = repo.run(&["task", "run", &id]);
    assert!(
        run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&run),
        stderr_str(&run)
    );

    let runs = common::parse_jsonl(&repo.runs_log());
    let converge_row = runs
        .iter()
        .rev()
        .find(|v| v.get("tool").and_then(Value::as_str) == Some("cxtask_converge"))
        .expect("converge row");
    let votes = converge_row
        .get("converge_votes")
        .and_then(Value::as_object)
        .expect("converge votes object");
    assert_eq!(
        votes.get("decision_source").and_then(Value::as_str),
        Some("model_judge"),
        "unexpected converge votes: {votes:?}"
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

#[test]
fn schema_failure_creates_quarantine_and_logs() {
    let repo = TempRepo::new("cxrs-it");

    // Mock codex JSONL output with invalid JSON payload for schema commands.
    repo.write_mock_codex(
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"not-json"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":123,"cached_input_tokens":23,"output_tokens":10}}'
"#,
    );

    let out = repo.run(&["next", "echo", "hello"]);
    assert!(
        !out.status.success(),
        "expected schema failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let qdir = repo.quarantine_dir();
    let mut q_entries: Vec<std::fs::DirEntry> = Vec::new();
    for _ in 0..20 {
        if let Ok(rd) = fs::read_dir(&qdir) {
            q_entries = rd.filter_map(Result::ok).collect();
            if !q_entries.is_empty() {
                break;
            }
        }
        sleep(Duration::from_millis(50));
    }
    assert!(
        !q_entries.is_empty(),
        "expected quarantine entries in {}",
        qdir.display()
    );

    let sf_log = fs::read_to_string(repo.schema_fail_log()).expect("read schema fail log");
    let sf_last: Value = serde_json::from_str(sf_log.lines().last().expect("schema fail line"))
        .expect("schema fail json");
    let qid = sf_last
        .get("quarantine_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(!qid.is_empty(), "schema failure log missing quarantine_id");

    let runs = fs::read_to_string(repo.runs_log()).expect("read runs log");
    let run_last: Value =
        serde_json::from_str(runs.lines().last().expect("runs line")).expect("runs json");
    assert_eq!(
        run_last.get("schema_valid").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        run_last.get("quarantine_id").and_then(Value::as_str),
        Some(qid)
    );
}

#[test]
fn command_parsing_and_file_io_edge_cases() {
    let repo = TempRepo::new("cxrs-it");

    let bad_status = repo.run(&["task", "list", "--status", "bogus"]);
    assert_eq!(bad_status.status.code(), Some(2));
    assert!(
        stderr_str(&bad_status).contains("invalid status"),
        "stderr={}",
        stderr_str(&bad_status)
    );

    fs::create_dir_all(repo.tasks_file().parent().expect("tasks parent"))
        .expect("mkdir tasks parent");
    fs::write(repo.tasks_file(), "{ this-is: not-json ]").expect("write invalid tasks.json");

    let list = repo.run(&["task", "list"]);
    assert_eq!(list.status.code(), Some(1));
    assert!(
        stderr_str(&list).contains("invalid JSON"),
        "stderr={}",
        stderr_str(&list)
    );

    let unknown_flag = repo.run(&["task", "run", "task_001", "--what"]);
    assert_eq!(unknown_flag.status.code(), Some(2));
    assert!(stderr_str(&unknown_flag).contains("unknown flag"));
}

#[test]
fn logs_stats_and_telemetry_alias_report_population_and_drift() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row1 = serde_json::json!({
        "execution_id":"e1","timestamp":"2026-01-01T00:00:00Z","command":"cx",
        "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
        "duration_ms":10,"schema_enforced":false,"schema_valid":true
    });
    let row2 = serde_json::json!({
        "execution_id":"e2","timestamp":"2026-01-01T00:00:01Z","command":"next",
        "backend_used":"codex","capture_provider":"native","execution_mode":"deterministic",
        "duration_ms":20,"schema_enforced":true,"schema_valid":true,"task_id":"task_001",
        "retry_attempt":2,"timed_out":false
    });
    let mut text = serde_json::to_string(&row1).expect("row1");
    text.push('\n');
    text.push_str(&serde_json::to_string(&row2).expect("row2"));
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["logs", "stats", "2"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let stdout = stdout_str(&out);
    assert!(stdout.contains("logs stats"), "{stdout}");
    assert!(stdout.contains("retry_telemetry"), "{stdout}");
    assert!(stdout.contains("retry_attempt_histogram"), "{stdout}");
    assert!(stdout.contains("field_population"), "{stdout}");
    assert!(stdout.contains("contract_drift"), "{stdout}");
    assert!(stdout.contains("new_keys_second_half"), "{stdout}");

    let out_json = repo.run(&["telemetry", "2", "--json"]);
    assert!(
        out_json.status.success(),
        "stderr={}",
        stderr_str(&out_json)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out_json)).expect("json output");
    assert_eq!(v.get("window_runs").and_then(Value::as_u64), Some(2));
    let fields = v
        .get("fields")
        .and_then(Value::as_array)
        .expect("fields array");
    assert!(
        fields
            .iter()
            .any(|f| f.get("field").and_then(Value::as_str) == Some("execution_id")),
        "missing execution_id coverage field: {v}"
    );
    assert!(
        fields.iter().all(|f| {
            f.get("field").is_some()
                && f.get("present").and_then(Value::as_u64).is_some()
                && f.get("non_null").and_then(Value::as_u64).is_some()
                && f.get("total").and_then(Value::as_u64).is_some()
        }),
        "invalid field shape in telemetry payload: {v}"
    );
    let drift = v.get("contract_drift").expect("contract_drift");
    assert!(drift.get("new_keys_second_half").is_some());
    assert!(drift.get("missing_keys_second_half").is_some());
    let retry = v.get("retry_telemetry").expect("retry_telemetry");
    assert!(retry.get("rows_with_retry_metadata").is_some());
    assert!(retry.get("rows_after_retry_success_rate").is_some());
    assert!(retry.get("attempt_histogram").is_some());
}

#[test]
fn logs_stats_strict_and_severity_flags_behave_as_expected() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let weak_row = serde_json::json!({
        "execution_id":"e1","timestamp":"2026-01-01T00:00:00Z","command":"cx",
        "backend_used":"codex","capture_provider":"native","execution_mode":"lean"
    });
    let mut text = serde_json::to_string(&weak_row).expect("row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let strict = repo.run(&["logs", "stats", "1", "--strict"]);
    assert_eq!(
        strict.status.code(),
        Some(1),
        "stdout={}",
        stdout_str(&strict)
    );
    let strict_out = stdout_str(&strict);
    assert!(strict_out.contains("severity: critical"), "{strict_out}");
    assert!(strict_out.contains("strict_violations"), "{strict_out}");

    let severity_only = repo.run(&["telemetry", "1", "--severity"]);
    assert!(
        severity_only.status.success(),
        "stderr={}",
        stderr_str(&severity_only)
    );
    let sev_out = stdout_str(&severity_only);
    assert!(sev_out.contains("severity:"), "{sev_out}");
    assert!(!sev_out.contains("field_population"), "{sev_out}");

    let validate_default = repo.run(&["logs", "validate"]);
    assert!(
        validate_default.status.success(),
        "stdout={} stderr={}",
        stdout_str(&validate_default),
        stderr_str(&validate_default)
    );
    let validate_default_out = stdout_str(&validate_default);
    assert!(
        validate_default_out.contains("status: ok_with_warnings"),
        "{validate_default_out}"
    );

    let validate = repo.run(&["logs", "validate", "--strict"]);
    assert_eq!(
        validate.status.code(),
        Some(1),
        "stdout={} stderr={}",
        stdout_str(&validate),
        stderr_str(&validate)
    );
    let validate_out = stdout_str(&validate);
    let issue_count = parse_labeled_u64(&validate_out, "issue_count:")
        .expect("issue_count in logs validate output");

    let telemetry_json = repo.run(&["telemetry", "1", "--json"]);
    assert!(
        telemetry_json.status.success(),
        "stderr={}",
        stderr_str(&telemetry_json)
    );
    let v: Value = serde_json::from_str(&stdout_str(&telemetry_json)).expect("telemetry json");
    let required = v
        .get("required_fields")
        .and_then(Value::as_u64)
        .expect("required_fields");
    let strict_violations = v
        .get("strict_violations")
        .and_then(Value::as_u64)
        .expect("strict_violations");

    assert_eq!(
        issue_count, strict_violations,
        "logs validate and telemetry strict violation counts diverged"
    );
    assert_eq!(required, 33, "unexpected strict contract field count");
}

#[cfg(target_os = "macos")]
#[test]
fn telemetry_json_output_is_stable_on_macos() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row = serde_json::json!({
        "execution_id":"m1","timestamp":"2026-01-01T00:00:00Z","command":"cx",
        "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
        "duration_ms":12,"schema_enforced":false,"schema_valid":true
    });
    let mut text = serde_json::to_string(&row).expect("row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out1 = repo.run(&["telemetry", "1", "--json"]);
    let out2 = repo.run(&["telemetry", "1", "--json"]);
    assert!(out1.status.success(), "stderr={}", stderr_str(&out1));
    assert!(out2.status.success(), "stderr={}", stderr_str(&out2));

    let v1: Value = serde_json::from_str(&stdout_str(&out1)).expect("json1");
    let v2: Value = serde_json::from_str(&stdout_str(&out2)).expect("json2");
    assert_eq!(
        v1.get("window_runs").and_then(Value::as_u64),
        Some(1),
        "unexpected telemetry window: {v1}"
    );
    assert_eq!(v1, v2, "telemetry output drifted on repeated invocation");
}

#[test]
fn adapter_telemetry_fields_present_for_codex_runs() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_codex(
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":1,"output_tokens":2}}'
"#,
    );
    let out = repo.run(&["cxo", "echo", "adapter-codex"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let runs = common::parse_jsonl(&repo.runs_log());
    let row = runs
        .iter()
        .rev()
        .find(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .expect("cxo row");
    assert_eq!(
        row.get("adapter_type").and_then(Value::as_str),
        Some("codex-cli"),
        "row={row}"
    );
    assert_eq!(
        row.get("provider_transport").and_then(Value::as_str),
        Some("process"),
        "row={row}"
    );
    assert!(
        row.get("provider_status").is_some()
            && row.get("provider_status").is_some_and(Value::is_null),
        "expected provider_status=null, row={row}"
    );
}

#[test]
fn adapter_telemetry_fields_present_for_ollama_runs() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "ollama",
        r#"#!/usr/bin/env bash
if [ "$1" = "list" ]; then
  printf '%s\n' "NAME ID SIZE MODIFIED"
  printf '%s\n' "llama3.1 abc 4GB now"
  exit 0
fi
cat >/dev/null
printf '%s\n' "ok"
"#,
    );
    let out = repo.run_with_env(
        &["cxo", "echo", "adapter-ollama"],
        &[
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "llama3.1"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let runs = common::parse_jsonl(&repo.runs_log());
    let row = runs
        .iter()
        .rev()
        .find(|v| v.get("tool").and_then(Value::as_str) == Some("cxo"))
        .expect("cxo row");
    assert_eq!(
        row.get("adapter_type").and_then(Value::as_str),
        Some("ollama-cli"),
        "row={row}"
    );
    assert_eq!(
        row.get("provider_transport").and_then(Value::as_str),
        Some("process"),
        "row={row}"
    );
    assert!(
        row.get("provider_status").is_some()
            && row.get("provider_status").is_some_and(Value::is_null),
        "expected provider_status=null, row={row}"
    );
}

#[test]
fn mock_adapter_next_schema_success_without_provider_binaries() {
    let repo = TempRepo::new("cxrs-it");
    let out = repo.run_with_env(
        &["next", "echo", "mock-adapter"],
        &[
            ("CX_PROVIDER_ADAPTER", "mock"),
            (
                "CX_MOCK_PLAIN_RESPONSE",
                "{\"commands\":[\"echo ok-from-mock\"]}",
            ),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let stdout = stdout_str(&out);
    assert!(
        stdout.contains("echo ok-from-mock"),
        "unexpected stdout: {stdout}"
    );

    let runs = common::parse_jsonl(&repo.runs_log());
    let row = runs
        .iter()
        .rev()
        .find(|v| v.get("tool").and_then(Value::as_str) == Some("cxrs_next"))
        .expect("cxrs_next row");
    assert_eq!(
        row.get("adapter_type").and_then(Value::as_str),
        Some("mock"),
        "row={row}"
    );
    assert_eq!(
        row.get("provider_transport").and_then(Value::as_str),
        Some("mock"),
        "row={row}"
    );
}

#[test]
fn mock_adapter_schema_failure_creates_quarantine_and_logs() {
    let repo = TempRepo::new("cxrs-it");
    let out = repo.run_with_env(
        &["next", "echo", "mock-fail"],
        &[
            ("CX_PROVIDER_ADAPTER", "mock"),
            ("CX_MOCK_PLAIN_RESPONSE", "not-json"),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected schema failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let qdir = repo.quarantine_dir();
    let entries = fs::read_dir(&qdir)
        .expect("read quarantine dir")
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    assert!(
        !entries.is_empty(),
        "expected quarantine entries in {}",
        qdir.display()
    );
    let schema_fail_log = repo.schema_fail_log();
    let last_fail = common::parse_jsonl(&schema_fail_log)
        .into_iter()
        .last()
        .expect("schema failure log row");
    let qid = last_fail
        .get("quarantine_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(!qid.is_empty(), "schema failure log missing quarantine_id");

    let run_last = common::parse_jsonl(&repo.runs_log())
        .into_iter()
        .last()
        .expect("last run row");
    assert_eq!(
        run_last.get("schema_valid").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        run_last.get("adapter_type").and_then(Value::as_str),
        Some("mock")
    );
    assert_eq!(
        run_last.get("provider_transport").and_then(Value::as_str),
        Some("mock")
    );
}

#[test]
fn schema_command_parity_next_between_codex_cli_and_mock_adapter() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_codex(
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"{\"commands\":[\"echo parity-next\"]}"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":14,"cached_input_tokens":2,"output_tokens":4}}'
"#,
    );

    let codex_out = repo.run(&["next", "echo", "parity-next"]);
    assert!(
        codex_out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&codex_out),
        stderr_str(&codex_out)
    );
    let codex_stdout = stdout_str(&codex_out);
    let codex_lines: Vec<String> = codex_stdout
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();
    assert_eq!(codex_lines, vec!["echo parity-next".to_string()]);

    let mock_out = repo.run_with_env(
        &["next", "echo", "parity-next"],
        &[
            ("CX_PROVIDER_ADAPTER", "mock"),
            (
                "CX_MOCK_PLAIN_RESPONSE",
                "{\"commands\":[\"echo parity-next\"]}",
            ),
        ],
    );
    assert!(
        mock_out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&mock_out),
        stderr_str(&mock_out)
    );
    let mock_stdout = stdout_str(&mock_out);
    let mock_lines: Vec<String> = mock_stdout
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();
    assert_eq!(mock_lines, vec!["echo parity-next".to_string()]);
    assert_eq!(
        codex_lines, mock_lines,
        "next output diverged between codex-cli and mock adapter"
    );
}

#[test]
fn http_stub_adapter_fails_fast_and_logs_http_transport_status() {
    let repo = TempRepo::new("cxrs-it");
    let out = repo.run_with_env(
        &["cxo", "echo", "http-stub"],
        &[("CX_PROVIDER_ADAPTER", "http-stub")],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("http-stub adapter selected"),
        "stderr={}",
        stderr_str(&out)
    );

    let run_last = common::parse_jsonl(&repo.runs_log())
        .into_iter()
        .last()
        .expect("last run row");
    assert_eq!(
        run_last.get("adapter_type").and_then(Value::as_str),
        Some("http-stub")
    );
    assert_eq!(
        run_last.get("provider_transport").and_then(Value::as_str),
        Some("http")
    );
    assert_eq!(
        run_last.get("provider_status").and_then(Value::as_str),
        Some("stub_unimplemented")
    );
}

#[test]
fn http_curl_adapter_requires_url_and_logs_experimental_status() {
    let repo = TempRepo::new("cxrs-it");
    let out = repo.run_with_env(
        &["cxo", "echo", "http-curl"],
        &[("CX_PROVIDER_ADAPTER", "http-curl")],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("CX_HTTP_PROVIDER_URL"),
        "stderr={}",
        stderr_str(&out)
    );
    let run_last = common::parse_jsonl(&repo.runs_log())
        .into_iter()
        .last()
        .expect("last run row");
    assert_eq!(
        run_last.get("adapter_type").and_then(Value::as_str),
        Some("http-curl")
    );
    assert_eq!(
        run_last.get("provider_transport").and_then(Value::as_str),
        Some("http")
    );
    assert_eq!(
        run_last.get("provider_status").and_then(Value::as_str),
        Some("experimental")
    );
}

#[test]
fn http_curl_adapter_parses_json_text_payload_from_curl() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"text":"http adapter ok"}'
"#,
    );
    let out = repo.run_with_env(
        &["cxo", "echo", "http-curl"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:9999/infer"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "http adapter ok");

    let run_last = common::parse_jsonl(&repo.runs_log())
        .into_iter()
        .last()
        .expect("last run row");
    assert_eq!(
        run_last.get("adapter_type").and_then(Value::as_str),
        Some("http-curl")
    );
    assert_eq!(
        run_last.get("provider_transport").and_then(Value::as_str),
        Some("http")
    );
}

#[test]
fn http_curl_adapter_hits_local_server_and_sends_auth_and_prompt() {
    if std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let repo = TempRepo::new("cxrs-it");
    let (url, captured, handle) = run_fixture_http_server_once(r#"{"text":"fixture-http-ok"}"#);
    let out = repo.run_with_env(
        &["cxo", "echo", "http-live"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", &url),
            ("CX_HTTP_PROVIDER_TOKEN", "token-123"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "fixture-http-ok");
    handle.join().expect("fixture join");

    let req = captured
        .lock()
        .expect("fixture lock")
        .clone()
        .expect("captured request");
    assert_eq!(req.method, "POST");
    assert_eq!(req.path, "/infer");
    assert_eq!(req.authorization.as_deref(), Some("Bearer token-123"));
    assert!(
        req.body.contains("http-live"),
        "expected prompt/body to include command output context, body={}",
        req.body
    );
}

#[test]
fn http_curl_adapter_non_200_is_classified_as_http_status() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat >/dev/null
echo "curl: (22) The requested URL returned error: 503" >&2
exit 22
"#,
    );
    let out = repo.run_with_env(
        &["cxo", "echo", "http-503"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:9999/infer"),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("http provider [http_status]"),
        "stderr={}",
        stderr_str(&out)
    );
}

#[test]
fn http_curl_adapter_transport_failure_is_classified_as_unreachable() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat >/dev/null
echo "curl: (7) Failed to connect to 127.0.0.1 port 9999: Connection refused" >&2
exit 7
"#,
    );
    let out = repo.run_with_env(
        &["cxo", "echo", "http-down"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:9999/infer"),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("http provider [transport_unreachable]"),
        "stderr={}",
        stderr_str(&out)
    );
}

#[test]
fn http_curl_adapter_unknown_json_envelope_falls_back_to_raw_text() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"unexpected":"shape"}'
"#,
    );
    let out = repo.run_with_env(
        &["cxo", "echo", "http-raw-fallback"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:9999/infer"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), r#"{"unexpected":"shape"}"#);
}

#[test]
fn http_curl_adapter_json_format_supports_schema_commands() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"commands":["echo via-http-json"]}'
"#,
    );
    let out = repo.run_with_env(
        &["next", "echo", "http-json-schema"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:9999/infer"),
            ("CX_HTTP_PROVIDER_FORMAT", "json"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "echo via-http-json");
}

#[test]
fn http_curl_adapter_jsonl_format_passthrough_for_cxj() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"jsonl-ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":3,"cached_input_tokens":1,"output_tokens":1}}'
"#,
    );
    let out = repo.run_with_env(
        &["cxj", "echo", "http-jsonl"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:9999/infer"),
            ("CX_HTTP_PROVIDER_FORMAT", "jsonl"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let stdout = stdout_str(&out);
    assert!(stdout.contains(r#""type":"item.completed""#), "{stdout}");
    assert!(stdout.contains(r#""type":"turn.completed""#), "{stdout}");
}

#[test]
fn http_curl_adapter_jsonl_format_rejects_non_jsonl_payload() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"unexpected":"shape"}'
"#,
    );
    let out = repo.run_with_env(
        &["cxj", "echo", "http-jsonl-bad"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:9999/infer"),
            ("CX_HTTP_PROVIDER_FORMAT", "jsonl"),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("jsonl payload missing item.completed"),
        "stderr={}",
        stderr_str(&out)
    );
}

#[test]
fn diag_reports_scheduler_distribution_fields() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"d1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,"queue_ms":0,"worker_id":"w1"
        }),
        serde_json::json!({
            "execution_id":"d2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"ollama","backend_selected":"ollama","capture_provider":"native","execution_mode":"lean",
            "duration_ms":12,"schema_enforced":false,"schema_valid":true,"queue_ms":900,"worker_id":"w2"
        }),
        serde_json::json!({
            "execution_id":"d3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":14,"schema_enforced":false,"schema_valid":true,"queue_ms":1800,"worker_id":"w1"
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let stdout = stdout_str(&out);
    assert!(stdout.contains("scheduler_window_runs: 3"), "{stdout}");
    assert!(stdout.contains("scheduler_queue_rows: 3"), "{stdout}");
    assert!(stdout.contains("scheduler_workers_seen: w1,w2"), "{stdout}");
    assert!(
        stdout.contains("scheduler_worker_distribution: w1=2,w2=1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("scheduler_backend_distribution: codex=2,ollama=1"),
        "{stdout}"
    );
    assert!(stdout.contains("retry_rows_with_metadata:"), "{stdout}");
    assert!(stdout.contains("retry_attempt_histogram:"), "{stdout}");
}

#[test]
fn diag_json_reports_scheduler_object() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row = serde_json::json!({
        "execution_id":"dj1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
        "duration_ms":10,"schema_enforced":false,"schema_valid":true,"queue_ms":500,"worker_id":"w1"
    });
    let mut text = serde_json::to_string(&row).expect("serialize row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    assert_eq!(v.get("backend").and_then(Value::as_str), Some("codex"));
    assert_eq!(
        v.get("scheduler_window_requested").and_then(Value::as_u64),
        Some(200)
    );
    let scheduler = v.get("scheduler").expect("scheduler");
    assert_eq!(scheduler.get("queue_rows").and_then(Value::as_u64), Some(1));
    let worker_dist = scheduler
        .get("worker_distribution")
        .and_then(Value::as_object)
        .expect("worker distribution");
    assert_eq!(
        worker_dist.get("w1").and_then(Value::as_u64),
        Some(1),
        "unexpected scheduler object: {scheduler}"
    );
    let retry = v.get("retry").expect("retry");
    assert_eq!(retry.get("window_runs").and_then(Value::as_u64), Some(1));
    assert!(
        retry.get("attempt_histogram").is_some(),
        "unexpected retry object: {retry}"
    );
}

#[test]
fn diag_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row = serde_json::json!({
        "execution_id":"diagfx1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
        "duration_ms":10,"schema_enforced":false,"schema_valid":true,"queue_ms":250,"worker_id":"w1"
    });
    let mut text = serde_json::to_string(&row).expect("serialize row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag", "--json", "--window", "1"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    let fixture = load_fixture_json("diag_json_contract.json");

    assert_fixture_contract(
        &payload,
        &fixture,
        "top_level_keys",
        &[
            ("routing_trace", "routing_trace_keys", "diag.routing_trace"),
            ("scheduler", "scheduler_keys", "diag.scheduler"),
            ("retry", "retry_keys", "diag.retry"),
        ],
    );
}

#[test]
fn diag_json_window_scopes_scheduler_rows() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let mut text = String::new();
    for i in 1..=3u64 {
        let row = serde_json::json!({
            "execution_id":format!("dw{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":i * 100,"worker_id":"w1"
        });
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag", "--json", "--window", "1"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    assert_eq!(
        v.get("scheduler_window_requested").and_then(Value::as_u64),
        Some(1)
    );
    let scheduler = v.get("scheduler").expect("scheduler");
    assert_eq!(
        scheduler.get("window_runs").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(scheduler.get("queue_rows").and_then(Value::as_u64), Some(1));
}

#[test]
fn diag_json_strict_fails_on_high_queue_severity() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let mut text = String::new();
    for i in 1..=6u64 {
        let row = serde_json::json!({
            "execution_id":format!("ds{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":3000 + i * 10,"worker_id":"w1"
        });
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag", "--json", "--strict"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    assert_ne!(v.get("severity").and_then(Value::as_str), Some("ok"));
    let reasons = v
        .get("severity_reasons")
        .and_then(Value::as_array)
        .expect("severity reasons array");
    assert!(
        !reasons.is_empty(),
        "expected severity reasons in strict mode"
    );
}

#[test]
fn diag_json_strict_passes_on_ok_severity() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row = serde_json::json!({
        "execution_id":"dsp1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
        "duration_ms":11,"schema_enforced":false,"schema_valid":true,"queue_ms":50,"worker_id":"w1"
    });
    let mut text = serde_json::to_string(&row).expect("serialize row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag", "--json", "--strict"]);
    assert!(
        out.status.success(),
        "expected strict pass; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    assert_eq!(v.get("severity").and_then(Value::as_str), Some("ok"));
}

#[test]
fn scheduler_json_strict_reports_severity() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let mut text = String::new();
    for i in 1..=4u64 {
        let row = serde_json::json!({
            "execution_id":format!("sch{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":2500 + i * 10,"worker_id":"w1"
        });
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["scheduler", "--json", "--strict", "--window", "4"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict scheduler failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("scheduler json");
    assert_eq!(
        v.get("scheduler_window_requested").and_then(Value::as_u64),
        Some(4)
    );
    assert_ne!(v.get("severity").and_then(Value::as_str), Some("ok"));
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

#[test]
fn broker_benchmark_strict_fails_when_backend_samples_are_insufficient() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row = serde_json::json!({
        "execution_id":"bs1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
        "duration_ms":1200,"schema_enforced":false,"schema_valid":true,"effective_input_tokens":100,"output_tokens":20
    });
    let mut text = serde_json::to_string(&row).expect("serialize row");
    text.push('\n');
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
        "--strict",
        "--min-runs",
        "1",
        "--json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("broker benchmark json");
    assert_eq!(payload.get("strict").and_then(Value::as_bool), Some(true));
    let fixture = load_fixture_json("broker_benchmark_json_contract.json");
    let violation_keys = fixture_keys(&fixture, "violation_item_keys");
    let violations = payload
        .get("violations")
        .and_then(Value::as_array)
        .expect("violations array");
    for item in violations {
        assert_has_keys(item, &violation_keys, "broker.benchmark.violations[*]");
    }
    assert!(
        violations.iter().filter_map(Value::as_object).any(|o| {
            o.get("backend").and_then(Value::as_str) == Some("ollama")
                && o.get("message")
                    .and_then(Value::as_str)
                    .map(|s| s.contains("below min_runs"))
                    .unwrap_or(false)
        }),
        "violations={violations:?}"
    );
}

#[test]
fn broker_benchmark_strict_critical_allows_warn_only_violations() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"bc1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1000,"schema_enforced":false,"schema_valid":true
        }),
        serde_json::json!({
            "execution_id":"bc2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"ollama","backend_selected":"ollama","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1100,"schema_enforced":false,"schema_valid":true
        }),
    ];
    let mut text = String::new();
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
        "--strict",
        "--min-runs",
        "2",
        "--severity",
        "critical",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "expected success with warn-only violations; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
}

#[test]
fn broker_benchmark_strict_warn_fails_on_warn_violations() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"bw1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":900,"schema_enforced":false,"schema_valid":true
        }),
        serde_json::json!({
            "execution_id":"bw2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"ollama","backend_selected":"ollama","capture_provider":"native","execution_mode":"lean",
            "duration_ms":950,"schema_enforced":false,"schema_valid":true
        }),
    ];
    let mut text = String::new();
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
        "--strict",
        "--min-runs",
        "2",
        "--severity",
        "warn",
        "--json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected warn-threshold failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("broker benchmark json");
    assert_eq!(
        payload.get("severity").and_then(Value::as_str),
        Some("warn")
    );
}

#[test]
fn diag_json_strict_fails_on_retry_recovery_degradation() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let mut text = String::new();
    let rows = vec![
        serde_json::json!({
            "execution_id":"rr1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":11,"schema_enforced":false,"schema_valid":true,"queue_ms":50,"worker_id":"w1",
            "retry_attempt":1,"timed_out":true
        }),
        serde_json::json!({
            "execution_id":"rr2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":11,"schema_enforced":false,"schema_valid":true,"queue_ms":50,"worker_id":"w1",
            "retry_attempt":2,"timed_out":true
        }),
        serde_json::json!({
            "execution_id":"rr3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":11,"schema_enforced":false,"schema_valid":true,"queue_ms":50,"worker_id":"w1",
            "retry_attempt":2,"timed_out":true
        }),
        serde_json::json!({
            "execution_id":"rr4","timestamp":"2026-01-01T00:00:03Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":11,"schema_enforced":false,"schema_valid":true,"queue_ms":50,"worker_id":"w1",
            "retry_attempt":2,"timed_out":true
        }),
        serde_json::json!({
            "execution_id":"rr5","timestamp":"2026-01-01T00:00:04Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":11,"schema_enforced":false,"schema_valid":true,"queue_ms":50,"worker_id":"w1",
            "retry_attempt":2,"timed_out":false
        }),
    ];
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag", "--json", "--strict", "--window", "5"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    let reasons = v
        .get("severity_reasons")
        .and_then(Value::as_array)
        .expect("severity reasons array");
    let joined = reasons
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<&str>>()
        .join(",");
    assert!(
        joined.contains("retry_recovery_low"),
        "expected retry_recovery_low reason, got: {joined}"
    );
}

#[test]
fn scheduler_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let mut text = String::new();
    for i in 1..=2u64 {
        let row = serde_json::json!({
            "execution_id":format!("schfx{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":i * 100,"worker_id":"w1"
        });
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["scheduler", "--json", "--window", "2"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("scheduler json");
    let fixture = load_fixture_json("scheduler_json_contract.json");

    assert_fixture_contract(
        &payload,
        &fixture,
        "top_level_keys",
        &[
            ("scheduler", "scheduler_keys", "scheduler.scheduler"),
            ("retry", "retry_keys", "scheduler.retry"),
        ],
    );
}

#[test]
fn optimize_json_includes_retry_health_metrics() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"o1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1000,"schema_enforced":false,"schema_valid":true,
            "task_id":"task_001","retry_attempt":1,"timed_out":true
        }),
        serde_json::json!({
            "execution_id":"o2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":800,"schema_enforced":false,"schema_valid":true,
            "task_id":"task_001","retry_attempt":2,"timed_out":false
        }),
        serde_json::json!({
            "execution_id":"o3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":700,"schema_enforced":false,"schema_valid":true,
            "task_id":"task_002","retry_attempt":1,"timed_out":true
        }),
        serde_json::json!({
            "execution_id":"o4","timestamp":"2026-01-01T00:00:03Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":650,"schema_enforced":false,"schema_valid":true,
            "task_id":"task_002","retry_attempt":2,"timed_out":true
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["optimize", "10", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("optimize json");
    let scoreboard = payload.get("scoreboard").expect("scoreboard");
    let retry = scoreboard.get("retry_health").expect("retry_health");
    assert_eq!(
        retry.get("rows_after_retry").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        retry
            .get("rows_after_retry_success")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        retry.get("tasks_with_timeout").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        retry.get("tasks_recovered").and_then(Value::as_u64),
        Some(1)
    );
    assert!(retry.get("attempt_histogram").is_some(), "retry={retry}");
}

#[test]
fn optimize_recommendations_include_retry_actions_when_recovery_is_low() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"or1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1200,"schema_enforced":false,"schema_valid":true,
            "task_id":"task_101","retry_attempt":1,"timed_out":true
        }),
        serde_json::json!({
            "execution_id":"or2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1100,"schema_enforced":false,"schema_valid":true,
            "task_id":"task_101","retry_attempt":2,"timed_out":true
        }),
        serde_json::json!({
            "execution_id":"or3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1050,"schema_enforced":false,"schema_valid":true,
            "task_id":"task_102","retry_attempt":1,"timed_out":true
        }),
        serde_json::json!({
            "execution_id":"or4","timestamp":"2026-01-01T00:00:03Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1000,"schema_enforced":false,"schema_valid":true,
            "task_id":"task_102","retry_attempt":2,"timed_out":true
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["optimize", "10", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("optimize json");
    let recs = payload
        .get("recommendations")
        .and_then(Value::as_array)
        .expect("recommendations array");
    let joined = recs
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<&str>>()
        .join("\n");
    assert!(
        joined.contains("Retry recovery is low"),
        "expected retry recovery recommendation, got:\n{joined}"
    );
}

#[test]
fn optimize_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row = serde_json::json!({
        "execution_id":"ofx1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
        "duration_ms":1000,"schema_enforced":false,"schema_valid":true,
        "retry_attempt":2,"timed_out":false
    });
    let mut text = serde_json::to_string(&row).expect("serialize row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["optimize", "10", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("optimize json");
    let fixture = load_fixture_json("optimize_json_contract.json");

    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "optimize");
    let sb_keys = fixture_keys(&fixture, "scoreboard_keys");
    assert_has_keys(
        payload.get("scoreboard").expect("scoreboard"),
        &sb_keys,
        "optimize.scoreboard",
    );
    let retry_keys = fixture_keys(&fixture, "retry_health_keys");
    assert_has_keys(
        payload
            .get("scoreboard")
            .and_then(|v| v.get("retry_health"))
            .expect("retry_health"),
        &retry_keys,
        "optimize.scoreboard.retry_health",
    );
}

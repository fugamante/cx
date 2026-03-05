mod common;

use common::*;
use serde_json::Value;

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
fn judge_convergence_falls_back_on_invalid_model_output() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
prompt="$(cat)"
if printf '%s' "$prompt" | grep -q "Select the best candidate index"; then
  printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"not-json"}}'
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
        "cxo echo judge-fallback",
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
        Some("score_fallback"),
        "unexpected converge votes: {votes:?}"
    );
}

#[test]
fn diag_json_strict_fails_on_retry_recovery_degradation() {
    let repo = TempRepo::new("cxrs-it");
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
    write_runs_log_rows(&repo, &rows);

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
fn optimize_json_includes_retry_health_metrics() {
    let repo = TempRepo::new("cxrs-it");
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
    write_runs_log_rows(&repo, &rows);

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
    write_runs_log_rows(&repo, &rows);

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

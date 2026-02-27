mod common;

use common::{TempRepo, read_json, stderr_str, stdout_str};
use serde_json::Value;
use std::fs;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn parse_labeled_u64(s: &str, label: &str) -> Option<u64> {
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix(label) {
            return rest.trim().parse::<u64>().ok();
        }
    }
    None
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
        elapsed_ms >= 1800 && elapsed_ms <= 3800,
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
        "duration_ms":20,"schema_enforced":true,"schema_valid":true,"task_id":"task_001"
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

    let validate = repo.run(&["logs", "validate"]);
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
    assert_eq!(required, 26, "unexpected strict contract field count");
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

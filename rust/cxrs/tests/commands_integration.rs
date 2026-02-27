mod common;

use common::{TempRepo, read_json, stderr_str, stdout_str};
use serde_json::Value;
use std::fs;
use std::thread::sleep;
use std::time::Duration;

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

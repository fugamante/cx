mod common;

use common::*;
use serde_json::Value;

#[test]
fn diag_reports_scheduler_distribution_fields() {
    let repo = TempRepo::new("cxrs-it");
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
    write_runs_log_rows(&repo, &rows);

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
    assert!(stdout.contains("critical_summary_rows:"), "{stdout}");
    assert!(stdout.contains("critical_errors_total:"), "{stdout}");
}

#[test]
fn diag_json_reports_scheduler_object() {
    let repo = TempRepo::new("cxrs-it");
    let row = serde_json::json!({
        "execution_id":"dj1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
        "duration_ms":10,"schema_enforced":false,"schema_valid":true,"queue_ms":500,"worker_id":"w1"
    });
    write_runs_log_row(&repo, &row);

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
    let critical = v.get("critical").expect("critical");
    assert_eq!(
        critical.get("summary_rows").and_then(Value::as_u64),
        Some(0),
        "unexpected critical object: {critical}"
    );
}

#[test]
fn diag_json_window_scopes_scheduler_rows() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=3u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("dw{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":i * 100,"worker_id":"w1"
        }));
    }
    write_runs_log_rows(&repo, &rows);

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
fn diag_json_reports_run_all_critical_telemetry() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"dc1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":120,"schema_enforced":false,"schema_valid":true,
            "run_all_mode":"mixed","halt_on_critical":true,
            "run_all_scheduled":3,"run_all_complete":1,"run_all_failed":1,"run_all_critical_errors":1
        }),
        serde_json::json!({
            "execution_id":"dc2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["diag", "--json", "--strict", "--window", "5"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict warning on critical halt telemetry; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    let critical = v.get("critical").expect("critical");
    assert_eq!(
        critical.get("summary_rows").and_then(Value::as_u64),
        Some(1),
        "unexpected critical object: {critical}"
    );
    assert_eq!(
        critical.get("halted_rows").and_then(Value::as_u64),
        Some(1),
        "unexpected critical object: {critical}"
    );
    let reasons = v
        .get("severity_reasons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| x.as_str().map(ToOwned::to_owned))
        .collect::<Vec<String>>()
        .join(",");
    assert!(
        reasons.contains("critical_halts_detected"),
        "expected critical_halts_detected reason, got: {reasons}"
    );
}

#[test]
fn scheduler_json_strict_reports_severity() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=4u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("sch{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":2500 + i * 10,"worker_id":"w1"
        }));
    }
    write_runs_log_rows(&repo, &rows);

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
fn scheduler_json_strict_flags_critical_halts() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"schc1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":120,"schema_enforced":false,"schema_valid":true,
            "run_all_mode":"mixed","halt_on_critical":true,
            "run_all_scheduled":3,"run_all_complete":1,"run_all_failed":1,"run_all_critical_errors":1
        }),
        serde_json::json!({
            "execution_id":"schc2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,"queue_ms":100,"worker_id":"w1"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["scheduler", "--json", "--strict", "--window", "5"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict scheduler failure on critical halt; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("scheduler json");
    let reasons = v
        .get("severity_reasons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| x.as_str().map(ToOwned::to_owned))
        .collect::<Vec<String>>()
        .join(",");
    assert!(
        reasons.contains("critical_halts_detected"),
        "expected critical_halts_detected reason, got: {reasons}"
    );
}

#[test]
fn scheduler_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=2u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("schfx{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":i * 100,"worker_id":"w1"
        }));
    }
    write_runs_log_rows(&repo, &rows);

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
            ("critical", "critical_keys", "scheduler.critical"),
        ],
    );
}

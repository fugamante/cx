mod common;

use common::*;
use serde_json::Value;
use std::fs;

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

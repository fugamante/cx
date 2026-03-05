mod common;

use common::*;
use serde_json::Value;
use std::fs;

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

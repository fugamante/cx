mod common;

use common::*;
use serde_json::Value;
use std::fs;

fn assert_actions_for_command(repo: &TempRepo, cmd: &[&str], rows: &[Value], ctx: &str) {
    write_runs_log_rows(repo, rows);
    let out = repo.run(cmd);
    assert!(out.status.success(), "{ctx}: stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("actions json");
    assert_actions_contract(&payload);
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
    let critical = v.get("critical_telemetry").expect("critical_telemetry");
    assert!(critical.get("summary_rows").is_some());
    assert!(critical.get("halted_rows").is_some());
    assert!(critical.get("critical_errors_total").is_some());
    let http_modes = v
        .get("http_mode_stats")
        .and_then(Value::as_array)
        .expect("http_mode_stats");
    assert!(
        http_modes.is_empty()
            || http_modes.iter().all(|m| {
                m.get("format").is_some()
                    && m.get("parser_mode").is_some()
                    && m.get("runs").and_then(Value::as_u64).is_some()
                    && m.get("success_rate").is_some()
            }),
        "invalid http_mode_stats shape: {v}"
    );
}

#[test]
fn telemetry_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"tf1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,
            "provider_transport":"http","http_provider_format":"text","http_parser_mode":"envelope"
        }),
        serde_json::json!({
            "execution_id":"tf2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":12,"schema_enforced":false,"schema_valid":true
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["telemetry", "10", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("telemetry json");
    let fixture = load_fixture_json("telemetry_json_contract.json");

    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "telemetry");
    let drift_keys = fixture_keys(&fixture, "contract_drift_keys");
    assert_has_keys(
        payload.get("contract_drift").expect("contract_drift"),
        &drift_keys,
        "telemetry.contract_drift",
    );
    let retry_keys = fixture_keys(&fixture, "retry_keys");
    assert_has_keys(
        payload.get("retry_telemetry").expect("retry_telemetry"),
        &retry_keys,
        "telemetry.retry_telemetry",
    );
    let critical_keys = fixture_keys(&fixture, "critical_keys");
    assert_has_keys(
        payload
            .get("critical_telemetry")
            .expect("critical_telemetry"),
        &critical_keys,
        "telemetry.critical_telemetry",
    );
    let item_keys = fixture_keys(&fixture, "http_mode_item_keys");
    let modes = payload
        .get("http_mode_stats")
        .and_then(Value::as_array)
        .expect("http_mode_stats array");
    for item in modes {
        assert_has_keys(item, &item_keys, "telemetry.http_mode_stats[*]");
    }
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

#[test]
fn telemetry_json_groups_http_mode_stats() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"h1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,
            "provider_transport":"http","http_provider_format":"text","http_parser_mode":"envelope"
        }),
        serde_json::json!({
            "execution_id":"h2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":12,"schema_enforced":false,"schema_valid":false,
            "provider_transport":"http","http_provider_format":"text","http_parser_mode":"envelope"
        }),
        serde_json::json!({
            "execution_id":"h3","timestamp":"2026-01-01T00:00:02Z","command":"cxj","tool":"cxj",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":14,"schema_enforced":false,"schema_valid":true,
            "provider_transport":"http","http_provider_format":"jsonl","http_parser_mode":"jsonl_passthrough"
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["telemetry", "10", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("telemetry json");
    let modes = v
        .get("http_mode_stats")
        .and_then(Value::as_array)
        .expect("http_mode_stats array");
    assert!(!modes.is_empty(), "expected grouped http_mode_stats: {v}");

    let text_mode = modes
        .iter()
        .find(|m| {
            m.get("format").and_then(Value::as_str) == Some("text")
                && m.get("parser_mode").and_then(Value::as_str) == Some("envelope")
        })
        .expect("text/envelope mode");
    assert_eq!(text_mode.get("runs").and_then(Value::as_u64), Some(2));
    assert_eq!(
        text_mode.get("schema_invalid").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        text_mode.get("healthy_runs").and_then(Value::as_u64),
        Some(1)
    );
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
            ("critical", "critical_keys", "diag.critical"),
        ],
    );
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
fn diag_json_actions_match_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=4u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("diact{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":2500 + i,"worker_id":"w1"
        }));
    }
    assert_actions_for_command(
        &repo,
        &["diag", "--json", "--actions", "--window", "4"],
        &rows,
        "diag actions",
    );
}

#[test]
fn scheduler_json_actions_match_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=4u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("schact{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","backend_selected":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":2400 + i,"worker_id":"w1"
        }));
    }
    assert_actions_for_command(
        &repo,
        &["scheduler", "--json", "--actions", "--window", "4"],
        &rows,
        "scheduler actions",
    );
}

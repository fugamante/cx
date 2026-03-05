mod common;

use common::*;
use serde_json::Value;
use std::fs;

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
fn broker_set_accepts_quota_saver_policy() {
    let repo = TempRepo::new("cxrs-it");
    let set = repo.run(&["broker", "set", "--policy", "quota_saver"]);
    assert!(set.status.success(), "stderr={}", stderr_str(&set));
    assert!(stdout_str(&set).contains("quota_saver"));

    let show = repo.run(&["broker", "show", "--json"]);
    assert!(show.status.success(), "stderr={}", stderr_str(&show));
    let payload: Value = serde_json::from_str(&stdout_str(&show)).expect("broker show json");
    assert_eq!(
        payload.get("broker_policy").and_then(Value::as_str),
        Some("quota_saver")
    );
}

#[test]
fn broker_benchmark_accepts_warning_severity_alias() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    fs::write(&log, "").expect("write empty runs log");
    let out = repo.run(&[
        "broker",
        "benchmark",
        "--window",
        "10",
        "--severity",
        "warning",
        "--json",
    ]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("broker benchmark json");
    assert_eq!(
        payload.get("severity").and_then(Value::as_str),
        Some("warn")
    );
}

#[test]
fn quota_json_reports_projection_and_top_commands() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let now = chrono::Utc::now().to_rfc3339();
    let rows = vec![
        serde_json::json!({
            "execution_id":"q1","timestamp":now,"command":"cxdiffsum_staged","tool":"cxdiffsum_staged",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":2200,"input_tokens":2000,"cached_input_tokens":500,"effective_input_tokens":1500,"output_tokens":120
        }),
        serde_json::json!({
            "execution_id":"q2","timestamp":chrono::Utc::now().to_rfc3339(),"command":"cxcommitmsg","tool":"cxcommitmsg",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":1800,"input_tokens":900,"cached_input_tokens":100,"effective_input_tokens":800,"output_tokens":80
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["quota", "30", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("quota json");
    assert_eq!(payload.get("window_days").and_then(Value::as_u64), Some(30));
    assert!(payload.get("monthly_effective_projection").is_some());
    assert!(
        payload
            .get("top_commands")
            .and_then(Value::as_array)
            .is_some()
    );
}

#[test]
fn quota_probe_reports_configured_total_and_remaining() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let now = chrono::Utc::now().to_rfc3339();
    let rows = vec![serde_json::json!({
        "execution_id":"qp1","timestamp":now,"command":"cxo","tool":"cxo",
        "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
        "duration_ms":1000,"input_tokens":500,"cached_input_tokens":100,"effective_input_tokens":400,"output_tokens":80
    })];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run_with_env(
        &["quota", "probe", "30", "--json"],
        &[("CX_QUOTA_CODEX_TOTAL_TOKENS", "1000")],
    );
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("quota probe json");
    assert_eq!(
        payload.get("backend").and_then(Value::as_str),
        Some("codex")
    );
    assert_eq!(
        payload.get("quota_source").and_then(Value::as_str),
        Some("env:CX_QUOTA_CODEX_TOTAL_TOKENS")
    );
    assert_eq!(
        payload.get("quota_total_tokens").and_then(Value::as_u64),
        Some(1000)
    );
    assert_eq!(
        payload
            .get("quota_used_tokens_window")
            .and_then(Value::as_u64),
        Some(400)
    );
    assert_eq!(
        payload
            .get("quota_remaining_tokens")
            .and_then(Value::as_u64),
        Some(600)
    );
}

#[test]
fn prompt_stats_json_reports_filter_savings() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let now = chrono::Utc::now().to_rfc3339();
    let rows = vec![
        serde_json::json!({
            "execution_id":"ps1","timestamp":now,"command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":200,"schema_enforced":false,"schema_valid":true,
            "prompt_len_raw":120,"prompt_len_filtered":90,"prompt_filter_applied":true
        }),
        serde_json::json!({
            "execution_id":"ps2","timestamp":chrono::Utc::now().to_rfc3339(),"command":"cxcommitmsg","tool":"cxcommitmsg",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":210,"schema_enforced":true,"schema_valid":true,
            "prompt_len_raw":80,"prompt_len_filtered":80,"prompt_filter_applied":false
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["prompt-stats", "50", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("prompt-stats json");
    assert_eq!(payload.get("window").and_then(Value::as_u64), Some(50));
    assert_eq!(
        payload
            .get("rows_with_prompt_lengths")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        payload
            .get("prompt_filter_applied_runs")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        payload.get("saved_chars_total").and_then(Value::as_u64),
        Some(30)
    );
    assert!(payload.get("by_tool").and_then(Value::as_array).is_some());
}

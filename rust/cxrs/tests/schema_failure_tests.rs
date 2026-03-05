mod common;

use common::*;
use serde_json::Value;
use std::fs;
use std::thread::sleep;
use std::time::Duration;

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
fn http_curl_adapter_json_format_rejects_invalid_content_envelope() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"content":[{"unexpected":"shape"}]}'
"#,
    );
    let out = repo.run_with_env(
        &["next", "echo", "http-json-bad-content"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:9999/infer"),
            ("CX_HTTP_PROVIDER_FORMAT", "json"),
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
        stderr_str(&out).contains("http_json_content_invalid"),
        "stderr={}",
        stderr_str(&out)
    );
}

#[test]
fn http_curl_adapter_json_format_rejects_invalid_json_payload() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{not-json'
"#,
    );
    let out = repo.run_with_env(
        &["next", "echo", "http-json-invalid"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:9999/infer"),
            ("CX_HTTP_PROVIDER_FORMAT", "json"),
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
        stderr_str(&out).contains("http_json_invalid"),
        "stderr={}",
        stderr_str(&out)
    );
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

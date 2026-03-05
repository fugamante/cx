mod common;

use common::*;
use serde_json::Value;

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
    assert!(
        row.get("http_provider_format").is_some()
            && row.get("http_provider_format").is_some_and(Value::is_null),
        "expected http_provider_format=null, row={row}"
    );
    assert!(
        row.get("http_parser_mode").is_some()
            && row.get("http_parser_mode").is_some_and(Value::is_null),
        "expected http_parser_mode=null, row={row}"
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
    assert!(
        row.get("http_provider_format").is_some()
            && row.get("http_provider_format").is_some_and(Value::is_null),
        "expected http_provider_format=null, row={row}"
    );
    assert!(
        row.get("http_parser_mode").is_some()
            && row.get("http_parser_mode").is_some_and(Value::is_null),
        "expected http_parser_mode=null, row={row}"
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
    assert_eq!(
        run_last.get("http_provider_format").and_then(Value::as_str),
        Some("text")
    );
    assert_eq!(
        run_last.get("http_parser_mode").and_then(Value::as_str),
        Some("envelope")
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
    assert_eq!(
        run_last.get("http_provider_format").and_then(Value::as_str),
        Some("text")
    );
    assert_eq!(
        run_last.get("http_parser_mode").and_then(Value::as_str),
        Some("envelope")
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
    assert_eq!(
        run_last.get("http_provider_format").and_then(Value::as_str),
        Some("text")
    );
    assert_eq!(
        run_last.get("http_parser_mode").and_then(Value::as_str),
        Some("envelope")
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

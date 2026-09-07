mod common;

use common::*;
use serde_json::Value;
use std::fs;

fn mode_json(repo: &TempRepo, envs: &[(&str, &str)]) -> Value {
    let out = repo.run_with_env(&["mode", "--json"], envs);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    serde_json::from_str(&stdout_str(&out)).expect("mode json")
}

#[test]
fn mode_default_text() {
    let repo = TempRepo::new("cxrs-it");
    let payload = mode_json(&repo, &[]);
    assert_eq!(
        payload.get("source").and_then(Value::as_str),
        Some("default")
    );
    assert_eq!(
        payload.get("selected").and_then(Value::as_str),
        Some("text")
    );
}

#[test]
fn mode_auto_tty() {
    let repo = TempRepo::new("cxrs-it");
    let payload = mode_json(&repo, &[("CX_JSON_AUTO", "1")]);
    assert_eq!(payload.get("source").and_then(Value::as_str), Some("auto"));
    assert_eq!(
        payload.get("selected").and_then(Value::as_str),
        Some("json")
    );
}

#[test]
fn env_over_auto() {
    let repo = TempRepo::new("cxrs-it");
    let payload = mode_json(&repo, &[("CX_JSON_AUTO", "1"), ("CX_JSON_DEFAULT", "0")]);
    assert_eq!(payload.get("source").and_then(Value::as_str), Some("env"));
    assert_eq!(
        payload.get("selected").and_then(Value::as_str),
        Some("text")
    );
}

#[test]
fn state_env_nil() {
    let repo = TempRepo::new("cxrs-it");
    let state = repo.state_file();
    fs::create_dir_all(state.parent().expect("state parent")).expect("mkdir state parent");
    fs::write(&state, r#"{"preferences":{"default_json_output":true}}"#).expect("write state");

    let payload = mode_json(&repo, &[]);
    assert_eq!(payload.get("source").and_then(Value::as_str), Some("state"));
    assert_eq!(
        payload.get("selected").and_then(Value::as_str),
        Some("json")
    );
}

#[test]
fn cli_over_env() {
    let repo = TempRepo::new("cxrs-it");
    let out = repo.run_with_env(
        &["mode", "--json", "--cli", "text"],
        &[("CX_JSON_DEFAULT", "1"), ("CX_JSON_AUTO", "1")],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("mode json");
    assert_eq!(payload.get("source").and_then(Value::as_str), Some("cli"));
    assert_eq!(
        payload.get("selected").and_then(Value::as_str),
        Some("text")
    );
}

#[test]
fn auto_diag_json() {
    let repo = TempRepo::new("cxrs-it");
    let out = repo.run_with_env(&["diag", "--window", "5"], &[("CX_JSON_AUTO", "1")]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("diag.v1")
    );
    assert!(payload.get("routing_trace").is_some(), "payload={payload}");
}

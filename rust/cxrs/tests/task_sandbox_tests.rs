mod common;

use common::*;
use serde_json::Value;

#[test]
fn sandbox_show_mutate() {
    let repo = TempRepo::new("cxrs-it");

    let show = repo.run(&["task", "sandbox", "show", "--json"]);
    assert!(show.status.success(), "stderr={}", stderr_str(&show));
    let initial: Value = serde_json::from_str(&stdout_str(&show)).expect("valid json");
    assert_eq!(
        initial.get("contract_version").and_then(Value::as_str),
        Some("task-sandbox.v1")
    );
    assert_eq!(initial.get("enabled").and_then(Value::as_bool), Some(false));
    assert_eq!(initial.get("image"), Some(&Value::Null));

    let set_image = repo.run(&["task", "sandbox", "set-image", "xshelf-compat:local"]);
    assert!(
        set_image.status.success(),
        "stderr={}",
        stderr_str(&set_image)
    );

    let enable = repo.run(&["task", "sandbox", "enable"]);
    assert!(enable.status.success(), "stderr={}", stderr_str(&enable));

    let show_enabled = repo.run(&["task", "sandbox", "show", "--json"]);
    assert!(
        show_enabled.status.success(),
        "stderr={}",
        stderr_str(&show_enabled)
    );
    let enabled: Value = serde_json::from_str(&stdout_str(&show_enabled)).expect("valid json");
    assert_eq!(enabled.get("enabled").and_then(Value::as_bool), Some(true));
    assert_eq!(
        enabled.get("image").and_then(Value::as_str),
        Some("xshelf-compat:local")
    );

    let disable = repo.run(&["task", "sandbox", "disable"]);
    assert!(disable.status.success(), "stderr={}", stderr_str(&disable));

    let clear = repo.run(&["task", "sandbox", "clear-image"]);
    assert!(clear.status.success(), "stderr={}", stderr_str(&clear));

    let state = read_json(&repo.state_file());
    assert_eq!(
        state.pointer("/preferences/task_sandbox/enabled"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        state.pointer("/preferences/task_sandbox/image"),
        Some(&Value::Null)
    );
}

#[test]
fn sandbox_not_ready() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "docker",
        r#"#!/usr/bin/env bash
case "${1:-}" in
  --version)
    printf '%s\n' 'Docker version 99.0.0'
    ;;
  *)
    exit 1
    ;;
esac
"#,
    );

    let check = repo.run(&["task", "sandbox", "check", "--json"]);
    assert!(
        !check.status.success(),
        "stdout={} stderr={}",
        stdout_str(&check),
        stderr_str(&check)
    );
    let out: Value = serde_json::from_str(&stdout_str(&check)).expect("valid json");
    assert_eq!(
        out.get("contract_version").and_then(Value::as_str),
        Some("task-sandbox-readiness.v1")
    );
    assert_eq!(out.get("ready").and_then(Value::as_bool), Some(false));
    let issues = out.get("issues").and_then(Value::as_array).expect("issues");
    assert!(
        issues
            .iter()
            .any(|v| v.as_str() == Some("sandbox_disabled"))
    );
    assert!(issues.iter().any(|v| v.as_str() == Some("image_unset")));
    assert_eq!(
        out.get("recommended_action").and_then(Value::as_str),
        Some("run `xshelf task sandbox enable`")
    );
}

#[test]
fn sandbox_check_ready() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_cx_wrapper();
    repo.write_mock(
        "docker",
        r#"#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  --version)
    printf '%s\n' 'Docker version 99.0.0'
    ;;
  image)
    test "${2:-}" = "inspect"
    test "${3:-}" = "xshelf-compat:local"
    ;;
  run)
    exit 0
    ;;
  *)
    exit 1
    ;;
esac
"#,
    );

    let set_image = repo.run(&["task", "sandbox", "set-image", "xshelf-compat:local"]);
    assert!(
        set_image.status.success(),
        "stderr={}",
        stderr_str(&set_image)
    );
    let enable = repo.run(&["task", "sandbox", "enable"]);
    assert!(enable.status.success(), "stderr={}", stderr_str(&enable));

    let check = repo.run(&["task", "sandbox", "check", "--json"]);
    assert!(
        check.status.success(),
        "stdout={} stderr={}",
        stdout_str(&check),
        stderr_str(&check)
    );
    let out: Value = serde_json::from_str(&stdout_str(&check)).expect("valid json");
    assert_eq!(
        out.get("contract_version").and_then(Value::as_str),
        Some("task-sandbox-readiness.v1")
    );
    assert_eq!(out.get("ready").and_then(Value::as_bool), Some(true));
    assert_eq!(
        out.get("docker_available").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        out.get("image_available").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        out.get("repo_mount_writable").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        out.get("entrypoint_available").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        out.get("issues").and_then(Value::as_array).unwrap().len(),
        0
    );
}

#[test]
fn task_container_lane() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_cx_wrapper();
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#);
    repo.write_mock(
        "docker",
        r#"#!/usr/bin/env bash
set -euo pipefail
script="${!#}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    -e)
      shift
      export "$1"
      ;;
  esac
  shift || true
done
bash -c "$script"
"#,
    );

    let set_image = repo.run(&["task", "sandbox", "set-image", "xshelf-compat:local"]);
    assert!(
        set_image.status.success(),
        "stderr={}",
        stderr_str(&set_image)
    );
    let enable = repo.run(&["task", "sandbox", "enable"]);
    assert!(enable.status.success(), "stderr={}", stderr_str(&enable));

    let add = repo.run(&[
        "task",
        "add",
        "cxo echo sandboxed-run",
        "--role",
        "implementer",
        "--backend",
        "primary",
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

    let show = repo.run(&["task", "show", &id]);
    assert!(show.status.success(), "stderr={}", stderr_str(&show));
    let out: Value = serde_json::from_str(&stdout_str(&show)).expect("valid json");
    let latest = out.get("latest_run").expect("latest_run");
    assert_eq!(
        latest.get("execution_lane").and_then(Value::as_str),
        Some("container")
    );
    assert_eq!(
        latest.get("execution_lane_detail").and_then(Value::as_str),
        Some("docker:xshelf-compat:local")
    );

    let runs = parse_jsonl(&repo.runs_log());
    let task_row = runs
        .iter()
        .find(|row| row.get("task_id").and_then(Value::as_str) == Some(id.as_str()))
        .expect("task run row");
    assert_eq!(
        task_row.get("execution_lane").and_then(Value::as_str),
        Some("container")
    );
}

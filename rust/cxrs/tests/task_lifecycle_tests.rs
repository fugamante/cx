mod common;

use common::*;
use serde_json::Value;

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
fn show_latest_run() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    let add = repo.run(&[
        "task",
        "add",
        "cxo echo show-summary",
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
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let out: Value = serde_json::from_str(&stdout_str(&show)).expect("valid json");
    let fixture = load_fixture_json("task_show_contract.json");
    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&out, &top_keys, "task_show.top");
    let latest = out.get("latest_run").expect("latest_run field");
    assert!(latest.is_object(), "latest_run should be object: {latest}");
    let latest_keys = fixture_keys(&fixture, "latest_run_keys");
    assert_has_keys(latest, &latest_keys, "task_show.latest_run");
    assert!(latest.get("execution_id").is_some(), "missing execution_id");
    assert!(
        latest.get("duration_ms").is_some(),
        "missing duration_ms: {latest}"
    );
    let readiness = out.get("run_readiness").expect("run_readiness field");
    let readiness_keys = fixture_keys(&fixture, "run_readiness_keys");
    assert_has_keys(readiness, &readiness_keys, "task_show.run_readiness");
    assert_eq!(
        readiness.get("runnable_now").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        readiness.get("recommended_reason").and_then(Value::as_str),
        Some("inspect_non_pending_task")
    );
}

#[test]
fn run_json_contract() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock_primary(r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    let add = repo.run(&[
        "task",
        "add",
        "cxo echo run-json",
        "--role",
        "implementer",
        "--backend",
        "primary",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let id = stdout_str(&add).trim().to_string();

    let run = repo.run(&["task", "run", &id, "--json"]);
    assert!(
        run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&run),
        stderr_str(&run)
    );
    let out: Value = serde_json::from_str(&stdout_str(&run)).expect("valid json");
    let fixture = load_fixture_json("task_run_contract.json");
    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&out, &top_keys, "task_run.top");
    assert_eq!(
        out.get("contract_version").and_then(Value::as_str),
        Some("task-run.v1")
    );
    assert_eq!(
        out.get("task_id").and_then(Value::as_str),
        Some(id.as_str())
    );
    assert_eq!(out.get("status").and_then(Value::as_str), Some("complete"));
    let preflight = out.get("preflight").expect("preflight object");
    let preflight_keys = fixture_keys(&fixture, "preflight_keys");
    assert_has_keys(preflight, &preflight_keys, "task_run.preflight");
}

#[test]
fn show_list_alias() {
    let repo = TempRepo::new("cxrs-it");
    let add = repo.run(&["task", "add", "Alias list", "--role", "implementer"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let id = stdout_str(&add).trim().to_string();

    let show_list = repo.run(&["task", "show", "list"]);
    assert!(
        show_list.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show_list),
        stderr_str(&show_list)
    );
    let text = stdout_str(&show_list);
    assert!(text.contains("id | role | status | parent_id | objective"));
    assert!(text.contains(&id), "{text}");
}

#[test]
fn show_ready_readiness() {
    let repo = TempRepo::new("cxrs-it");
    let add = repo.run(&["task", "add", "Ready task", "--role", "implementer"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let id = stdout_str(&add).trim().to_string();

    let show = repo.run(&["task", "show", &id]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let out: Value = serde_json::from_str(&stdout_str(&show)).expect("valid json");
    let fixture = load_fixture_json("task_show_contract.json");
    let readiness = out.get("run_readiness").expect("run_readiness field");
    let readiness_keys = fixture_keys(&fixture, "run_readiness_keys");
    assert_has_keys(readiness, &readiness_keys, "task_show.run_readiness");
    assert_eq!(
        readiness.get("runnable_now").and_then(Value::as_bool),
        Some(true)
    );
    let run_cmd = format!("xshelf task run {id}");
    assert_eq!(
        readiness.get("recommended_command").and_then(Value::as_str),
        Some(run_cmd.as_str())
    );
}

#[test]
fn show_blocked_readiness() {
    let repo = TempRepo::new("cxrs-it");

    let parent = repo.run(&["task", "add", "Parent task", "--role", "implementer"]);
    assert!(parent.status.success(), "stderr={}", stderr_str(&parent));
    let parent_id = stdout_str(&parent).trim().to_string();

    let child = repo.run(&[
        "task",
        "add",
        "Child task",
        "--role",
        "implementer",
        "--depends-on",
        &parent_id,
    ]);
    assert!(child.status.success(), "stderr={}", stderr_str(&child));
    let child_id = stdout_str(&child).trim().to_string();

    let show = repo.run(&["task", "show", &child_id]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let out: Value = serde_json::from_str(&stdout_str(&show)).expect("valid json");
    let fixture = load_fixture_json("task_show_contract.json");
    let readiness = out.get("run_readiness").expect("run_readiness field");
    let readiness_keys = fixture_keys(&fixture, "run_readiness_keys");
    assert_has_keys(readiness, &readiness_keys, "task_show.run_readiness");
    assert_eq!(
        readiness.get("runnable_now").and_then(Value::as_bool),
        Some(false)
    );
    let blocked = format!("unresolved dependencies: {parent_id}");
    assert_eq!(
        readiness.get("blocked_reason").and_then(Value::as_str),
        Some(blocked.as_str())
    );
    assert_eq!(
        readiness.get("recommended_command").and_then(Value::as_str),
        Some("xshelf task check --json")
    );
}

#[test]
fn list_json_readiness() {
    let repo = TempRepo::new("cxrs-it");

    let parent = repo.run(&["task", "add", "Parent list task", "--role", "implementer"]);
    assert!(parent.status.success(), "stderr={}", stderr_str(&parent));
    let parent_id = stdout_str(&parent).trim().to_string();

    let child = repo.run(&[
        "task",
        "add",
        "Child list task",
        "--role",
        "implementer",
        "--depends-on",
        &parent_id,
    ]);
    assert!(child.status.success(), "stderr={}", stderr_str(&child));
    let child_id = stdout_str(&child).trim().to_string();

    let out = repo.run(&["task", "list", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("valid json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("task-list.v1")
    );
    let fixture = load_fixture_json("task_list_contract.json");
    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "task_list.top");
    let list_readiness = payload
        .get("list_readiness")
        .expect("list_readiness object");
    let list_keys = fixture_keys(&fixture, "list_readiness_keys");
    assert_has_keys(list_readiness, &list_keys, "task_list.list_readiness");
    assert_eq!(
        list_readiness.get("selected_count").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        list_readiness
            .get("runnable_now_count")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        list_readiness
            .get("blocked_now_count")
            .and_then(Value::as_u64),
        Some(1)
    );
    let next_wave = list_readiness.get("next_wave").expect("next_wave");
    let next_wave_keys = fixture_keys(&fixture, "list_readiness_next_wave_keys");
    assert_has_keys(
        next_wave,
        &next_wave_keys,
        "task_list.list_readiness.next_wave",
    );
    assert_eq!(next_wave.get("index").and_then(Value::as_u64), Some(1));
    assert_eq!(
        next_wave.get("mode").and_then(Value::as_str),
        Some("sequential")
    );
    assert_eq!(next_wave.get("size").and_then(Value::as_u64), Some(1));
    let tasks = payload
        .get("tasks")
        .and_then(Value::as_array)
        .expect("tasks array");
    let task_keys = fixture_keys(&fixture, "task_keys");
    let child = tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(child_id.as_str()))
        .expect("child task exists");
    assert_has_keys(child, &task_keys, "task_list.tasks[]");
    let readiness = child.get("run_readiness").expect("run_readiness");
    let readiness_keys = fixture_keys(&fixture, "run_readiness_keys");
    assert_has_keys(
        readiness,
        &readiness_keys,
        "task_list.tasks[].run_readiness",
    );
    assert_eq!(
        readiness.get("runnable_now").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        readiness.get("recommended_command").and_then(Value::as_str),
        Some("xshelf task check --json")
    );
}

#[test]
fn list_text_summary() {
    let repo = TempRepo::new("cxrs-it");

    let parent = repo.run(&["task", "add", "Parent text task", "--role", "implementer"]);
    assert!(parent.status.success(), "stderr={}", stderr_str(&parent));
    let parent_id = stdout_str(&parent).trim().to_string();

    let child = repo.run(&[
        "task",
        "add",
        "Child text task",
        "--role",
        "implementer",
        "--depends-on",
        &parent_id,
    ]);
    assert!(child.status.success(), "stderr={}", stderr_str(&child));

    let out = repo.run(&["task", "list"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let text = stdout_str(&out);
    assert!(
        text.contains(
            "list_readiness: selected=2 runnable_now=1 blocked_now=1 inspect_only=0 waves=2 blocked=0"
        ),
        "{text}"
    );
    assert!(
        text.contains("list_readiness_next_wave: index=1 mode=sequential size=1"),
        "{text}"
    );
    assert!(
        text.contains("id | role | status | parent_id | objective"),
        "{text}"
    );
}

#[test]
fn list_complete_summary() {
    let repo = TempRepo::new("cxrs-it");

    let add = repo.run(&["task", "add", "Complete list task", "--role", "implementer"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let id = stdout_str(&add).trim().to_string();

    let done = repo.run(&["task", "complete", &id]);
    assert!(done.status.success(), "stderr={}", stderr_str(&done));

    let out = repo.run(&["task", "list", "--status", "complete", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("valid json");
    let fixture = load_fixture_json("task_list_contract.json");
    let list_readiness = payload
        .get("list_readiness")
        .expect("list_readiness object");
    let list_keys = fixture_keys(&fixture, "list_readiness_keys");
    assert_has_keys(list_readiness, &list_keys, "task_list.list_readiness");
    assert_eq!(
        list_readiness.get("selected_count").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        list_readiness
            .get("runnable_now_count")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        list_readiness
            .get("blocked_now_count")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        list_readiness
            .get("inspect_only_count")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        list_readiness.get("wave_count").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        list_readiness.get("blocked_count").and_then(Value::as_u64),
        Some(0)
    );
    assert!(list_readiness.get("next_wave").unwrap().is_null());
}

#[test]
fn run_preflight_blocked() {
    let repo = TempRepo::new("cxrs-it");

    let parent = repo.run(&["task", "add", "Parent run task", "--role", "implementer"]);
    assert!(parent.status.success(), "stderr={}", stderr_str(&parent));
    let parent_id = stdout_str(&parent).trim().to_string();

    let child = repo.run(&[
        "task",
        "add",
        "Child run task",
        "--role",
        "implementer",
        "--depends-on",
        &parent_id,
    ]);
    assert!(child.status.success(), "stderr={}", stderr_str(&child));
    let child_id = stdout_str(&child).trim().to_string();

    let run = repo.run(&["task", "run", &child_id]);
    let text = stdout_str(&run);
    assert!(
        text.contains("task_run_preflight: runnable_now=false wave_index=2 wave_mode=sequential"),
        "{text}"
    );
    let reason = format!("task_run_preflight_reason: unresolved dependencies: {parent_id}");
    assert!(text.contains(&reason), "{text}");
    assert!(
        text.contains("task_run_preflight_recommended: xshelf task check --json"),
        "{text}"
    );
}

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

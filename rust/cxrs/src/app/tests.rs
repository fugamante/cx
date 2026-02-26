use crate::capture::{BudgetConfig, choose_clip_mode, clip_text_with_config, should_use_rtk};
use crate::logs::append_jsonl;
use crate::runlog::log_schema_failure;
use serde_json::Value;
use serde_json::json;
use std::env;
use std::fs;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn smart_mode_prefers_tail_on_error_keywords() {
    assert_eq!(choose_clip_mode("all good", "smart"), "head");
    assert_eq!(choose_clip_mode("WARNING: issue", "smart"), "tail");
    assert_eq!(choose_clip_mode("failed to run", "smart"), "tail");
}

#[test]
fn clip_text_respects_line_and_char_budget() {
    let cfg = BudgetConfig {
        budget_chars: 12,
        budget_lines: 2,
        clip_mode: "head".to_string(),
        clip_footer: false,
    };
    let (out, stats) = clip_text_with_config("line1\nline2\nline3\n", &cfg);
    assert!(out.starts_with("line1\nline2"));
    assert_eq!(stats.budget_chars, Some(12));
    assert_eq!(stats.budget_lines, Some(2));
    assert_eq!(stats.clipped, Some(true));
}

#[test]
fn jsonl_append_integrity() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("runs.jsonl");
    append_jsonl(&file, &json!({"a": 1})).expect("append 1");
    append_jsonl(&file, &json!({"b": 2})).expect("append 2");
    let content = fs::read_to_string(&file).expect("read");
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    let v1: Value = serde_json::from_str(lines[0]).expect("line1 json");
    let v2: Value = serde_json::from_str(lines[1]).expect("line2 json");
    assert_eq!(v1.get("a").and_then(Value::as_i64), Some(1));
    assert_eq!(v2.get("b").and_then(Value::as_i64), Some(2));
}

#[test]
fn rtk_unavailable_path_uses_native() {
    let cmd = vec!["git".to_string(), "status".to_string()];
    assert!(!should_use_rtk(&cmd, "auto", true, false));
    assert!(!should_use_rtk(&cmd, "native", true, true));
}

#[test]
fn schema_failure_writes_quarantine_and_logs() {
    let _guard = cwd_lock().lock().expect("lock");
    let dir = tempdir().expect("tempdir");
    let prev = env::current_dir().expect("cwd");
    env::set_current_dir(dir.path()).expect("cd temp");
    let _ = Command::new("git")
        .args(["init"])
        .output()
        .expect("git init");

    let qid = log_schema_failure(
        "cxrs_next",
        "invalid_json",
        "raw",
        "{}",
        "prompt",
        Vec::new(),
    )
    .expect("schema failure log");
    assert!(!qid.is_empty());

    assert_quarantine_and_logs(dir.path(), &qid);

    env::set_current_dir(prev).expect("restore cwd");
}

fn read_last_json_line(path: &std::path::Path, label: &str) -> Value {
    let content = fs::read_to_string(path).expect(label);
    serde_json::from_str(content.lines().last().expect("jsonl line")).expect("valid json line")
}

fn assert_quarantine_and_logs(root: &std::path::Path, qid: &str) {
    let qfile = root
        .join(".codex")
        .join("quarantine")
        .join(format!("{qid}.json"));
    assert!(qfile.exists());

    let sf_log = root
        .join(".codex")
        .join("cxlogs")
        .join("schema_failures.jsonl");
    let last_sf = read_last_json_line(&sf_log, "read schema fail log");
    assert_eq!(
        last_sf.get("quarantine_id").and_then(Value::as_str),
        Some(qid)
    );

    let runs_log = root.join(".codex").join("cxlogs").join("runs.jsonl");
    let last_run = read_last_json_line(&runs_log, "read runs");
    assert_eq!(
        last_run.get("quarantine_id").and_then(Value::as_str),
        Some(qid)
    );
    assert_eq!(
        last_run.get("schema_valid").and_then(Value::as_bool),
        Some(false)
    );
}

mod common;

use common::{
    TempRepo, parse_jsonl, read_json, set_readonly, set_writable, stderr_str, stdout_str,
};
use serde_json::{Value, json};
use std::fs;

fn assert_required_run_fields(v: &Value) {
    for key in [
        "execution_id",
        "backend_used",
        "capture_provider",
        "execution_mode",
        "schema_valid",
        "duration_ms",
    ] {
        assert!(
            v.get(key).is_some(),
            "missing key {key} in run log row: {v}"
        );
    }
}

fn mock_codex_jsonl_agent_text(text: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{{"type":"item.completed","item":{{"type":"agent_message","text":{text:?}}}}}'
printf '%s\n' '{{"type":"turn.completed","usage":{{"input_tokens":64,"cached_input_tokens":8,"output_tokens":12}}}}'
"#
    )
}

#[test]
fn timeout_injection_logs_timeout_metadata_and_required_fields() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
cat >/dev/null
sleep 2
"#,
    );

    let out = repo.run_with_env(&["cxo", "echo", "hello"], &[("CX_CMD_TIMEOUT_SECS", "1")]);
    assert!(
        !out.status.success(),
        "expected timeout failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("timed out"),
        "missing timeout message: {}",
        stderr_str(&out)
    );

    let runs = parse_jsonl(&repo.runs_log());
    assert!(!runs.is_empty(), "expected run logs");
    let last = runs.last().expect("last run");
    assert_required_run_fields(last);
    assert_eq!(last.get("timed_out").and_then(Value::as_bool), Some(true));
    assert_eq!(last.get("timeout_secs").and_then(Value::as_u64), Some(1));
    assert!(
        last.get("command_label")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
    );
}

#[test]
fn timeout_override_llm_precedence_is_logged_end_to_end() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
cat >/dev/null
sleep 2
"#,
    );

    let out = repo.run_with_env(
        &["cxo", "echo", "hello"],
        &[("CX_CMD_TIMEOUT_SECS", "9"), ("CX_TIMEOUT_LLM_SECS", "1")],
    );
    assert!(
        !out.status.success(),
        "expected timeout failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let runs = parse_jsonl(&repo.runs_log());
    let last = runs.last().expect("last run");
    assert_eq!(last.get("timed_out").and_then(Value::as_bool), Some(true));
    assert_eq!(last.get("timeout_secs").and_then(Value::as_u64), Some(1));
    assert!(
        last.get("command_label")
            .and_then(Value::as_str)
            .is_some_and(|s| s.contains("codex"))
    );
}

#[test]
fn timeout_override_git_precedence_is_logged_end_to_end() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock(
        "git",
        r#"#!/usr/bin/env bash
sleep 2
exit 0
"#,
    );
    repo.write_mock("codex", &mock_codex_jsonl_agent_text("ok"));

    let out = repo.run_with_env(
        &["cxo", "git", "status"],
        &[("CX_CMD_TIMEOUT_SECS", "9"), ("CX_TIMEOUT_GIT_SECS", "1")],
    );
    assert!(
        !out.status.success(),
        "expected git timeout; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("timed out after 1s"),
        "expected git timeout override in stderr: {}",
        stderr_str(&out)
    );
}

#[test]
fn timeout_override_shell_precedence_applies_to_clipboard_backend() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock("codex", &mock_codex_jsonl_agent_text("copy me"));
    repo.write_mock(
        "pbcopy",
        r#"#!/usr/bin/env bash
cat >/dev/null
sleep 2
exit 0
"#,
    );

    let out = repo.run_with_env(
        &["cxcopy", "echo", "hello"],
        &[("CX_CMD_TIMEOUT_SECS", "9"), ("CX_TIMEOUT_SHELL_SECS", "1")],
    );
    assert!(
        !out.status.success(),
        "expected clipboard timeout failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("pbcopy timed out after 1s"),
        "expected shell timeout override in stderr: {}",
        stderr_str(&out)
    );
}

#[test]
fn schema_failure_injection_creates_quarantine_and_run_flags() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"not-json"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":123,"cached_input_tokens":23,"output_tokens":10}}'
"#,
    );

    let out = repo.run_with_env(&["next", "echo", "hello"], &[]);
    assert!(!out.status.success(), "expected schema failure");

    let sf_log = parse_jsonl(&repo.schema_fail_log());
    let sf_last = sf_log.last().expect("schema failure row");
    let qid = sf_last
        .get("quarantine_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(!qid.is_empty(), "missing quarantine_id in schema_failures");
    assert!(
        repo.quarantine_file(qid).exists(),
        "missing quarantine file for {qid}"
    );

    let runs = parse_jsonl(&repo.runs_log());
    let last = runs.last().expect("last run");
    assert_required_run_fields(last);
    assert_eq!(
        last.get("schema_enforced").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        last.get("schema_valid").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(last.get("quarantine_id").and_then(Value::as_str), Some(qid));
}

#[test]
fn missing_schema_file_fails_structured_command_cleanly() {
    let repo = TempRepo::new("cxrs-rel");
    let schema_file = repo
        .root
        .join(".codex")
        .join("schemas")
        .join("next.schema.json");
    fs::remove_file(&schema_file).expect("remove next schema");
    repo.write_mock(
        "codex",
        &mock_codex_jsonl_agent_text("{\"commands\":[\"echo ok\"]}"),
    );

    let out = repo.run_with_env(&["next", "echo", "hello"], &[]);
    assert!(
        !out.status.success(),
        "expected missing schema failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("failed to read"),
        "missing schema error not surfaced: {}",
        stderr_str(&out)
    );
}

#[test]
fn corrupted_quarantine_record_fails_show_with_clear_error() {
    let repo = TempRepo::new("cxrs-rel");
    let qdir = repo.root.join(".codex").join("quarantine");
    fs::create_dir_all(&qdir).expect("create quarantine dir");
    let qid = "bad_record";
    fs::write(qdir.join(format!("{qid}.json")), "{broken json").expect("write broken quarantine");

    let out = repo.run_with_env(&["quarantine", "show", qid], &[]);
    assert!(
        !out.status.success(),
        "expected corrupted quarantine failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("invalid quarantine JSON"),
        "expected invalid quarantine JSON error: {}",
        stderr_str(&out)
    );
}

#[test]
fn unwritable_quarantine_path_surfaces_schema_failure_io_error() {
    let repo = TempRepo::new("cxrs-rel");
    let qdir = repo.root.join(".codex").join("quarantine");
    fs::create_dir_all(&qdir).expect("create quarantine dir");
    #[cfg(unix)]
    set_readonly(&qdir);

    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"not-json"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":32,"cached_input_tokens":1,"output_tokens":9}}'
"#,
    );

    let out = repo.run_with_env(&["next", "echo", "hello"], &[]);
    #[cfg(unix)]
    set_writable(&qdir);

    assert!(
        !out.status.success(),
        "expected failure for unwritable quarantine; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("failed to write"),
        "expected IO write error in stderr: {}",
        stderr_str(&out)
    );
}

#[test]
fn unwritable_run_log_path_does_not_break_command_execution() {
    let repo = TempRepo::new("cxrs-rel");
    let logs_dir = repo.root.join(".codex").join("cxlogs");
    fs::create_dir_all(&logs_dir).expect("create logs dir");
    #[cfg(unix)]
    set_readonly(&logs_dir);
    repo.write_mock("codex", &mock_codex_jsonl_agent_text("ok"));

    let out = repo.run_with_env(&["cxo", "echo", "hello"], &[]);
    #[cfg(unix)]
    set_writable(&logs_dir);
    assert!(
        out.status.success(),
        "command should still succeed when logging path is unwritable; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
}

#[test]
fn replay_is_deterministic_and_schema_valid_over_repeated_runs() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"{\"commands\":[\"echo ok\",\"git status --short\"]}"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":80,"cached_input_tokens":10,"output_tokens":12}}'
"#,
    );

    let next_schema = fs::read_to_string(
        repo.root
            .join(".codex")
            .join("schemas")
            .join("next.schema.json"),
    )
    .expect("read next schema");
    let qid = "fixture_replay_next";
    let q = json!({
        "id": qid,
        "ts": "2026-01-01T00:00:00Z",
        "tool": "next",
        "reason": "invalid_json",
        "schema": next_schema,
        "prompt": "Command: git status --short\nOutput: M src/main.rs",
        "prompt_sha256": "fixture",
        "raw_response": "not-json",
        "raw_sha256": "fixture",
        "attempts": []
    });
    fs::create_dir_all(repo.root.join(".codex").join("quarantine")).expect("create quarantine dir");
    fs::write(
        repo.quarantine_file(qid),
        serde_json::to_string_pretty(&q).expect("serialize fixture"),
    )
    .expect("write quarantine fixture");

    let mut baseline: Option<Value> = None;
    for _ in 0..5 {
        let out = repo.run_with_env(&["replay", qid], &[]);
        assert!(
            out.status.success(),
            "replay failed; stdout={} stderr={}",
            stdout_str(&out),
            stderr_str(&out)
        );
        let parsed: Value =
            serde_json::from_str(stdout_str(&out).trim()).expect("valid replay JSON");
        let commands = parsed
            .get("commands")
            .and_then(Value::as_array)
            .expect("commands array");
        assert!(!commands.is_empty(), "commands must be non-empty");
        if let Some(b) = baseline.as_ref() {
            assert_eq!(parsed, *b, "replay output drifted");
        } else {
            baseline = Some(parsed);
        }
    }
}

#[test]
fn ollama_timeout_failure_logs_backend_and_timeout_fields() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock(
        "ollama",
        r#"#!/usr/bin/env bash
sleep 2
exit 0
"#,
    );

    let out = repo.run_with_env(
        &["cxo", "echo", "hello"],
        &[
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "llama3.1"),
            ("CX_CMD_TIMEOUT_SECS", "1"),
        ],
    );
    assert!(
        !out.status.success(),
        "expected ollama timeout; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let runs = parse_jsonl(&repo.runs_log());
    let last = runs.last().expect("last run");
    assert_required_run_fields(last);
    assert_eq!(
        last.get("backend_used").and_then(Value::as_str),
        Some("ollama")
    );
    assert_eq!(last.get("timed_out").and_then(Value::as_bool), Some(true));
    assert_eq!(last.get("timeout_secs").and_then(Value::as_u64), Some(1));
    assert_eq!(
        last.get("schema_valid").and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn ollama_model_unset_set_transitions_are_persisted_and_enforced() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock("ollama", "#!/usr/bin/env bash\ncat >/dev/null\necho ok\n");

    let unset_all = repo.run_with_env(&["llm", "unset", "all"], &[]);
    assert!(unset_all.status.success(), "unset all should succeed");

    let use_backend_only = repo.run_with_env(&["llm", "use", "ollama"], &[]);
    assert!(
        use_backend_only.status.success(),
        "llm use ollama should succeed; stdout={} stderr={}",
        stdout_str(&use_backend_only),
        stderr_str(&use_backend_only)
    );

    let missing_model_run = repo.run_with_env(&["cxo", "echo", "hello"], &[]);
    assert!(
        !missing_model_run.status.success(),
        "expected failure when ollama model unset; stdout={} stderr={}",
        stdout_str(&missing_model_run),
        stderr_str(&missing_model_run)
    );
    assert!(
        stderr_str(&missing_model_run).contains("ollama model is unset"),
        "missing unset model guidance: {}",
        stderr_str(&missing_model_run)
    );

    let set_model = repo.run_with_env(&["llm", "set-model", "llama3.1"], &[]);
    assert!(set_model.status.success(), "set-model should succeed");
    let show = repo.run_with_env(&["llm", "show"], &[]);
    let show_text = stdout_str(&show);
    assert!(show_text.contains("llm_backend: ollama"), "{show_text}");
    assert!(show_text.contains("ollama_model: llama3.1"), "{show_text}");

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .and_then(Value::as_str),
        Some("ollama")
    );
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("ollama_model"))
            .and_then(Value::as_str),
        Some("llama3.1")
    );
}

#[test]
fn ollama_schema_malformed_output_creates_quarantine() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock(
        "ollama",
        r#"#!/usr/bin/env bash
cat >/dev/null
echo "this is not json"
"#,
    );

    let out = repo.run_with_env(
        &["next", "echo", "hello"],
        &[
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "llama3.1"),
        ],
    );
    assert!(
        !out.status.success(),
        "expected schema failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let sf_log = parse_jsonl(&repo.schema_fail_log());
    let sf_last = sf_log.last().expect("schema failure row");
    let qid = sf_last
        .get("quarantine_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(!qid.is_empty(), "expected quarantine_id");
    assert!(
        repo.quarantine_file(qid).exists(),
        "missing quarantine file"
    );

    let runs = parse_jsonl(&repo.runs_log());
    let last = runs.last().expect("last run");
    assert_eq!(
        last.get("backend_used").and_then(Value::as_str),
        Some("ollama")
    );
    assert_eq!(
        last.get("schema_valid").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(last.get("quarantine_id").and_then(Value::as_str), Some(qid));
}

#[test]
fn ollama_schema_commands_remain_enforced_when_mode_is_lean() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock(
        "ollama",
        r#"#!/usr/bin/env bash
cat >/dev/null
echo '{"commands":["git status --short","cargo test -q"]}'
"#,
    );

    let out = repo.run_with_env(
        &["next", "echo", "hello"],
        &[
            ("CX_MODE", "lean"),
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "llama3.1"),
        ],
    );
    assert!(
        out.status.success(),
        "expected schema success; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let text = stdout_str(&out);
    assert!(text.contains("git status --short"), "{text}");
    assert!(text.contains("cargo test -q"), "{text}");

    let runs = parse_jsonl(&repo.runs_log());
    let last = runs.last().expect("last run");
    assert_eq!(
        last.get("backend_used").and_then(Value::as_str),
        Some("ollama")
    );
    assert_eq!(
        last.get("execution_mode").and_then(Value::as_str),
        Some("lean")
    );
    assert_eq!(
        last.get("schema_enforced").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        last.get("schema_valid").and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn rtk_failure_falls_back_to_native_and_logs_capture_provider() {
    let repo = TempRepo::new("cxrs-rel");
    repo.write_mock("codex", &mock_codex_jsonl_agent_text("ok"));
    repo.write_mock(
        "rtk",
        r#"#!/usr/bin/env bash
if [ "$1" = "--help" ]; then exit 0; fi
if [ "$1" = "--version" ]; then
  echo "rtk 0.22.1"
  exit 0
fi
exit 2
"#,
    );

    let out = repo.run_with_env(
        &["cxo", "git", "status", "--short"],
        &[
            ("CX_CAPTURE_PROVIDER", "rtk"),
            ("CX_RTK_SYSTEM", "1"),
            ("CX_NATIVE_REDUCE", "0"),
        ],
    );
    assert!(
        out.status.success(),
        "expected success with native fallback; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let runs = parse_jsonl(&repo.runs_log());
    let last = runs.last().expect("last run");
    assert_required_run_fields(last);
    assert_eq!(
        last.get("capture_provider").and_then(Value::as_str),
        Some("native")
    );
    assert_eq!(last.get("rtk_used").and_then(Value::as_bool), Some(false));
}

#[test]
fn fix_run_policy_block_is_logged_with_reason() {
    let repo = TempRepo::new("cxrs-rel");
    let fix_json = r#"{"analysis":"dangerous path","commands":["rm -rf /tmp/cxrs-danger-test"]}"#;
    repo.write_mock("codex", &mock_codex_jsonl_agent_text(fix_json));

    let out = repo.run_with_env(
        &["fix-run", "echo", "hello"],
        &[
            ("CXFIX_RUN", "1"),
            ("CXFIX_FORCE", "0"),
            ("CX_UNSAFE", "0"),
            ("CX_TIMEOUT_LLM_SECS", "20"),
        ],
    );
    assert!(
        out.status.success(),
        "fix-run should return wrapped command exit status; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("blocked dangerous command"),
        "expected policy warning in stderr: {}",
        stderr_str(&out)
    );

    let runs = parse_jsonl(&repo.runs_log());
    let last = runs.last().expect("last run");
    assert_required_run_fields(last);
    assert_eq!(
        last.get("policy_blocked").and_then(Value::as_bool),
        Some(true)
    );
    assert!(
        last.get("policy_reason")
            .and_then(Value::as_str)
            .is_some_and(|s| s.contains("rm -rf")),
        "policy_reason missing rm -rf context: {last}"
    );
}

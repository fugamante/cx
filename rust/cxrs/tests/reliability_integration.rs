use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn git_bin() -> String {
    if let Ok(v) = std::env::var("GIT_BIN")
        && !v.trim().is_empty()
    {
        return v;
    }
    for c in ["git", "/opt/homebrew/bin/git", "/usr/bin/git"] {
        if std::process::Command::new(c)
            .arg("--version")
            .output()
            .is_ok()
        {
            return c.to_string();
        }
    }
    "git".to_string()
}

fn init_git_repo_with_retry(root: &Path, template_dir: &Path) {
    let mut last = None;
    for _ in 0..5 {
        let out = Command::new(git_bin())
            .arg("init")
            .arg("-q")
            .arg(format!("--template={}", template_dir.display()))
            .current_dir(root)
            .output()
            .expect("run git init");
        if out.status.success() {
            return;
        }
        last = Some(out);
        sleep(Duration::from_millis(50));
    }
    panic!("git init failed after retries: {:?}", last);
}

struct TempRepo {
    root: PathBuf,
    home: PathBuf,
    mock_bin: PathBuf,
    original_path: String,
}

impl TempRepo {
    fn new() -> Self {
        let base = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let root = base.join(format!("cxrs-rel-repo-{}-{}", std::process::id(), ts));
        let home = base.join(format!("cxrs-rel-home-{}-{}", std::process::id(), ts));
        let mock_bin = base.join(format!("cxrs-rel-mockbin-{}-{}", std::process::id(), ts));

        fs::create_dir_all(&root).expect("create temp repo dir");
        fs::create_dir_all(&home).expect("create temp home dir");
        fs::create_dir_all(&mock_bin).expect("create mock bin dir");

        let template_dir = root.join(".git-template");
        fs::create_dir_all(&template_dir).expect("create git template dir");
        init_git_repo_with_retry(&root, &template_dir);

        let original_path = std::env::var("PATH").unwrap_or_default();
        let me = Self {
            root,
            home,
            mock_bin,
            original_path,
        };
        me.copy_schema_registry();
        me
    }

    fn copy_schema_registry(&self) {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(".codex")
            .join("schemas");
        let dst = self.root.join(".codex").join("schemas");
        fs::create_dir_all(&dst).expect("create schema dst dir");
        for entry in fs::read_dir(&src).expect("read schema src dir") {
            let entry = entry.expect("schema dir entry");
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) == Some("json") {
                let fname = path.file_name().expect("schema filename");
                fs::copy(&path, dst.join(fname)).expect("copy schema file");
            }
        }
    }

    fn write_mock(&self, name: &str, body: &str) {
        let p = self.mock_bin.join(name);
        fs::write(&p, body).expect("write mock");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&p).expect("mock metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms).expect("set mock executable");
        }
    }

    fn run_with_env(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let path = format!("{}:{}", self.mock_bin.display(), self.original_path);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cxrs"));
        cmd.args(args)
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("PATH", path)
            .env("CX_RTK_SYSTEM", "0");
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("run cxrs")
    }

    fn runs_log(&self) -> PathBuf {
        self.root.join(".codex").join("cxlogs").join("runs.jsonl")
    }

    fn schema_fail_log(&self) -> PathBuf {
        self.root
            .join(".codex")
            .join("cxlogs")
            .join("schema_failures.jsonl")
    }

    fn quarantine_file(&self, id: &str) -> PathBuf {
        self.root
            .join(".codex")
            .join("quarantine")
            .join(format!("{id}.json"))
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.home);
        let _ = fs::remove_dir_all(&self.mock_bin);
    }
}

fn stdout_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn parse_jsonl(path: &Path) -> Vec<Value> {
    let text = fs::read_to_string(path).expect("read jsonl");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("valid json line"))
        .collect()
}

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
    let repo = TempRepo::new();
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
fn schema_failure_injection_creates_quarantine_and_run_flags() {
    let repo = TempRepo::new();
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
fn replay_is_deterministic_and_schema_valid_over_repeated_runs() {
    let repo = TempRepo::new();
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
    let repo = TempRepo::new();
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
fn rtk_failure_falls_back_to_native_and_logs_capture_provider() {
    let repo = TempRepo::new();
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
    let repo = TempRepo::new();
    let fix_json = r#"{"analysis":"dangerous path","commands":["rm -rf /tmp/cxrs-danger-test"]}"#;
    repo.write_mock("codex", &mock_codex_jsonl_agent_text(fix_json));

    let out = repo.run_with_env(
        &["fix-run", "echo", "hello"],
        &[("CXFIX_RUN", "1"), ("CXFIX_FORCE", "0"), ("CX_UNSAFE", "0")],
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

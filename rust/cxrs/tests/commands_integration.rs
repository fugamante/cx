use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

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
        let root = base.join(format!("cxrs-it-repo-{}-{}", std::process::id(), ts));
        let home = base.join(format!("cxrs-it-home-{}-{}", std::process::id(), ts));
        let mock_bin = base.join(format!("cxrs-it-mockbin-{}-{}", std::process::id(), ts));

        fs::create_dir_all(&root).expect("create temp repo dir");
        fs::create_dir_all(&home).expect("create temp home dir");
        fs::create_dir_all(&mock_bin).expect("create mock bin dir");

        let init = Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(&root)
            .output()
            .expect("run git init");
        assert!(init.status.success(), "git init failed: {init:?}");

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

    fn write_mock_codex(&self, body: &str) {
        let codex_path = self.mock_bin.join("codex");
        fs::write(&codex_path, body).expect("write mock codex");
        let mut perms = fs::metadata(&codex_path)
            .expect("codex metadata")
            .permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            fs::set_permissions(&codex_path, perms).expect("set mock codex executable");
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        let path = format!("{}:{}", self.mock_bin.display(), self.original_path);
        Command::new(env!("CARGO_BIN_EXE_cxrs"))
            .args(args)
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("PATH", path)
            .env("CX_RTK_SYSTEM", "0")
            .output()
            .expect("run cxrs command")
    }

    fn tasks_file(&self) -> PathBuf {
        self.root.join(".codex").join("tasks.json")
    }

    fn schema_fail_log(&self) -> PathBuf {
        self.root
            .join(".codex")
            .join("cxlogs")
            .join("schema_failures.jsonl")
    }

    fn runs_log(&self) -> PathBuf {
        self.root.join(".codex").join("cxlogs").join("runs.jsonl")
    }

    fn quarantine_dir(&self) -> PathBuf {
        self.root.join(".codex").join("quarantine")
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

fn read_json(path: &Path) -> Value {
    let s = fs::read_to_string(path).expect("read json file");
    serde_json::from_str(&s).expect("parse json")
}

#[test]
fn task_lifecycle_add_claim_complete() {
    let repo = TempRepo::new();

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
fn schema_failure_creates_quarantine_and_logs() {
    let repo = TempRepo::new();

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
    let q_entries: Vec<_> = fs::read_dir(&qdir)
        .expect("read quarantine dir")
        .filter_map(Result::ok)
        .collect();
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
fn command_parsing_and_file_io_edge_cases() {
    let repo = TempRepo::new();

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

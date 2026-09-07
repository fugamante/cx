#![allow(dead_code)]
#![allow(unused_imports)]

pub mod fixture_http;
pub mod json_contract;
pub mod telemetry_helpers;

pub use fixture_http::{FixtureHttpRequest, run_fixture_http_server_once};
pub use json_contract::{
    assert_actions_contract, assert_fixture_contract, assert_has_keys, fixture_keys,
    load_fixture_json,
};
pub use telemetry_helpers::parse_labeled_u64;

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn unique_test_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn git_bin() -> String {
    if let Ok(v) = std::env::var("GIT_BIN")
        && !v.trim().is_empty()
        && Command::new(&v).arg("--version").output().is_ok()
    {
        return v;
    }
    for c in ["git", "/opt/homebrew/bin/git", "/usr/bin/git"] {
        if Command::new(c).arg("--version").output().is_ok() {
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
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
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

fn find_repo_root(mut cur: PathBuf) -> Option<PathBuf> {
    for _ in 0..6 {
        if cur.join(".cx").join("schemas").is_dir() && cur.join("bin").join("cx").is_file() {
            return Some(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    None
}

fn repo_root() -> PathBuf {
    let runtime_dir = std::env::current_dir().expect("resolve test working directory");
    // Cached test binaries may outlive a disposable build worktree.
    for start in [runtime_dir, PathBuf::from(env!("CARGO_MANIFEST_DIR"))] {
        if let Some(root) = find_repo_root(start) {
            return root;
        }
    }
    panic!(
        "unable to resolve repo root from runtime or compiled manifest path ({})",
        env!("CARGO_MANIFEST_DIR")
    );
}

pub struct TempRepo {
    pub root: PathBuf,
    pub home: PathBuf,
    pub mock_bin: PathBuf,
    original_path: String,
}

impl TempRepo {
    pub fn new(prefix: &str) -> Self {
        let base = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let uniq = unique_test_id();
        let root = base.join(format!("{prefix}-repo-{}-{ts}-{uniq}", std::process::id()));
        let home = base.join(format!("{prefix}-home-{}-{ts}-{uniq}", std::process::id()));
        let mock_bin = base.join(format!(
            "{prefix}-mockbin-{}-{ts}-{uniq}",
            std::process::id()
        ));

        fs::create_dir_all(&root).expect("create temp repo dir");
        fs::create_dir_all(&home).expect("create temp home dir");
        fs::create_dir_all(&mock_bin).expect("create mock bin dir");

        let template_dir = root.join(".git-template");
        fs::create_dir_all(&template_dir).expect("create git template dir");
        init_git_repo_with_retry(&root, &template_dir);

        let me = Self {
            root,
            home,
            mock_bin,
            original_path: std::env::var("PATH").unwrap_or_default(),
        };
        me.copy_schema_registry();
        me
    }

    pub fn copy_schema_registry(&self) {
        let src = repo_root().join(".cx").join("schemas");
        let dst = self.root.join(".cx").join("schemas");
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

    pub fn write_mock(&self, name: &str, body: &str) {
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

    pub fn write_mock_primary(&self, body: &str) {
        self.write_mock(concat!("co", "dex"), body);
    }

    pub fn write_cx_wrapper(&self) {
        let bin_dir = self.root.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let body = format!(
            "#!/usr/bin/env bash\nexec \"{}\" \"$@\"\n",
            env!("CARGO_BIN_EXE_cxrs")
        );
        let path = bin_dir.join("cx");
        fs::write(&path, body).expect("write cx wrapper");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).expect("wrapper metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).expect("set wrapper executable");
        }
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.run_with_env(args, &[])
    }

    pub fn run_with_env(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let path = format!("{}:{}", self.mock_bin.display(), self.original_path);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cxrs"));
        cmd.args(args)
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("PATH", path)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE");
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("run cxrs command")
    }

    pub fn tasks_file(&self) -> PathBuf {
        self.root.join(".cx").join("tasks.json")
    }

    pub fn schema_fail_log(&self) -> PathBuf {
        self.root
            .join(".cx")
            .join("cxlogs")
            .join("schema_failures.jsonl")
    }

    pub fn runs_log(&self) -> PathBuf {
        self.root.join(".cx").join("cxlogs").join("runs.jsonl")
    }

    pub fn task_events_log(&self) -> PathBuf {
        self.root
            .join(".codex")
            .join("cxlogs")
            .join("task_events.jsonl")
    }

    pub fn quarantine_dir(&self) -> PathBuf {
        self.root.join(".cx").join("quarantine")
    }

    pub fn quarantine_file(&self, id: &str) -> PathBuf {
        self.root
            .join(".cx")
            .join("quarantine")
            .join(format!("{id}.json"))
    }

    pub fn state_file(&self) -> PathBuf {
        self.root.join(".cx").join("state.json")
    }

    pub fn quota_catalog_file(&self) -> PathBuf {
        self.root.join(".cx").join("quota_catalog.json")
    }

    pub fn local_models_file(&self) -> PathBuf {
        self.root.join(".cx").join("local_models.json")
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.home);
        let _ = fs::remove_dir_all(&self.mock_bin);
    }
}

pub fn stdout_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

pub fn stderr_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

pub fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).expect("read json");
    serde_json::from_str::<Value>(&text).expect("parse json")
}

pub fn write_runs_log_row(repo: &TempRepo, row: &Value) {
    write_runs_log_rows(repo, std::slice::from_ref(row));
}

pub fn write_runs_log_rows(repo: &TempRepo, rows: &[Value]) {
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");
}

pub fn parse_jsonl(path: &Path) -> Vec<Value> {
    let mut text = None;
    for _ in 0..20 {
        match fs::read_to_string(path) {
            Ok(v) => {
                text = Some(v);
                break;
            }
            Err(_) => sleep(Duration::from_millis(50)),
        }
    }
    let text = text.unwrap_or_else(|| panic!("read jsonl: {}", path.display()));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("valid json line"))
        .collect()
}

pub fn expect_schema_fail(repo: &TempRepo) -> String {
    let qdir = repo.quarantine_dir();
    let mut has_entries = false;
    for _ in 0..20 {
        if let Ok(rd) = fs::read_dir(&qdir)
            && rd.filter_map(Result::ok).next().is_some()
        {
            has_entries = true;
            break;
        }
        sleep(Duration::from_millis(50));
    }
    assert!(
        has_entries,
        "expected quarantine entries in {}",
        qdir.display()
    );

    let sf_last = parse_jsonl(&repo.schema_fail_log())
        .into_iter()
        .last()
        .expect("schema failure log row");
    let qid = sf_last
        .get("quarantine_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    assert!(!qid.is_empty(), "schema failure log missing quarantine_id");

    let run_last = parse_jsonl(&repo.runs_log())
        .into_iter()
        .last()
        .expect("last run row");
    assert_eq!(
        run_last.get("schema_valid").and_then(Value::as_bool),
        Some(false),
        "expected schema_valid=false in run log row: {run_last}"
    );
    assert_eq!(
        run_last.get("quarantine_id").and_then(Value::as_str),
        Some(qid.as_str()),
        "run log quarantine_id should match schema failure row: {run_last}"
    );
    qid
}

pub fn write_quarantine_fixture(
    repo: &TempRepo,
    id: &str,
    tool: &str,
    schema: &str,
    prompt: &str,
    raw_response: &str,
) {
    fn hash(s: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    let payload = serde_json::json!({
        "id": id,
        "ts": "2026-01-01T00:00:00Z",
        "tool": tool,
        "reason": "invalid_json",
        "schema": schema,
        "prompt": prompt,
        "prompt_sha256": hash(prompt),
        "raw_response": raw_response,
        "raw_sha256": hash(raw_response),
        "attempts": []
    });
    fs::create_dir_all(repo.quarantine_dir()).expect("create quarantine dir");
    fs::write(
        repo.quarantine_file(id),
        serde_json::to_string_pretty(&payload).expect("serialize quarantine fixture"),
    )
    .expect("write quarantine fixture");
}

#[cfg(unix)]
pub fn set_readonly(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o555);
    fs::set_permissions(path, perms).expect("set readonly");
}

#[cfg(unix)]
pub fn set_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("set writable");
}

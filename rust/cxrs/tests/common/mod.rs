#![allow(dead_code)]

use serde_json::Value;
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
        let root = base.join(format!("{prefix}-repo-{}-{ts}", std::process::id()));
        let home = base.join(format!("{prefix}-home-{}-{ts}", std::process::id()));
        let mock_bin = base.join(format!("{prefix}-mockbin-{}-{ts}", std::process::id()));

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

    pub fn write_mock_codex(&self, body: &str) {
        self.write_mock("codex", body);
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
            .env("CX_RTK_SYSTEM", "0");
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("run cxrs command")
    }

    pub fn tasks_file(&self) -> PathBuf {
        self.root.join(".codex").join("tasks.json")
    }

    pub fn schema_fail_log(&self) -> PathBuf {
        self.root
            .join(".codex")
            .join("cxlogs")
            .join("schema_failures.jsonl")
    }

    pub fn runs_log(&self) -> PathBuf {
        self.root.join(".codex").join("cxlogs").join("runs.jsonl")
    }

    pub fn quarantine_dir(&self) -> PathBuf {
        self.root.join(".codex").join("quarantine")
    }

    pub fn quarantine_file(&self, id: &str) -> PathBuf {
        self.root
            .join(".codex")
            .join("quarantine")
            .join(format!("{id}.json"))
    }

    pub fn state_file(&self) -> PathBuf {
        self.root.join(".codex").join("state.json")
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

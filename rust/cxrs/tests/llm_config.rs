use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn init_git_repo_with_retry(root: &Path, template_dir: &Path) {
    let mut last = None;
    for _ in 0..5 {
        let out = Command::new("git")
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
}

impl TempRepo {
    fn new() -> Self {
        let base = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let root = base.join(format!("cxrs-test-repo-{}-{}", std::process::id(), ts));
        let home = base.join(format!("cxrs-test-home-{}-{}", std::process::id(), ts));
        fs::create_dir_all(&root).expect("create temp repo dir");
        fs::create_dir_all(&home).expect("create temp home dir");

        let template_dir = root.join(".git-template");
        fs::create_dir_all(&template_dir).expect("create git template dir");
        init_git_repo_with_retry(&root, &template_dir);

        Self { root, home }
    }

    fn state_file(&self) -> PathBuf {
        self.root.join(".codex").join("state.json")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_cxrs"))
            .args(args)
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .output()
            .expect("run cxrs command")
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.home);
    }
}

fn stdout_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).expect("read json file");
    serde_json::from_str::<Value>(&text).expect("parse json")
}

#[test]
fn llm_use_persists_backend_and_model() {
    let repo = TempRepo::new();

    let out = repo.run(&["llm", "use", "ollama", "llama3.1"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let show = repo.run(&["llm", "show"]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let text = stdout_str(&show);
    assert!(text.contains("llm_backend: ollama"), "{text}");
    assert!(text.contains("ollama_model: llama3.1"), "{text}");

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
fn llm_unset_can_clear_model_backend_and_all() {
    let repo = TempRepo::new();

    let out = repo.run(&["llm", "use", "ollama", "llama3.1"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let unset_model = repo.run(&["llm", "unset", "model"]);
    assert!(
        unset_model.status.success(),
        "stdout={} stderr={}",
        stdout_str(&unset_model),
        stderr_str(&unset_model)
    );

    let show_after_model = repo.run(&["llm", "show"]);
    assert!(show_after_model.status.success());
    let show_text = stdout_str(&show_after_model);
    assert!(show_text.contains("llm_backend: ollama"), "{show_text}");
    assert!(show_text.contains("ollama_model: <unset>"), "{show_text}");

    let unset_backend = repo.run(&["llm", "unset", "backend"]);
    assert!(unset_backend.status.success());
    let show_after_backend = repo.run(&["llm", "show"]);
    let show_backend_text = stdout_str(&show_after_backend);
    assert!(
        show_backend_text.contains("llm_backend: codex"),
        "{show_backend_text}"
    );

    let out2 = repo.run(&["llm", "use", "ollama", "llama3.1"]);
    assert!(out2.status.success());
    let unset_all = repo.run(&["llm", "unset", "all"]);
    assert!(unset_all.status.success());

    let state = read_json(&repo.state_file());
    assert!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .is_some_and(Value::is_null)
    );
    assert!(
        state
            .get("preferences")
            .and_then(|v| v.get("ollama_model"))
            .is_some_and(Value::is_null)
    );
}

#[test]
fn ollama_without_model_fails_non_interactive_with_clear_error() {
    let repo = TempRepo::new();

    assert!(repo.run(&["llm", "unset", "all"]).status.success());
    assert!(repo.run(&["llm", "use", "ollama"]).status.success());

    let out = repo.run(&["cxo", "echo", "hi"]);
    assert!(
        !out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let err = stderr_str(&out);
    assert!(
        err.contains("ollama model is unset"),
        "expected unset-model guidance in stderr; got: {err}"
    );
}

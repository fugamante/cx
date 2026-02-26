use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub fn repo_root() -> Option<PathBuf> {
    static CACHED: OnceLock<Option<PathBuf>> = OnceLock::new();
    if env::var("CX_NO_CACHE").ok().as_deref() == Some("1") {
        return repo_root_uncached();
    }
    CACHED.get_or_init(repo_root_uncached).as_ref().cloned()
}

pub fn repo_root_hint() -> Option<PathBuf> {
    if let Ok(v) = env::var("CX_REPO_ROOT") {
        let p = PathBuf::from(v);
        if p.exists() {
            return Some(p);
        }
    }
    repo_root()
}

fn repo_root_uncached() -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

pub fn resolve_log_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".codex").join("cxlogs").join("runs.jsonl"));
    }
    home_dir().map(|h| h.join(".codex").join("cxlogs").join("runs.jsonl"))
}

pub fn resolve_schema_fail_log_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(
            root.join(".codex")
                .join("cxlogs")
                .join("schema_failures.jsonl"),
        );
    }
    home_dir().map(|h| {
        h.join(".codex")
            .join("cxlogs")
            .join("schema_failures.jsonl")
    })
}

pub fn resolve_quarantine_dir() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".codex").join("quarantine"));
    }
    home_dir().map(|h| h.join(".codex").join("quarantine"))
}

pub fn resolve_state_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".codex").join("state.json"));
    }
    home_dir().map(|h| h.join(".codex").join("state.json"))
}

pub fn resolve_tasks_file() -> Result<PathBuf, String> {
    let root = repo_root().ok_or_else(|| "cx task: not inside a git repository".to_string())?;
    Ok(root.join(".codex").join("tasks.json"))
}

pub fn resolve_schema_dir() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".codex").join("schemas"));
    }
    home_dir().map(|h| h.join(".codex").join("schemas"))
}

pub fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("failed to create {}: {e}", parent.display()))
}

use std::env;
use std::path::{Path, PathBuf};

use crate::config::cli_app_name;
use std::process::Command;
#[cfg(not(test))]
use std::sync::OnceLock;

use crate::process::run_command_output_with_timeout;

pub fn repo_root() -> Option<PathBuf> {
    #[cfg(test)]
    {
        repo_root_uncached()
    }
    #[cfg(not(test))]
    {
        static CACHED: OnceLock<Option<PathBuf>> = OnceLock::new();
        if env::var("CX_NO_CACHE").ok().as_deref() == Some("1") {
            return repo_root_uncached();
        }
        CACHED.get_or_init(repo_root_uncached).as_ref().cloned()
    }
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
    let mut cmd = Command::new("git");
    cmd.args(["rev-parse", "--show-toplevel"]);
    cmd.env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    let out = run_command_output_with_timeout(cmd, "git rev-parse --show-toplevel").ok()?;
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
        return Some(root.join(".cx").join("cxlogs").join("runs.jsonl"));
    }
    home_dir().map(|h| h.join(".cx").join("cxlogs").join("runs.jsonl"))
}

pub fn resolve_schema_fail_log_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(
            root.join(".cx")
                .join("cxlogs")
                .join("schema_failures.jsonl"),
        );
    }
    home_dir().map(|h| h.join(".cx").join("cxlogs").join("schema_failures.jsonl"))
}

pub fn task_events_log() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".codex").join("cxlogs").join("task_events.jsonl"));
    }
    home_dir().map(|h| h.join(".codex").join("cxlogs").join("task_events.jsonl"))
}

pub fn resolve_quarantine_dir() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".cx").join("quarantine"));
    }
    home_dir().map(|h| h.join(".cx").join("quarantine"))
}

pub fn resolve_state_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".cx").join("state.json"));
    }
    home_dir().map(|h| h.join(".cx").join("state.json"))
}

pub fn resolve_quota_catalog_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".cx").join("quota_catalog.json"));
    }
    home_dir().map(|h| h.join(".cx").join("quota_catalog.json"))
}

pub fn resolve_models_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".cx").join("local_models.json"));
    }
    home_dir().map(|h| h.join(".cx").join("local_models.json"))
}

pub fn resolve_tasks_file() -> Result<PathBuf, String> {
    let root = repo_root()
        .ok_or_else(|| format!("{} task: not inside a git repository", cli_app_name()))?;
    Ok(root.join(".cx").join("tasks.json"))
}

pub fn resolve_schema_dir() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".cx").join("schemas"));
    }
    home_dir().map(|h| h.join(".cx").join("schemas"))
}

pub fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("failed to create {}: {e}", parent.display()))
}

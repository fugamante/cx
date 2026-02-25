use crate::paths::{ensure_parent_dir, resolve_state_file};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static STATE_CACHE: OnceLock<Mutex<Option<Value>>> = OnceLock::new();

pub fn state_cache_clear() {
    if let Ok(mut g) = STATE_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *g = None;
    }
}

pub fn read_state_value() -> Option<Value> {
    if std::env::var("CX_NO_CACHE").ok().as_deref() != Some("1") {
        if let Some(v) = STATE_CACHE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .ok()
            .and_then(|g| g.clone())
        {
            return Some(v);
        }
    }
    let state_file = resolve_state_file()?;
    if !state_file.exists() {
        return None;
    }
    let mut s = String::new();
    File::open(state_file).ok()?.read_to_string(&mut s).ok()?;
    let parsed = serde_json::from_str::<Value>(&s).ok()?;
    if std::env::var("CX_NO_CACHE").ok().as_deref() != Some("1") {
        if let Ok(mut g) = STATE_CACHE.get_or_init(|| Mutex::new(None)).lock() {
            *g = Some(parsed.clone());
        }
    }
    Some(parsed)
}

fn default_state_value() -> Value {
    json!({
        "preferences": {
            "llm_backend": Value::Null,
            "ollama_model": Value::Null,
            "conventional_commits": Value::Null,
            "pr_summary_format": Value::Null
        },
        "runtime": {
            "current_task_id": Value::Null,
            "current_task_parent_id": Value::Null
        },
        "alert_overrides": {},
        "last_model": Value::Null
    })
}

pub fn ensure_state_value() -> Result<(PathBuf, Value), String> {
    let state_file =
        resolve_state_file().ok_or_else(|| "unable to resolve state file".to_string())?;
    if !state_file.exists() {
        ensure_parent_dir(&state_file)?;
        let initial = default_state_value();
        write_json_atomic(&state_file, &initial)?;
        return Ok((state_file, initial));
    }
    let mut s = String::new();
    File::open(&state_file)
        .map_err(|e| format!("cannot open {}: {e}", state_file.display()))?
        .read_to_string(&mut s)
        .map_err(|e| format!("cannot read {}: {e}", state_file.display()))?;
    let value = serde_json::from_str::<Value>(&s)
        .map_err(|e| format!("invalid JSON in {}: {e}", state_file.display()))?;
    Ok((state_file, value))
}

pub fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    ensure_parent_dir(path)?;
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut serialized = serde_json::to_string_pretty(value)
        .map_err(|e| format!("failed to serialize JSON: {e}"))?;
    serialized.push('\n');
    fs::write(&tmp, serialized).map_err(|e| format!("failed to write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| {
        format!(
            "failed to move {} -> {}: {e}",
            tmp.display(),
            path.display()
        )
    })?;
    if path.file_name().and_then(|s| s.to_str()) == Some("state.json") {
        state_cache_clear();
    }
    Ok(())
}


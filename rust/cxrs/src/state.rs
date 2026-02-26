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
    if std::env::var("CX_NO_CACHE").ok().as_deref() != Some("1")
        && let Some(v) = STATE_CACHE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .ok()
            .and_then(|g| g.clone())
    {
        return Some(v);
    }
    let state_file = resolve_state_file()?;
    if !state_file.exists() {
        return None;
    }
    let mut s = String::new();
    File::open(state_file).ok()?.read_to_string(&mut s).ok()?;
    let parsed = serde_json::from_str::<Value>(&s).ok()?;
    if std::env::var("CX_NO_CACHE").ok().as_deref() != Some("1")
        && let Ok(mut g) = STATE_CACHE.get_or_init(|| Mutex::new(None)).lock()
    {
        *g = Some(parsed.clone());
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

pub fn parse_cli_value(raw: &str) -> Value {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        return v;
    }
    if raw.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if raw.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if raw.eq_ignore_ascii_case("null") {
        return Value::Null;
    }
    if let Ok(v) = raw.parse::<i64>() {
        return json!(v);
    }
    if let Ok(v) = raw.parse::<f64>() {
        return json!(v);
    }
    Value::String(raw.to_string())
}

pub fn value_at_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.split('.') {
        if seg.is_empty() {
            continue;
        }
        cur = cur.get(seg)?;
    }
    Some(cur)
}

pub fn set_value_at_path(root: &mut Value, path: &str, new_value: Value) -> Result<(), String> {
    let mut segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        return Err("key cannot be empty".to_string());
    }
    let last = segs.pop().unwrap_or_default();
    let mut cur = root;
    for seg in segs {
        if !cur.is_object() {
            *cur = json!({});
        }
        let obj = cur
            .as_object_mut()
            .ok_or_else(|| "failed to access state object".to_string())?;
        cur = obj.entry(seg.to_string()).or_insert_with(|| json!({}));
    }
    if !cur.is_object() {
        *cur = json!({});
    }
    let obj = cur
        .as_object_mut()
        .ok_or_else(|| "failed to access final state object".to_string())?;
    obj.insert(last.to_string(), new_value);
    Ok(())
}

pub fn set_state_path(path: &str, value: Value) -> Result<(), String> {
    let (state_file, mut state) = ensure_state_value()?;
    set_value_at_path(&mut state, path, value)?;
    write_json_atomic(&state_file, &state)
}

pub fn current_task_id() -> Option<String> {
    if let Ok(v) = std::env::var("CX_TASK_ID")
        && !v.trim().is_empty()
    {
        return Some(v);
    }
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "runtime.current_task_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub fn current_task_parent_id() -> Option<String> {
    if let Ok(v) = std::env::var("CX_TASK_PARENT_ID")
        && !v.trim().is_empty()
    {
        return Some(v);
    }
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "runtime.current_task_parent_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_value_handles_primitives() {
        assert_eq!(parse_cli_value("true"), Value::Bool(true));
        assert_eq!(parse_cli_value("42"), json!(42));
        assert_eq!(parse_cli_value("3.5"), json!(3.5));
        assert_eq!(parse_cli_value("null"), Value::Null);
    }

    #[test]
    fn set_and_get_nested_path() {
        let mut v = json!({});
        set_value_at_path(&mut v, "a.b.c", json!(7)).expect("set nested path");
        assert_eq!(value_at_path(&v, "a.b.c"), Some(&json!(7)));
    }
}

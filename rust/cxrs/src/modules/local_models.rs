use serde_json::{Value, json};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::paths::resolve_models_file;
use crate::state::write_json_atomic;

const CONTRACT_VERSION: &str = "local_models.v1";

#[derive(Debug, Clone)]
pub struct LocalModelRecord {
    pub id: String,
    pub alias: String,
    pub backend: String,
    pub provider: Option<String>,
    pub repo_id: Option<String>,
    pub revision: Option<String>,
    pub resolved_model: String,
    pub local_path: Option<String>,
    pub quantization: Option<String>,
    pub format: Option<String>,
    pub size_bytes: Option<u64>,
    pub cache_path: Option<String>,
    pub last_used_at: Option<String>,
    pub last_smoke_status: Option<String>,
    pub preferred_args: Option<String>,
    pub trust_remote_code: String,
}

#[derive(Debug, Clone)]
pub struct AddLocalModelInput {
    pub id: Option<String>,
    pub alias: String,
    pub backend: String,
    pub provider: Option<String>,
    pub repo_id: Option<String>,
    pub revision: Option<String>,
    pub resolved_model: String,
    pub local_path: Option<String>,
    pub quantization: Option<String>,
    pub format: Option<String>,
    pub size_bytes: Option<u64>,
    pub cache_path: Option<String>,
    pub preferred_args: Option<String>,
    pub trust_remote_code: String,
    pub replace: bool,
}

fn registry_path() -> Result<PathBuf, String> {
    resolve_models_file().ok_or_else(|| "unable to resolve local model registry".to_string())
}

fn empty_registry() -> Value {
    json!({
        "contract_version": CONTRACT_VERSION,
        "models": []
    })
}

fn registry_value() -> Result<Value, String> {
    let path = registry_path()?;
    if !path.exists() {
        return Ok(empty_registry());
    }
    let mut s = String::new();
    File::open(&path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?
        .read_to_string(&mut s)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value = serde_json::from_str::<Value>(&s)
        .map_err(|e| format!("invalid JSON in {}: {e}", path.display()))?;
    Ok(normalize_registry(value))
}

fn normalize_registry(mut value: Value) -> Value {
    if !value.is_object() {
        return empty_registry();
    }
    let obj = value.as_object_mut().expect("registry object");
    obj.entry("contract_version".to_string())
        .or_insert_with(|| json!(CONTRACT_VERSION));
    if !obj.get("models").is_some_and(Value::is_array) {
        obj.insert("models".to_string(), json!([]));
    }
    value
}

fn write_registry(value: &Value) -> Result<(), String> {
    let path = registry_path()?;
    write_json_atomic(&path, value)
}

fn opt_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn required_string(value: &Value, key: &str) -> Result<String, String> {
    opt_string(value, key).ok_or_else(|| format!("local model record missing '{key}'"))
}

fn record_from_value(value: &Value) -> Result<LocalModelRecord, String> {
    Ok(LocalModelRecord {
        id: required_string(value, "id")?,
        alias: required_string(value, "alias")?,
        backend: required_string(value, "backend")?,
        provider: opt_string(value, "provider"),
        repo_id: opt_string(value, "repo_id"),
        revision: opt_string(value, "revision"),
        resolved_model: required_string(value, "resolved_model")?,
        local_path: opt_string(value, "local_path"),
        quantization: opt_string(value, "quantization"),
        format: opt_string(value, "format"),
        size_bytes: value.get("size_bytes").and_then(Value::as_u64),
        cache_path: opt_string(value, "cache_path"),
        last_used_at: opt_string(value, "last_used_at"),
        last_smoke_status: opt_string(value, "last_smoke_status"),
        preferred_args: opt_string(value, "preferred_args"),
        trust_remote_code: opt_string(value, "trust_remote_code")
            .unwrap_or_else(|| "unknown".to_string()),
    })
}

fn record_to_value(record: &LocalModelRecord) -> Value {
    json!({
        "id": record.id,
        "alias": record.alias,
        "backend": record.backend,
        "provider": record.provider,
        "repo_id": record.repo_id,
        "revision": record.revision,
        "resolved_model": record.resolved_model,
        "local_path": record.local_path,
        "quantization": record.quantization,
        "format": record.format,
        "size_bytes": record.size_bytes,
        "cache_path": record.cache_path,
        "last_used_at": record.last_used_at,
        "last_smoke_status": record.last_smoke_status,
        "preferred_args": record.preferred_args,
        "trust_remote_code": record.trust_remote_code
    })
}

pub fn normalize_backend(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "ollama" => Some("ollama"),
        "llamacpp" | "llama.cpp" | "llama_cpp" => Some("llamacpp"),
        "mlx" => Some("mlx"),
        _ => None,
    }
}

fn valid_remote_code(value: &str) -> bool {
    matches!(value, "false" | "true" | "unknown")
}

fn clean_token(name: &str, value: &str) -> Result<String, String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(format!("{name} cannot be empty"));
    }
    if v.chars().any(char::is_whitespace) {
        return Err(format!("{name} cannot contain whitespace"));
    }
    Ok(v.to_string())
}

pub fn list_records() -> Result<Vec<LocalModelRecord>, String> {
    let registry = registry_value()?;
    let mut records = Vec::new();
    for item in registry
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        records.push(record_from_value(&item)?);
    }
    records.sort_by(|a, b| a.backend.cmp(&b.backend).then(a.alias.cmp(&b.alias)));
    Ok(records)
}

fn ambiguous_query_error(query: &str, records: &[LocalModelRecord]) -> String {
    let mut ids = records
        .iter()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    format!(
        "local model selector '{}' is ambiguous across backends; use a backend-scoped id ({})",
        query,
        ids.join(", ")
    )
}

pub fn find_record(query: &str) -> Result<Option<LocalModelRecord>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(None);
    }
    let matches = list_records()?
        .into_iter()
        .filter(|record| record.id == q || record.alias == q)
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(ambiguous_query_error(q, &matches));
    }
    Ok(matches.into_iter().next())
}

pub fn find_record_for_backend(
    query: &str,
    backend: &str,
) -> Result<Option<LocalModelRecord>, String> {
    let Some(canonical_backend) = normalize_backend(backend) else {
        return Ok(None);
    };
    let q = query.trim();
    if q.is_empty() {
        return Ok(None);
    }
    for record in list_records()? {
        if record.backend == canonical_backend
            && (record.id == q || record.alias == q || record.resolved_model == q)
        {
            return Ok(Some(record));
        }
    }
    Ok(None)
}

pub fn selector_preferred_args(backend: &str, query: &str) -> Result<Option<String>, String> {
    let Some(canonical_backend) = normalize_backend(backend) else {
        return Ok(None);
    };
    let q = query.trim();
    if q.is_empty() {
        return Ok(None);
    }
    for record in list_records()? {
        if record.backend == canonical_backend && (record.id == q || record.alias == q) {
            return Ok(record.preferred_args);
        }
    }
    Ok(None)
}

pub fn add_record(input: AddLocalModelInput) -> Result<LocalModelRecord, String> {
    let alias = clean_token("alias", &input.alias)?;
    let Some(backend) = normalize_backend(&input.backend) else {
        return Err("backend must be ollama|llamacpp|mlx".to_string());
    };
    let backend = backend.to_string();
    let resolved_model = input.resolved_model.trim();
    if resolved_model.is_empty() {
        return Err("resolved model cannot be empty".to_string());
    }
    let trust_remote_code = input.trust_remote_code.trim().to_ascii_lowercase();
    if !valid_remote_code(&trust_remote_code) {
        return Err("trust_remote_code must be false|true|unknown".to_string());
    }
    let id = input
        .id
        .as_deref()
        .map(|v| clean_token("id", v))
        .transpose()?
        .unwrap_or_else(|| format!("{backend}:{alias}"));
    let record = LocalModelRecord {
        id,
        alias,
        backend,
        provider: input.provider,
        repo_id: input.repo_id,
        revision: input.revision,
        resolved_model: resolved_model.to_string(),
        local_path: input.local_path,
        quantization: input.quantization,
        format: input.format,
        size_bytes: input.size_bytes,
        cache_path: input.cache_path,
        last_used_at: None,
        last_smoke_status: None,
        preferred_args: input.preferred_args,
        trust_remote_code,
    };

    let mut registry = registry_value()?;
    let models = registry
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "local model registry has invalid models array".to_string())?;
    let existing_idx = models.iter().position(|item| {
        item.get("id").and_then(Value::as_str) == Some(record.id.as_str())
            || (item.get("backend").and_then(Value::as_str) == Some(record.backend.as_str())
                && item.get("alias").and_then(Value::as_str) == Some(record.alias.as_str()))
    });
    if let Some(idx) = existing_idx {
        if !input.replace {
            return Err(format!(
                "local model '{}' already exists; pass --replace to update it",
                record.alias
            ));
        }
        models[idx] = record_to_value(&record);
    } else {
        models.push(record_to_value(&record));
    }
    models.sort_by(|a, b| {
        let ab = a.get("backend").and_then(Value::as_str).unwrap_or("");
        let bb = b.get("backend").and_then(Value::as_str).unwrap_or("");
        let aa = a.get("alias").and_then(Value::as_str).unwrap_or("");
        let ba = b.get("alias").and_then(Value::as_str).unwrap_or("");
        ab.cmp(bb).then(aa.cmp(ba))
    });
    write_registry(&registry)?;
    Ok(record)
}

pub fn remove_record(query: &str) -> Result<Option<LocalModelRecord>, String> {
    let selected = match find_record(query)? {
        Some(record) => record,
        None => return Ok(None),
    };
    let mut registry = registry_value()?;
    let models = registry
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "local model registry has invalid models array".to_string())?;
    let Some(idx) = models
        .iter()
        .position(|item| item.get("id").and_then(Value::as_str) == Some(selected.id.as_str()))
    else {
        return Ok(None);
    };
    let removed = record_from_value(&models.remove(idx))?;
    write_registry(&registry)?;
    Ok(Some(removed))
}

pub fn touch_model_record(
    backend: &str,
    query: &str,
    last_used_at: Option<&str>,
    last_smoke_status: Option<&str>,
) -> Result<Option<LocalModelRecord>, String> {
    let Some(canonical_backend) = normalize_backend(backend) else {
        return Err("backend must be ollama|llamacpp|mlx".to_string());
    };
    let q = query.trim();
    if q.is_empty() {
        return Ok(None);
    }

    let mut registry = registry_value()?;
    let models = registry
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "local model registry has invalid models array".to_string())?;
    let Some(idx) = models.iter().position(|item| {
        item.get("backend").and_then(Value::as_str) == Some(canonical_backend)
            && (item.get("id").and_then(Value::as_str) == Some(q)
                || item.get("alias").and_then(Value::as_str) == Some(q)
                || item.get("resolved_model").and_then(Value::as_str) == Some(q))
    }) else {
        return Ok(None);
    };

    let item = models
        .get_mut(idx)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "local model registry has invalid model record".to_string())?;
    if let Some(ts) = last_used_at {
        item.insert("last_used_at".to_string(), Value::String(ts.to_string()));
    }
    if let Some(status) = last_smoke_status {
        item.insert(
            "last_smoke_status".to_string(),
            Value::String(status.to_string()),
        );
    }
    let record = record_from_value(&Value::Object(item.clone()))?;
    write_registry(&registry)?;
    Ok(Some(record))
}

pub fn registry_json() -> Result<Value, String> {
    registry_value()
}

pub fn record_json(record: &LocalModelRecord) -> Value {
    record_to_value(record)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResolution {
    pub resolved_model: String,
    pub alias: Option<String>,
    pub id: Option<String>,
}

pub fn resolve_model_for_backend(backend: &str, query: &str) -> Result<ModelResolution, String> {
    let Some(normalized_backend) = normalize_backend(backend) else {
        return Err("backend must be ollama|llamacpp|mlx".to_string());
    };
    let q = query.trim();
    if q.is_empty() {
        return Err("model cannot be empty".to_string());
    }

    if let Some(record) = find_record_for_backend(q, normalized_backend)? {
        return Ok(ModelResolution {
            resolved_model: record.resolved_model,
            alias: Some(record.alias),
            id: Some(record.id),
        });
    }

    Ok(ModelResolution {
        resolved_model: q.to_string(),
        alias: None,
        id: None,
    })
}

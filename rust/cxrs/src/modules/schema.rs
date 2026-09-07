use jsonschema::validator_for;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use crate::config::app_config;
use crate::paths::resolve_schema_dir;
use crate::types::{LoadedSchema, SCHEMA_COMPILED_CACHE};
use crate::util::sha256_hex;

#[derive(Debug, Clone)]
pub struct SchemaPromptEnvelope {
    pub full_prompt: String,
    pub prompt_sha256: String,
}

fn normalize_schema_name(name: &str) -> String {
    if name.ends_with(".schema.json") {
        name.to_string()
    } else {
        format!("{name}.schema.json")
    }
}

pub fn load_schema(schema_name: &str) -> Result<LoadedSchema, String> {
    let dir = resolve_schema_dir().ok_or_else(|| "unable to resolve schema dir".to_string())?;
    let name = normalize_schema_name(schema_name);
    let path = dir.join(&name);
    let raw =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid schema JSON {}: {e}", path.display()))?;
    let id = value
        .get("$id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(LoadedSchema {
        name,
        path,
        value,
        id,
    })
}

pub fn list_schemas() -> Result<Vec<LoadedSchema>, String> {
    let dir = resolve_schema_dir().ok_or_else(|| "unable to resolve schema dir".to_string())?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut out: Vec<LoadedSchema> = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("failed to list {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("failed reading schema dir entry: {e}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(fname) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if !fname.ends_with(".schema.json") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let value: Value = serde_json::from_str(&raw)
            .map_err(|e| format!("invalid schema JSON {}: {e}", path.display()))?;
        let id = value
            .get("$id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        out.push(LoadedSchema {
            name: fname.to_string(),
            path,
            value,
            id,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn schema_name_for_tool(tool: &str) -> Option<&'static str> {
    match tool {
        "cxrs_commitjson" | "cxcommitjson" | "commitjson" | "cxrs_commitmsg" | "cxcommitmsg"
        | "commitmsg" => Some("commitjson"),
        "cxrs_diffsum"
        | "cxdiffsum"
        | "diffsum"
        | "cxrs_diffsum_staged"
        | "cxdiffsum_staged"
        | "diffsum-staged" => Some("diffsum"),
        "cxrs_next" | "cxnext" | "next" => Some("next"),
        "cxrs_fix_run" | "cxfix_run" | "fix-run" => Some("fixrun"),
        _ => None,
    }
}

pub fn build_strict_schema_prompt(schema: &str, task_input: &str) -> String {
    if app_config().schema_relaxed {
        return format!(
            "You are a structured output generator.\nReturn JSON ONLY. No markdown. No prose. No code fences.\nOutput MUST be a single valid JSON object matching the schema.\nSchema:\n{schema}\n\nTask input:\n{task_input}\n"
        );
    }
    format!(
        "You are a structured output generator.\nReturn STRICT JSON ONLY. No markdown. No prose. No code fences.\nOutput MUST be a single valid JSON object matching the schema.\nSchema-strict mode: deterministic JSON only; reject ambiguity.\nSchema:\n{schema}\n\nTask input:\n{task_input}\n"
    )
}

pub fn build_schema_prompt_envelope(
    schema: &str,
    task_input: &str,
    retry_reason: Option<&str>,
) -> SchemaPromptEnvelope {
    let mut full_prompt = build_strict_schema_prompt(schema, task_input);
    if let Some(reason) = retry_reason {
        full_prompt.push_str("\n\nThe previous response failed validation with reason: ");
        full_prompt.push_str(reason);
        full_prompt.push_str("\nReturn STRICT JSON only and satisfy the schema exactly.");
    }
    SchemaPromptEnvelope {
        prompt_sha256: sha256_hex(&full_prompt),
        full_prompt,
    }
}

pub fn validate_schema_instance(schema: &LoadedSchema, raw: &str) -> Result<Value, String> {
    let instance: Value = serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;
    // Schema names repeat across compatible registries; cache only validators
    // compiled from the same source so one repository cannot borrow another's rules.
    let cache_key = format!(
        "{}:{}",
        schema.path.display(),
        sha256_hex(&schema.value.to_string())
    );
    let compiled = {
        let mut lock = SCHEMA_COMPILED_CACHE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .map_err(|_| "schema cache poisoned".to_string())?;
        if let Some(existing) = lock.get(&cache_key) {
            existing.clone()
        } else {
            let compiled = validator_for(&schema.value)
                .map_err(|e| format!("failed to compile schema {}: {e}", schema.path.display()))?;
            let compiled = Arc::new(compiled);
            lock.insert(cache_key, compiled.clone());
            compiled
        }
    };
    let reasons: Vec<String> = compiled
        .iter_errors(&instance)
        .take(3)
        .map(|err| err.to_string())
        .collect();
    if !reasons.is_empty() {
        let reason = format!("schema_validation_failed: {}", reasons.join(" | "));
        return Err(reason);
    }
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn schema(path: &str, required: &str) -> LoadedSchema {
        LoadedSchema {
            name: "next.schema.json".to_string(),
            path: PathBuf::from(path),
            value: json!({"type": "object", "required": [required]}),
            id: None,
        }
    }

    #[test]
    fn schema_cache_isolated() {
        let nonce = std::process::id();
        let first = schema(&format!("/tmp/cxrs-schema-{nonce}-first.json"), "first");
        let second = schema(&format!("/tmp/cxrs-schema-{nonce}-second.json"), "second");

        assert!(validate_schema_instance(&first, r#"{"first":true}"#).is_ok());
        assert!(validate_schema_instance(&second, r#"{"first":true}"#).is_err());
    }
}

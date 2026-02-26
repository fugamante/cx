use crate::error::{CxError, CxResult};
use crate::paths::ensure_parent_dir;
use crate::types::ExecutionLog;
use crate::util::{IfEmpty, sha256_hex};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct MigrateSummary {
    pub entries_in: usize,
    pub entries_out: usize,
    pub invalid_json_skipped: usize,
    pub legacy_normalized: usize,
    pub modern_normalized: usize,
}

fn normalize_run_log_row(v: &Value) -> CxResult<(String, bool)> {
    let Some(obj) = v.as_object() else {
        return Err(CxError::invalid("run log row is not an object"));
    };
    let mut has_modern = false;
    let ts = obj
        .get("timestamp")
        .and_then(Value::as_str)
        .or_else(|| obj.get("ts").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    let command = obj
        .get("command")
        .and_then(Value::as_str)
        .or_else(|| obj.get("tool").and_then(Value::as_str))
        .unwrap_or("unknown")
        .to_string();
    let cwd_val = obj
        .get("cwd")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let scope_val = obj.get("scope").and_then(Value::as_str).unwrap_or("repo");
    let repo_root_val = obj.get("repo_root").and_then(Value::as_str).unwrap_or("");
    let backend_used = obj
        .get("backend_used")
        .and_then(Value::as_str)
        .or_else(|| obj.get("llm_backend").and_then(Value::as_str))
        .unwrap_or("codex")
        .to_string();
    if obj.contains_key("execution_id") && obj.contains_key("timestamp") {
        has_modern = true;
    }

    let capture_provider = obj
        .get("capture_provider")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let execution_mode = obj
        .get("execution_mode")
        .and_then(Value::as_str)
        .unwrap_or(if has_modern { "lean" } else { "legacy" })
        .to_string();

    let row = ExecutionLog {
        execution_id: obj
            .get("execution_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
            .if_empty_else(|| {
                format!(
                    "legacy_{}",
                    sha256_hex(&format!("{command}|{ts}|{cwd_val}"))
                )
            }),
        timestamp: ts.clone(),
        ts,
        command: command.clone(),
        tool: command,
        cwd: cwd_val,
        scope: scope_val.to_string(),
        repo_root: repo_root_val.to_string(),
        backend_used: backend_used.clone(),
        llm_backend: backend_used,
        llm_model: obj
            .get("llm_model")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        capture_provider,
        execution_mode,
        duration_ms: obj.get("duration_ms").and_then(Value::as_u64),
        schema_enforced: obj
            .get("schema_enforced")
            .and_then(Value::as_bool)
            .or_else(|| obj.get("schema_ok").and_then(Value::as_bool).map(|_| false))
            .unwrap_or(false),
        schema_name: obj
            .get("schema_name")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        schema_valid: obj
            .get("schema_valid")
            .and_then(Value::as_bool)
            .or_else(|| obj.get("schema_ok").and_then(Value::as_bool))
            .unwrap_or(true),
        schema_ok: obj
            .get("schema_ok")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        schema_reason: obj
            .get("schema_reason")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        quarantine_id: obj
            .get("quarantine_id")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        task_id: obj
            .get("task_id")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        task_parent_id: obj
            .get("task_parent_id")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        input_tokens: obj.get("input_tokens").and_then(Value::as_u64),
        cached_input_tokens: obj.get("cached_input_tokens").and_then(Value::as_u64),
        effective_input_tokens: obj.get("effective_input_tokens").and_then(Value::as_u64),
        output_tokens: obj.get("output_tokens").and_then(Value::as_u64),
        system_output_len_raw: obj.get("system_output_len_raw").and_then(Value::as_u64),
        system_output_len_processed: obj
            .get("system_output_len_processed")
            .and_then(Value::as_u64),
        system_output_len_clipped: obj.get("system_output_len_clipped").and_then(Value::as_u64),
        system_output_lines_raw: obj.get("system_output_lines_raw").and_then(Value::as_u64),
        system_output_lines_processed: obj
            .get("system_output_lines_processed")
            .and_then(Value::as_u64),
        system_output_lines_clipped: obj
            .get("system_output_lines_clipped")
            .and_then(Value::as_u64),
        clipped: obj.get("clipped").and_then(Value::as_bool),
        budget_chars: obj.get("budget_chars").and_then(Value::as_u64),
        budget_lines: obj.get("budget_lines").and_then(Value::as_u64),
        clip_mode: obj
            .get("clip_mode")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        clip_footer: obj.get("clip_footer").and_then(Value::as_bool),
        rtk_used: obj.get("rtk_used").and_then(Value::as_bool),
        prompt_sha256: obj
            .get("prompt_sha256")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        prompt_preview: obj
            .get("prompt_preview")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
        policy_blocked: obj.get("policy_blocked").and_then(Value::as_bool),
        policy_reason: obj
            .get("policy_reason")
            .and_then(Value::as_str)
            .map(|s| s.to_string()),
    };

    let line =
        serde_json::to_string(&row).map_err(|e| CxError::json("serialize normalized row", e))?;
    Ok((line, has_modern))
}

pub fn migrate_runs_jsonl(in_path: &Path, out_path: &Path) -> Result<MigrateSummary, String> {
    migrate_runs_jsonl_cx(in_path, out_path).map_err(|e| e.to_string())
}

fn migrate_runs_jsonl_cx(in_path: &Path, out_path: &Path) -> CxResult<MigrateSummary> {
    let file = File::open(in_path)
        .map_err(|e| CxError::io(format!("cannot open {}", in_path.display()), e))?;
    let reader = BufReader::new(file);
    ensure_parent_dir(out_path).map_err(CxError::invalid)?;
    let tmp = out_path.with_extension("jsonl.tmp");
    let mut out_f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&tmp)
        .map_err(|e| CxError::io(format!("cannot open {}", tmp.display()), e))?;

    let mut summary = MigrateSummary::default();
    for (idx, line_res) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = match line_res {
            Ok(v) => v,
            Err(e) => {
                return Err(CxError::io(
                    format!("read error at line {line_no} in {}", in_path.display()),
                    e,
                ));
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        summary.entries_in += 1;
        let parsed: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                summary.invalid_json_skipped += 1;
                continue;
            }
        };
        let (normalized, is_modern) = normalize_run_log_row(&parsed)?;
        if is_modern {
            summary.modern_normalized += 1;
        } else {
            summary.legacy_normalized += 1;
        }
        out_f
            .write_all(normalized.as_bytes())
            .and_then(|_| out_f.write_all(b"\n"))
            .map_err(|e| CxError::io(format!("failed to write {}", tmp.display()), e))?;
        summary.entries_out += 1;
    }
    out_f
        .flush()
        .map_err(|e| CxError::io(format!("flush failed for {}", tmp.display()), e))?;
    drop(out_f);
    fs::rename(&tmp, out_path).map_err(|e| {
        CxError::io(
            format!("failed to move {} -> {}", tmp.display(), out_path.display()),
            e,
        )
    })?;
    Ok(summary)
}

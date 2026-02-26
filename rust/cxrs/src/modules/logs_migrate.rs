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

fn get_str<'a>(obj: &'a serde_json::Map<String, Value>, keys: &[&str], default: &'a str) -> String {
    for key in keys {
        if let Some(v) = obj.get(*key).and_then(Value::as_str) {
            return v.to_string();
        }
    }
    default.to_string()
}

fn get_opt_str(obj: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(Value::as_str).map(|s| s.to_string())
}

fn get_opt_u64(obj: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
    obj.get(key).and_then(Value::as_u64)
}

fn get_opt_bool(obj: &serde_json::Map<String, Value>, key: &str) -> Option<bool> {
    obj.get(key).and_then(Value::as_bool)
}

fn extract_base_fields(
    obj: &serde_json::Map<String, Value>,
) -> (String, String, String, String, String, bool) {
    let ts = get_str(obj, &["timestamp", "ts"], "");
    let command = get_str(obj, &["command", "tool"], "unknown");
    let cwd_val = get_str(obj, &["cwd"], "");
    let scope_val = get_str(obj, &["scope"], "repo");
    let repo_root_val = get_str(obj, &["repo_root"], "");
    let has_modern = obj.contains_key("execution_id") && obj.contains_key("timestamp");
    (ts, command, cwd_val, scope_val, repo_root_val, has_modern)
}

fn normalize_schema_fields(obj: &serde_json::Map<String, Value>) -> (bool, bool) {
    let schema_valid = obj
        .get("schema_valid")
        .and_then(Value::as_bool)
        .or_else(|| obj.get("schema_ok").and_then(Value::as_bool))
        .unwrap_or(true);
    let schema_enforced = obj
        .get("schema_enforced")
        .and_then(Value::as_bool)
        .or_else(|| obj.get("schema_ok").and_then(Value::as_bool).map(|_| false))
        .unwrap_or(false);
    (schema_enforced, schema_valid)
}

fn normalize_execution_log_row(
    obj: &serde_json::Map<String, Value>,
    ts: String,
    command: String,
    cwd_val: String,
    scope_val: String,
    repo_root_val: String,
    has_modern: bool,
) -> ExecutionLog {
    let backend_used = get_str(obj, &["backend_used", "llm_backend"], "codex");
    let (schema_enforced, schema_valid) = normalize_schema_fields(obj);
    let mut row = ExecutionLog {
        execution_id: get_str(obj, &["execution_id"], "").if_empty_else(|| {
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
        scope: scope_val,
        repo_root: repo_root_val,
        backend_used: backend_used.clone(),
        llm_backend: backend_used,
        execution_mode: get_str(
            obj,
            &["execution_mode"],
            if has_modern { "lean" } else { "legacy" },
        ),
        schema_enforced,
        schema_valid,
        schema_ok: obj
            .get("schema_ok")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        ..Default::default()
    };
    fill_optional_fields(obj, &mut row);
    row
}

fn fill_optional_fields(obj: &serde_json::Map<String, Value>, row: &mut ExecutionLog) {
    row.llm_model = get_opt_str(obj, "llm_model");
    row.capture_provider = get_opt_str(obj, "capture_provider");
    row.duration_ms = get_opt_u64(obj, "duration_ms");
    row.schema_name = get_opt_str(obj, "schema_name");
    row.schema_reason = get_opt_str(obj, "schema_reason");
    row.quarantine_id = get_opt_str(obj, "quarantine_id");
    row.task_id = get_opt_str(obj, "task_id");
    row.task_parent_id = get_opt_str(obj, "task_parent_id");
    row.input_tokens = get_opt_u64(obj, "input_tokens");
    row.cached_input_tokens = get_opt_u64(obj, "cached_input_tokens");
    row.effective_input_tokens = get_opt_u64(obj, "effective_input_tokens");
    row.output_tokens = get_opt_u64(obj, "output_tokens");
    row.system_output_len_raw = get_opt_u64(obj, "system_output_len_raw");
    row.system_output_len_processed = get_opt_u64(obj, "system_output_len_processed");
    row.system_output_len_clipped = get_opt_u64(obj, "system_output_len_clipped");
    row.system_output_lines_raw = get_opt_u64(obj, "system_output_lines_raw");
    row.system_output_lines_processed = get_opt_u64(obj, "system_output_lines_processed");
    row.system_output_lines_clipped = get_opt_u64(obj, "system_output_lines_clipped");
    row.clipped = get_opt_bool(obj, "clipped");
    row.budget_chars = get_opt_u64(obj, "budget_chars");
    row.budget_lines = get_opt_u64(obj, "budget_lines");
    row.clip_mode = get_opt_str(obj, "clip_mode");
    row.clip_footer = get_opt_bool(obj, "clip_footer");
    row.rtk_used = get_opt_bool(obj, "rtk_used");
    row.prompt_sha256 = get_opt_str(obj, "prompt_sha256");
    row.schema_prompt_sha256 = get_opt_str(obj, "schema_prompt_sha256");
    row.schema_sha256 = get_opt_str(obj, "schema_sha256");
    row.schema_attempt = get_opt_u64(obj, "schema_attempt");
    row.timed_out = get_opt_bool(obj, "timed_out");
    row.timeout_secs = get_opt_u64(obj, "timeout_secs");
    row.command_label = get_opt_str(obj, "command_label");
    row.prompt_preview = get_opt_str(obj, "prompt_preview");
    row.policy_blocked = get_opt_bool(obj, "policy_blocked");
    row.policy_reason = get_opt_str(obj, "policy_reason");
}

fn normalize_run_log_row(v: &Value) -> CxResult<(String, bool)> {
    let Some(obj) = v.as_object() else {
        return Err(CxError::invalid("run log row is not an object"));
    };
    let (ts, command, cwd_val, scope_val, repo_root_val, has_modern) = extract_base_fields(obj);
    let row = normalize_execution_log_row(
        obj,
        ts,
        command,
        cwd_val,
        scope_val,
        repo_root_val,
        has_modern,
    );

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
        process_migrate_line(line_res, idx + 1, in_path, &tmp, &mut out_f, &mut summary)?;
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

fn process_migrate_line(
    line_res: Result<String, std::io::Error>,
    line_no: usize,
    in_path: &Path,
    tmp: &Path,
    out_f: &mut File,
    summary: &mut MigrateSummary,
) -> CxResult<()> {
    let line = line_res.map_err(|e| {
        CxError::io(
            format!("read error at line {line_no} in {}", in_path.display()),
            e,
        )
    })?;
    if line.trim().is_empty() {
        return Ok(());
    }
    summary.entries_in += 1;
    let parsed: Value = match serde_json::from_str(&line) {
        Ok(v) => v,
        Err(_) => {
            summary.invalid_json_skipped += 1;
            return Ok(());
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
    Ok(())
}

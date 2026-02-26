use crate::error::{CxError, CxResult};
use crate::paths::ensure_parent_dir;
use crate::types::ExecutionLog;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[path = "logs_cmd.rs"]
mod logs_cmd;
#[path = "logs_migrate.rs"]
mod logs_migrate;
#[path = "logs_read.rs"]
mod logs_read;

pub use logs_cmd::cmd_logs;
pub use logs_migrate::migrate_runs_jsonl;
pub use logs_read::{
    file_len, load_runs, load_runs_appended, load_values, validate_runs_jsonl_file,
};

pub fn validate_execution_log_row(row: &ExecutionLog) -> Result<(), String> {
    if row.execution_id.trim().is_empty() {
        return Err("execution log missing execution_id".to_string());
    }
    if row.timestamp.trim().is_empty() {
        return Err("execution log missing timestamp".to_string());
    }
    if row.command.trim().is_empty() {
        return Err("execution log missing command".to_string());
    }
    if row.backend_used.trim().is_empty() {
        return Err("execution log missing backend_used".to_string());
    }
    if row.execution_mode.trim().is_empty() {
        return Err("execution log missing execution_mode".to_string());
    }
    if row.schema_enforced && !row.schema_ok {
        if row
            .schema_reason
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            return Err("schema failure missing schema_reason".to_string());
        }
        if row
            .quarantine_id
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            return Err("schema failure missing quarantine_id".to_string());
        }
    }
    Ok(())
}

pub fn append_jsonl(path: &Path, value: &Value) -> Result<(), String> {
    append_jsonl_cx(path, value).map_err(|e| e.to_string())
}

fn append_jsonl_cx(path: &Path, value: &Value) -> CxResult<()> {
    ensure_parent_dir(path).map_err(CxError::invalid)?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| CxError::io(format!("failed opening {}", path.display()), e))?;
    let mut line =
        serde_json::to_string(value).map_err(|e| CxError::json("log json serialize", e))?;
    line.push('\n');
    f.write_all(line.as_bytes())
        .map_err(|e| CxError::io(format!("failed writing {}", path.display()), e))
}

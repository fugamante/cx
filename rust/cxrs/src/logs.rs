use crate::error::{CxError, CxResult};
use crate::paths::ensure_parent_dir;
use crate::types::RunEntry;
use fs2::FileExt;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::Path;

pub fn append_jsonl(path: &Path, value: &Value) -> Result<(), String> {
    append_jsonl_cx(path, value).map_err(|e| e.to_string())
}

fn append_jsonl_cx(path: &Path, value: &Value) -> CxResult<()> {
    ensure_parent_dir(path).map_err(|e| CxError::invalid(e))?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| CxError::io(format!("failed opening {}", path.display()), e))?;
    f.lock_exclusive()
        .map_err(|e| CxError::io(format!("failed locking {}", path.display()), e))?;
    let mut line = serde_json::to_string(value).map_err(|e| CxError::json("log json serialize", e))?;
    line.push('\n');
    let write_res = f
        .write_all(line.as_bytes())
        .map_err(|e| CxError::io(format!("failed writing {}", path.display()), e));
    let _ = f.unlock();
    write_res?;
    Ok(())
}

#[derive(Debug, Default, Clone)]
pub struct LogValidateOutcome {
    pub total: usize,
    pub legacy_ok: bool,
    pub legacy_lines: usize,
    pub corrupted_lines: BTreeSet<usize>,
    pub invalid_json_lines: usize,
    pub issues: Vec<String>,
}

pub fn validate_runs_jsonl_file(log_file: &Path, legacy_ok: bool) -> Result<LogValidateOutcome, String> {
    validate_runs_jsonl_file_cx(log_file, legacy_ok).map_err(|e| e.to_string())
}

fn validate_runs_jsonl_file_cx(log_file: &Path, legacy_ok: bool) -> CxResult<LogValidateOutcome> {
    let file = File::open(log_file)
        .map_err(|e| CxError::io(format!("cannot open {}", log_file.display()), e))?;
    let reader = BufReader::new(file);
    let required_strict = [
        "execution_id",
        "timestamp",
        "command",
        "backend_used",
        "capture_provider",
        "execution_mode",
        "duration_ms",
        "schema_enforced",
        "schema_valid",
        "quarantine_id",
        "task_id",
        "system_output_len_raw",
        "system_output_len_processed",
        "system_output_len_clipped",
        "system_output_lines_raw",
        "system_output_lines_processed",
        "system_output_lines_clipped",
        "input_tokens",
        "cached_input_tokens",
        "effective_input_tokens",
        "output_tokens",
        "policy_blocked",
        "policy_reason",
    ];
    let required_legacy_any_of = [("ts", "timestamp"), ("tool", "command"), ("repo_root", "repo_root")];

    let mut out = LogValidateOutcome {
        legacy_ok,
        ..Default::default()
    };
    for (idx, line_res) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = match line_res {
            Ok(v) => v,
            Err(e) => {
                out.corrupted_lines.insert(line_no);
                out.issues.push(format!("line {line_no}: read error: {e}"));
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        out.total += 1;
        let parsed: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                out.corrupted_lines.insert(line_no);
                out.invalid_json_lines += 1;
                let preview: String = line.chars().take(160).collect();
                out.issues.push(
                    CxError::JsonLineParse {
                        file: log_file.to_path_buf(),
                        line: line_no,
                        content_preview: preview,
                        source: e,
                    }
                    .to_string(),
                );
                continue;
            }
        };
        let Some(obj) = parsed.as_object() else {
            out.corrupted_lines.insert(line_no);
            out.issues.push(format!("line {line_no}: json is not an object"));
            continue;
        };
        if legacy_ok {
            let is_modern = obj.contains_key("execution_id") && obj.contains_key("timestamp");
            if is_modern {
                for k in required_strict {
                    if !obj.contains_key(k) {
                        out.corrupted_lines.insert(line_no);
                        out.issues.push(format!("line {line_no}: missing required field '{k}'"));
                    }
                }
            } else {
                let mut ok = true;
                for (legacy_k, modern_k) in required_legacy_any_of {
                    if !(obj.contains_key(legacy_k) || obj.contains_key(modern_k)) {
                        ok = false;
                        out.corrupted_lines.insert(line_no);
                        out.issues.push(format!(
                            "line {line_no}: missing legacy field '{legacy_k}' (or '{modern_k}')"
                        ));
                    }
                }
                if ok {
                    out.legacy_lines += 1;
                }
            }
        } else {
            for k in required_strict {
                if !obj.contains_key(k) {
                    out.corrupted_lines.insert(line_no);
                    out.issues.push(format!("line {line_no}: missing required field '{k}'"));
                }
            }
        }
    }
    Ok(out)
}

pub fn load_runs(log_file: &Path, limit: usize) -> Result<Vec<RunEntry>, String> {
    let file = File::open(log_file).map_err(|e| format!("cannot open {}: {e}", log_file.display()))?;
    let reader = BufReader::new(file);
    let mut out: Vec<RunEntry> = Vec::new();
    for line_res in reader.lines() {
        let Ok(line) = line_res else {
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<RunEntry>(&line) {
            out.push(v);
        }
    }
    if limit > 0 && out.len() > limit {
        out = out[out.len() - limit..].to_vec();
    }
    Ok(out)
}

pub fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

pub fn load_runs_appended(log_file: &Path, offset: u64) -> Result<Vec<RunEntry>, String> {
    let file = File::open(log_file).map_err(|e| format!("cannot open {}: {e}", log_file.display()))?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        let _ = reader.seek(SeekFrom::Start(offset));
    }
    let mut out: Vec<RunEntry> = Vec::new();
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        let s = line.trim_end().to_string();
        line.clear();
        if s.trim().is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<RunEntry>(&s) {
            out.push(v);
        }
    }
    Ok(out)
}

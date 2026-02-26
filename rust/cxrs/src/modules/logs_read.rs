use crate::error::{CxError, CxResult};
use crate::types::RunEntry;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static RUNS_PARSE_WARNED: AtomicBool = AtomicBool::new(false);
const REQUIRED_STRICT_FIELDS: [&str; 26] = [
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
    "timed_out",
    "timeout_secs",
    "command_label",
];
const REQUIRED_LEGACY_ANY_OF: [(&str, &str); 3] = [
    ("ts", "timestamp"),
    ("tool", "command"),
    ("repo_root", "repo_root"),
];

#[derive(Debug, Default, Clone)]
pub struct LogValidateOutcome {
    pub total: usize,
    pub legacy_ok: bool,
    pub legacy_lines: usize,
    pub corrupted_lines: BTreeSet<usize>,
    pub invalid_json_lines: usize,
    pub issues: Vec<String>,
}

pub fn validate_runs_jsonl_file(
    log_file: &Path,
    legacy_ok: bool,
) -> Result<LogValidateOutcome, String> {
    validate_runs_jsonl_file_cx(log_file, legacy_ok).map_err(|e| e.to_string())
}

fn validate_runs_jsonl_file_cx(log_file: &Path, legacy_ok: bool) -> CxResult<LogValidateOutcome> {
    let file = File::open(log_file)
        .map_err(|e| CxError::io(format!("cannot open {}", log_file.display()), e))?;
    let reader = BufReader::new(file);
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
        validate_row_fields(&parsed, line_no, legacy_ok, &mut out);
    }
    Ok(out)
}

fn validate_row_fields(
    parsed: &Value,
    line_no: usize,
    legacy_ok: bool,
    out: &mut LogValidateOutcome,
) {
    let Some(obj) = parsed.as_object() else {
        out.corrupted_lines.insert(line_no);
        out.issues
            .push(format!("line {line_no}: json is not an object"));
        return;
    };
    if legacy_ok {
        validate_legacy_or_modern_row(obj, line_no, out);
    } else {
        validate_required_fields(obj, line_no, out);
    }
}

fn validate_legacy_or_modern_row(
    obj: &serde_json::Map<String, Value>,
    line_no: usize,
    out: &mut LogValidateOutcome,
) {
    let is_modern = obj.contains_key("execution_id") && obj.contains_key("timestamp");
    if is_modern {
        validate_required_fields(obj, line_no, out);
        return;
    }
    let mut legacy_ok = true;
    for (legacy_k, modern_k) in REQUIRED_LEGACY_ANY_OF {
        if !(obj.contains_key(legacy_k) || obj.contains_key(modern_k)) {
            legacy_ok = false;
            out.corrupted_lines.insert(line_no);
            out.issues.push(format!(
                "line {line_no}: missing legacy field '{legacy_k}' (or '{modern_k}')"
            ));
        }
    }
    if legacy_ok {
        out.legacy_lines += 1;
    }
}

fn validate_required_fields(
    obj: &serde_json::Map<String, Value>,
    line_no: usize,
    out: &mut LogValidateOutcome,
) {
    for k in REQUIRED_STRICT_FIELDS {
        if !obj.contains_key(k) {
            out.corrupted_lines.insert(line_no);
            out.issues
                .push(format!("line {line_no}: missing required field '{k}'"));
        }
    }
}

pub fn load_runs(log_file: &Path, limit: usize) -> Result<Vec<RunEntry>, String> {
    load_runs_cx(log_file, limit).map_err(|e| e.to_string())
}

fn load_runs_cx(log_file: &Path, limit: usize) -> CxResult<Vec<RunEntry>> {
    let file = File::open(log_file)
        .map_err(|e| CxError::io(format!("cannot open {}", log_file.display()), e))?;
    let reader = BufReader::new(file);
    let mut out: Vec<RunEntry> = Vec::new();
    let mut invalid = 0usize;
    let mut sample: Option<String> = None;
    for (idx, line_res) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = match line_res {
            Ok(v) => v,
            Err(e) => {
                invalid += 1;
                if sample.is_none() {
                    sample = Some(format!("read error at line {line_no}: {e}"));
                }
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<RunEntry>(&line) {
            Ok(v) => out.push(v),
            Err(e) => {
                invalid += 1;
                if sample.is_none() {
                    let preview: String = line.chars().take(160).collect();
                    sample = Some(
                        CxError::JsonLineParse {
                            file: log_file.to_path_buf(),
                            line: line_no,
                            content_preview: preview,
                            source: e,
                        }
                        .to_string(),
                    );
                }
            }
        }
    }
    maybe_warn_invalid_lines(log_file, invalid, sample);
    if limit > 0 && out.len() > limit {
        out = out[out.len() - limit..].to_vec();
    }
    Ok(out)
}

pub fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

pub fn load_runs_appended(log_file: &Path, offset: u64) -> Result<Vec<RunEntry>, String> {
    load_runs_appended_cx(log_file, offset).map_err(|e| e.to_string())
}

fn load_runs_appended_cx(log_file: &Path, offset: u64) -> CxResult<Vec<RunEntry>> {
    let file = File::open(log_file)
        .map_err(|e| CxError::io(format!("cannot open {}", log_file.display()), e))?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|e| CxError::io(format!("seek failed on {}", log_file.display()), e))?;
    }
    let mut out: Vec<RunEntry> = Vec::new();
    let mut invalid = 0usize;
    let mut sample: Option<String> = None;
    let mut line = String::new();
    let mut line_no = 0usize;
    loop {
        let read_n = reader
            .read_line(&mut line)
            .map_err(|e| CxError::io(format!("read failed on {}", log_file.display()), e))?;
        if read_n == 0 {
            break;
        }
        line_no += 1;
        let s = line.trim_end().to_string();
        line.clear();
        if s.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<RunEntry>(&s) {
            Ok(v) => out.push(v),
            Err(e) => {
                invalid += 1;
                if sample.is_none() {
                    let preview: String = s.chars().take(160).collect();
                    sample = Some(
                        CxError::JsonLineParse {
                            file: log_file.to_path_buf(),
                            line: line_no,
                            content_preview: preview,
                            source: e,
                        }
                        .to_string(),
                    );
                }
            }
        }
    }
    maybe_warn_invalid_lines(log_file, invalid, sample);
    Ok(out)
}

fn maybe_warn_invalid_lines(log_file: &Path, invalid: usize, sample: Option<String>) {
    if invalid == 0 {
        return;
    }
    if RUNS_PARSE_WARNED.swap(true, Ordering::SeqCst) {
        return;
    }
    eprintln!(
        "cxrs: warning: skipped {} invalid JSON lines in {} (sample: {}). Run 'cx logs validate' for details.",
        invalid,
        log_file.display(),
        sample.unwrap_or_else(|| "n/a".to_string())
    );
}

use crate::error::{CxError, CxResult};
use crate::paths::{ensure_parent_dir, resolve_log_file};
use crate::types::{ExecutionLog, RunEntry};
use crate::util::{IfEmpty, sha256_hex};
use fs2::FileExt;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static RUNS_PARSE_WARNED: AtomicBool = AtomicBool::new(false);

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
    let mut invalid = 0usize;
    let mut sample: Option<String> = None;
    for line_res in reader.lines() {
        let Ok(line) = line_res else {
            continue;
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
                    sample = Some(format!("{} (preview='{}')", e, preview));
                }
            }
        }
    }
    if invalid > 0 && !RUNS_PARSE_WARNED.swap(true, Ordering::SeqCst) {
        eprintln!(
            "cxrs: warning: skipped {} invalid JSON lines in {} (sample: {}). Run 'cx logs validate' for details.",
            invalid,
            log_file.display(),
            sample.unwrap_or_else(|| "n/a".to_string())
        );
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
    let mut invalid = 0usize;
    let mut sample: Option<String> = None;
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
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
                    sample = Some(format!("{} (preview='{}')", e, preview));
                }
            }
        }
    }
    if invalid > 0 && !RUNS_PARSE_WARNED.swap(true, Ordering::SeqCst) {
        eprintln!(
            "cxrs: warning: skipped {} invalid JSON lines in {} (sample: {}). Run 'cx logs validate' for details.",
            invalid,
            log_file.display(),
            sample.unwrap_or_else(|| "n/a".to_string())
        );
    }
    Ok(out)
}

#[derive(Debug, Default, Clone)]
pub struct MigrateSummary {
    pub entries_in: usize,
    pub entries_out: usize,
    pub invalid_json_skipped: usize,
    pub legacy_normalized: usize,
    pub modern_normalized: usize,
}

fn normalize_run_log_row(v: &Value) -> Result<(String, bool), String> {
    let Some(obj) = v.as_object() else {
        return Err("row is not an object".to_string());
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
    let cwd_val = obj.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
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
        .or_else(|| obj.get("capture_provider").and_then(Value::as_str))
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
            .if_empty_else(|| format!("legacy_{}", sha256_hex(&format!("{command}|{ts}|{cwd_val}")))),
        timestamp: ts.clone(),
        ts,
        command: command.clone(),
        tool: command,
        cwd: cwd_val.to_string(),
        scope: scope_val.to_string(),
        repo_root: repo_root_val.to_string(),
        backend_used: backend_used.clone(),
        llm_backend: backend_used,
        llm_model: obj.get("llm_model").and_then(Value::as_str).map(|s| s.to_string()),
        capture_provider,
        execution_mode,
        duration_ms: obj.get("duration_ms").and_then(Value::as_u64),
        schema_enforced: obj
            .get("schema_enforced")
            .and_then(Value::as_bool)
            .or_else(|| obj.get("schema_ok").and_then(Value::as_bool).map(|_| false))
            .unwrap_or(false),
        schema_name: obj.get("schema_name").and_then(Value::as_str).map(|s| s.to_string()),
        schema_valid: obj
            .get("schema_valid")
            .and_then(Value::as_bool)
            .or_else(|| obj.get("schema_ok").and_then(Value::as_bool))
            .unwrap_or(true),
        schema_ok: obj.get("schema_ok").and_then(Value::as_bool).unwrap_or(true),
        schema_reason: obj.get("schema_reason").and_then(Value::as_str).map(|s| s.to_string()),
        quarantine_id: obj.get("quarantine_id").and_then(Value::as_str).map(|s| s.to_string()),
        task_id: obj.get("task_id").and_then(Value::as_str).map(|s| s.to_string()),
        task_parent_id: obj.get("task_parent_id").and_then(Value::as_str).map(|s| s.to_string()),
        input_tokens: obj.get("input_tokens").and_then(Value::as_u64),
        cached_input_tokens: obj.get("cached_input_tokens").and_then(Value::as_u64),
        effective_input_tokens: obj.get("effective_input_tokens").and_then(Value::as_u64),
        output_tokens: obj.get("output_tokens").and_then(Value::as_u64),
        system_output_len_raw: obj.get("system_output_len_raw").and_then(Value::as_u64),
        system_output_len_processed: obj.get("system_output_len_processed").and_then(Value::as_u64),
        system_output_len_clipped: obj.get("system_output_len_clipped").and_then(Value::as_u64),
        system_output_lines_raw: obj.get("system_output_lines_raw").and_then(Value::as_u64),
        system_output_lines_processed: obj.get("system_output_lines_processed").and_then(Value::as_u64),
        system_output_lines_clipped: obj.get("system_output_lines_clipped").and_then(Value::as_u64),
        clipped: obj.get("clipped").and_then(Value::as_bool),
        budget_chars: obj.get("budget_chars").and_then(Value::as_u64),
        budget_lines: obj.get("budget_lines").and_then(Value::as_u64),
        clip_mode: obj.get("clip_mode").and_then(Value::as_str).map(|s| s.to_string()),
        clip_footer: obj.get("clip_footer").and_then(Value::as_bool),
        rtk_used: obj.get("rtk_used").and_then(Value::as_bool),
        prompt_sha256: obj.get("prompt_sha256").and_then(Value::as_str).map(|s| s.to_string()),
        prompt_preview: obj.get("prompt_preview").and_then(Value::as_str).map(|s| s.to_string()),
        policy_blocked: obj.get("policy_blocked").and_then(Value::as_bool),
        policy_reason: obj.get("policy_reason").and_then(Value::as_str).map(|s| s.to_string()),
    };

    let line = serde_json::to_string(&row).map_err(|e| format!("failed to serialize normalized row: {e}"))?;
    Ok((line, has_modern))
}

pub fn migrate_runs_jsonl(in_path: &Path, out_path: &Path) -> Result<MigrateSummary, String> {
    let file = File::open(in_path).map_err(|e| format!("cannot open {}: {e}", in_path.display()))?;
    let reader = BufReader::new(file);
    ensure_parent_dir(out_path)?;
    let mut out_f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(out_path)
        .map_err(|e| format!("cannot open {}: {e}", out_path.display()))?;

    let mut summary = MigrateSummary::default();
    for line_res in reader.lines() {
        let Ok(line) = line_res else { continue };
        if line.trim().is_empty() { continue; }
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
            .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
        summary.entries_out += 1;
    }
    Ok(summary)
}

pub fn cmd_logs(app_name: &str, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("validate") {
        "validate" => {
            let legacy_ok = args.iter().any(|a| a == "--legacy-ok");
            let Some(log_file) = resolve_log_file() else {
                eprintln!("{app_name} logs validate: unable to resolve log file");
                return 1;
            };
            if !log_file.exists() {
                println!("{app_name} logs validate: no log file at {}", log_file.display());
                return 0;
            }
            let outcome = match validate_runs_jsonl_file(&log_file, legacy_ok) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{app_name} logs validate: {e}");
                    return 1;
                }
            };
            println!("== {app_name} logs validate ==");
            println!("log_file: {}", log_file.display());
            println!("entries_scanned: {}", outcome.total);
            if outcome.legacy_ok {
                println!("legacy_ok: true");
                println!("legacy_entries: {}", outcome.legacy_lines);
            } else {
                println!("legacy_ok: false");
            }
            println!("corrupted_entries: {}", outcome.corrupted_lines.len());
            println!("issue_count: {}", outcome.issues.len());
            println!("invalid_json_entries: {}", outcome.invalid_json_lines);
            if !outcome.issues.is_empty() {
                for issue in outcome.issues.iter().take(20) {
                    println!("- {issue}");
                }
                if outcome.issues.len() > 20 {
                    println!("- ... and {} more", outcome.issues.len() - 20);
                }
                if outcome.legacy_ok && outcome.invalid_json_lines == 0 {
                    println!("status: ok_with_warnings");
                    return 0;
                }
                return 1;
            }
            println!("status: ok");
            0
        }
        "migrate" => {
            let Some(log_file) = resolve_log_file() else {
                eprintln!("{app_name} logs migrate: unable to resolve log file");
                return 1;
            };
            if !log_file.exists() {
                eprintln!("{app_name} logs migrate: no log file at {}", log_file.display());
                return 1;
            }

            let mut out_path: Option<std::path::PathBuf> = None;
            let mut in_place = false;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--out" => {
                        let Some(v) = args.get(i + 1) else {
                            eprintln!("Usage: {app_name} logs migrate [--out PATH] [--in-place]");
                            return 2;
                        };
                        out_path = Some(std::path::PathBuf::from(v));
                        i += 2;
                    }
                    "--in-place" => {
                        in_place = true;
                        i += 1;
                    }
                    other => {
                        eprintln!("{app_name} logs migrate: unknown flag '{other}'");
                        eprintln!("Usage: {app_name} logs migrate [--out PATH] [--in-place]");
                        return 2;
                    }
                }
            }

            let target = out_path.unwrap_or_else(|| {
                log_file
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("runs.migrated.jsonl")
            });

            println!("== {app_name} logs migrate ==");
            println!("in: {}", log_file.display());
            println!("out: {}", target.display());
            match migrate_runs_jsonl(&log_file, &target) {
                Ok(summary) => {
                    println!("entries_in: {}", summary.entries_in);
                    println!("entries_out: {}", summary.entries_out);
                    println!("invalid_json_skipped: {}", summary.invalid_json_skipped);
                    println!("legacy_normalized: {}", summary.legacy_normalized);
                    println!("modern_normalized: {}", summary.modern_normalized);

                    if in_place {
                        let bak = log_file.with_extension(format!(
                            "jsonl.bak.{}",
                            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
                        ));
                        if let Err(e) = fs::copy(&log_file, &bak) {
                            eprintln!(
                                "{app_name} logs migrate: failed to backup {} -> {}: {e}",
                                log_file.display(),
                                bak.display()
                            );
                            return 1;
                        }
                        if let Err(e) = fs::rename(&target, &log_file) {
                            eprintln!(
                                "{app_name} logs migrate: failed to replace {} with {}: {e}",
                                log_file.display(),
                                target.display()
                            );
                            eprintln!("backup: {}", bak.display());
                            return 1;
                        }
                        println!("backup: {}", bak.display());
                        println!("status: replaced");
                    } else {
                        println!("status: wrote");
                    }
                    0
                }
                Err(e) => {
                    eprintln!("{app_name} logs migrate: {e}");
                    1
                }
            }
        }
        other => {
            eprintln!("Usage: {app_name} logs <validate|migrate> (unknown subcommand: {other})");
            2
        }
    }
}

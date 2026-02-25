use chrono::Utc;
use jsonschema::JSONSchema;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::types::{
    CaptureStats, ExecutionLog, ExecutionResult, LlmOutputKind, LoadedSchema, QuarantineAttempt,
    QuarantineRecord, RunEntry, TaskInput, TaskRecord, TaskSpec, UsageStats, SCHEMA_COMPILED_CACHE,
};
use crate::paths::{
    repo_root, resolve_log_file, resolve_quarantine_dir, resolve_schema_dir, resolve_schema_fail_log_file,
    resolve_state_file, resolve_tasks_file, repo_root_hint,
};
use crate::logs::{
    append_jsonl, cmd_logs, file_len, load_runs, load_runs_appended, validate_runs_jsonl_file,
};
use crate::state::{ensure_state_value, read_state_value, state_cache_clear, write_json_atomic};
use crate::util::sha256_hex;

const APP_NAME: &str = "cxrs";
const APP_DESC: &str = "Rust spike for the cx toolchain";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
static RTK_WARNED_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
enum TaskRunError {
    Critical(String),
}

impl std::fmt::Display for TaskRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskRunError::Critical(s) => write!(f, "{s}"),
        }
    }
}

fn print_help() {
    println!("{APP_NAME} - {APP_DESC}");
    println!();
    println!("Usage:");
    println!("  {APP_NAME} <command> [args]");
    println!();
    println!("Commands:");
    println!("  version            Print tool version");
    println!("  where              Show binary/source/log resolution details");
    println!("  routes [--json] [cmd...]  Show routing map/introspection");
    println!("  diag               Non-interactive diagnostic report");
    println!("  parity             Run Rust/Bash parity invariants");
    println!("  schema list [--json]  List registered schemas");
    println!("  logs validate [--fix=false] [--legacy-ok]  Validate execution log JSONL contract");
    println!("  logs migrate [--out PATH] [--in-place]  Normalize legacy run logs to current contract");
    println!("  ci validate [--strict] [--legacy-ok] [--json]  CI-friendly validation gate (no network)");
    println!("  core               Show execution-core pipeline config");
    println!(
        "  task <op> [...]    Task graph management (add/list/claim/complete/fail/show/fanout)"
    );
    println!("  doctor             Run non-interactive environment checks");
    println!("  supports <name>    Exit 0 if subcommand is supported by cxrs");
    println!(
        "  llm <op> [...]     Manage LLM backend/model defaults (show|use|unset|set-backend|set-model|clear-model)"
    );
    println!("  state <op> [...]   Manage repo state JSON (show|get|set)");
    println!("  policy [show|check ...] Show safety rules or classify a command");
    println!("  bench <N> -- <cmd...>  Benchmark command runtime and tokens");
    println!("  cx <cmd...>        Run command output through LLM text mode");
    println!("  cxj <cmd...>       Run command output through LLM JSONL mode");
    println!("  cxo <cmd...>       Run command output and print last agent message");
    println!("  cxol <cmd...>      Run command output through LLM plain mode");
    println!("  cxcopy <cmd...>    Copy cxo output to clipboard (pbcopy)");
    println!("  fix <cmd...>       Explain failures and suggest next steps (text)");
    println!("  budget             Show context budget settings and last clip fields");
    println!("  log-tail [N]       Pretty-print last N log entries");
    println!("  health             Run end-to-end selected-LLM/cx smoke checks");
    println!("  rtk-status         Show RTK version/range decision and fallback mode");
    println!("  log-on             Enable cx logging (process-local)");
    println!("  log-off            Disable cx logging in this process");
    println!("  alert-show         Show active alert thresholds/toggles");
    println!("  alert-on           Enable alerts (process-local)");
    println!("  alert-off          Disable alerts in this process");
    println!("  chunk              Chunk stdin text by context budget chars");
    println!("  metrics [N]        Token and duration aggregates from last N runs");
    println!("  prompt <mode> <request>  Generate Codex-ready prompt block");
    println!("  roles [role]       List roles or print role-specific prompt header");
    println!("  fanout <objective> Generate role-tagged parallelizable subtasks");
    println!("  promptlint [N]     Lint prompt/cost patterns from last N runs");
    println!("  cx-compat <cmd...> Compatibility shim for bash-style cx command names");
    println!("  profile [N]        Summarize last N runs from resolved cx log (default 50)");
    println!("  alert [N]          Report anomalies from last N runs (default 50)");
    println!("  optimize [N] [--json]  Recommend cost/latency improvements from last N runs");
    println!("  worklog [N]        Emit Markdown worklog from last N runs (default 50)");
    println!("  trace [N]          Show Nth most-recent run from resolved cx log (default 1)");
    println!("  next <cmd...>      Suggest next shell commands from command output (strict JSON)");
    println!("  diffsum            Summarize unstaged diff (strict schema)");
    println!("  diffsum-staged     Summarize staged diff (strict schema)");
    println!("  fix-run <cmd...>   Suggest remediation commands for a failed command");
    println!("  commitjson         Generate strict JSON commit object from staged diff");
    println!("  commitmsg          Generate commit message text from staged diff");
    println!("  replay <id>        Replay quarantined schema run in strict mode");
    println!("  quarantine list [N]  Show recent quarantine entries (default 20)");
    println!("  quarantine show <id> Show quarantined entry payload");
    println!("  help               Print this help");
}

fn print_task_help() {
    println!("cx help task");
    println!();
    println!("Task commands:");
    println!("  cx task add \"<objective>\" --role <architect|implementer|reviewer|tester|doc>");
    println!("  cx task list [--status pending|in_progress|complete|failed]");
    println!("  cx task claim <id>");
    println!("  cx task complete <id>");
    println!("  cx task fail <id>");
    println!("  cx task show <id>");
    println!("  cx task fanout \"<objective>\" [--from staged-diff|worktree|log|file:PATH]");
    println!("  cx task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]");
    println!("  cx task run-all [--status pending]");
}

// path resolution moved to `paths.rs`

fn normalize_schema_name(name: &str) -> String {
    if name.ends_with(".schema.json") {
        name.to_string()
    } else {
        format!("{name}.schema.json")
    }
}

fn load_schema(schema_name: &str) -> Result<LoadedSchema, String> {
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

fn list_schemas() -> Result<Vec<LoadedSchema>, String> {
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

fn schema_name_for_tool(tool: &str) -> Option<&'static str> {
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

// ensure_parent_dir moved to `paths.rs`

fn validate_execution_log_row(row: &ExecutionLog) -> Result<(), String> {
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
        if row.schema_reason.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
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

fn prompt_preview(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn usage_from_jsonl(jsonl: &str) -> UsageStats {
    let mut out = UsageStats::default();
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("turn.completed") {
            continue;
        }
        let usage = v.get("usage").cloned().unwrap_or(Value::Null);
        out.input_tokens = usage.get("input_tokens").and_then(Value::as_u64);
        out.cached_input_tokens = usage.get("cached_input_tokens").and_then(Value::as_u64);
        out.output_tokens = usage.get("output_tokens").and_then(Value::as_u64);
    }
    out
}

fn effective_input_tokens(input: Option<u64>, cached: Option<u64>) -> Option<u64> {
    match (input, cached) {
        (Some(i), Some(c)) => Some(i.saturating_sub(c)),
        (Some(i), None) => Some(i),
        _ => None,
    }
}

fn llm_backend() -> String {
    let raw = env::var("CX_LLM_BACKEND")
        .ok()
        .or_else(|| {
            read_state_value().and_then(|v| {
                value_at_path(&v, "preferences.llm_backend")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string())
            })
        })
        .unwrap_or_else(|| "codex".to_string());
    match raw.to_lowercase().as_str() {
        "ollama" => "ollama".to_string(),
        _ => "codex".to_string(),
    }
}

fn llm_model() -> String {
    if llm_backend() != "ollama" {
        return env::var("CX_MODEL").unwrap_or_default();
    }
    if let Ok(v) = env::var("CX_OLLAMA_MODEL") {
        let t = v.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    read_state_value()
        .and_then(|v| {
            value_at_path(&v, "preferences.ollama_model")
                .and_then(Value::as_str)
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

fn logging_enabled() -> bool {
    env::var("CXLOG_ENABLED")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1
}

fn ollama_model_preference() -> String {
    if let Ok(v) = env::var("CX_OLLAMA_MODEL") {
        let t = v.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    read_state_value()
        .and_then(|v| {
            value_at_path(&v, "preferences.ollama_model")
                .and_then(Value::as_str)
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

fn set_state_path(path: &str, value: Value) -> Result<(), String> {
    let (state_file, mut state) = ensure_state_value()?;
    set_value_at_path(&mut state, path, value)?;
    write_json_atomic(&state_file, &state)
}

fn is_interactive_tty() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

fn ollama_list_models() -> Vec<String> {
    let output = match Command::new("ollama").arg("list").output() {
        Ok(v) if v.status.success() => v,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut out: Vec<String> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 && line.to_lowercase().contains("name") {
            continue;
        }
        let name = line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn resolve_ollama_model_for_run() -> Result<String, String> {
    let model = llm_model();
    if !model.trim().is_empty() {
        return Ok(model);
    }
    if !is_interactive_tty() {
        return Err(
            "ollama model is unset; set CX_OLLAMA_MODEL or run 'cxrs llm set-model <model>'"
                .to_string(),
        );
    }

    let models = ollama_list_models();
    eprintln!("cxrs: no default Ollama model configured.");
    if models.is_empty() {
        eprintln!("No local models found from 'ollama list'.");
        eprintln!("Pull one first (example: ollama pull llama3.1) then set it.");
        return Err("ollama model selection aborted".to_string());
    }
    eprintln!("Select a default model (persisted to .codex/state.json):");
    for (idx, m) in models.iter().enumerate() {
        eprintln!("  {}. {}", idx + 1, m);
    }
    eprint!("Enter number or model name: ");
    let _ = io::stderr().flush();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("failed reading selection: {e}"))?;
    let selected_raw = input.trim();
    if selected_raw.is_empty() {
        return Err("no model selected".to_string());
    }
    let selected = if let Ok(n) = selected_raw.parse::<usize>() {
        models
            .get(n.saturating_sub(1))
            .cloned()
            .ok_or_else(|| "invalid model index".to_string())?
    } else {
        selected_raw.to_string()
    };
    set_state_path("preferences.ollama_model", Value::String(selected.clone()))?;
    eprintln!("cxrs: default Ollama model set to '{}'.", selected);
    Ok(selected)
}

fn llm_bin_name() -> &'static str {
    if llm_backend() == "ollama" {
        "ollama"
    } else {
        "codex"
    }
}

// repo_root_hint moved to `paths.rs`

fn toolchain_version_string() -> String {
    let mut base = APP_VERSION.to_string();
    if let Some(root) = repo_root_hint() {
        let version_file = root.join("VERSION");
        if let Ok(text) = fs::read_to_string(&version_file) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                base = trimmed.to_string();
            }
        }
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["rev-parse", "--short", "HEAD"])
            .output()
        {
            if out.status.success() {
                let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !sha.is_empty() {
                    return format!("{base}+{sha}");
                }
            }
        }
    }
    base
}

fn make_execution_id(tool: &str) -> String {
    format!(
        "{}_{}_{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        tool.replace(
            |c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-',
            "_"
        ),
        std::process::id()
    )
}

fn is_schema_tool(tool: &str) -> bool {
    matches!(
        tool,
        "cxcommitjson"
            | "cxcommitmsg"
            | "cxdiffsum"
            | "cxdiffsum_staged"
            | "cxnext"
            | "cxfix_run"
            | "cxrs_commitjson"
            | "cxrs_diffsum"
            | "cxrs_diffsum_staged"
            | "cxrs_next"
            | "cxrs_fix_run"
            | "commitjson"
            | "commitmsg"
            | "diffsum"
            | "diffsum-staged"
            | "next"
            | "fix-run"
    )
}

fn log_codex_run(
    tool: &str,
    prompt: &str,
    duration_ms: u64,
    usage: Option<&UsageStats>,
    capture: Option<&CaptureStats>,
    schema_ok: bool,
    schema_reason: Option<&str>,
    schema_name: Option<&str>,
    quarantine_id: Option<&str>,
    policy_blocked: Option<bool>,
    policy_reason: Option<&str>,
) -> Result<(), String> {
    let run_log = resolve_log_file().ok_or_else(|| "unable to resolve run log file".to_string())?;
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let root = repo_root()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let scope = if root.is_empty() { "global" } else { "repo" };

    let input = usage.and_then(|u| u.input_tokens);
    let cached = usage.and_then(|u| u.cached_input_tokens);
    let output = usage.and_then(|u| u.output_tokens);
    let effective = effective_input_tokens(input, cached);
    let cap = capture.cloned().unwrap_or_default();
    let backend = llm_backend();
    let model = llm_model();
    let mode = env::var("CX_MODE").unwrap_or_else(|_| "lean".to_string());
    let exec_id = make_execution_id(tool);
    let schema_enforced = is_schema_tool(tool);
    let task_id = current_task_id().unwrap_or_default();
    let task_parent_id = current_task_parent_id().unwrap_or_default();

    let ts = utc_now_iso();
    let row = ExecutionLog {
        execution_id: exec_id,
        timestamp: ts.clone(),
        ts,
        command: tool.to_string(),
        tool: tool.to_string(),
        cwd,
        scope: scope.to_string(),
        repo_root: root,
        backend_used: backend.clone(),
        llm_backend: backend,
        llm_model: if model.is_empty() { None } else { Some(model) },
        capture_provider: cap.capture_provider.clone(),
        execution_mode: mode,
        duration_ms: Some(duration_ms),
        schema_enforced,
        schema_name: schema_name.map(|s| s.to_string()),
        schema_valid: schema_ok,
        schema_ok,
        schema_reason: schema_reason.map(|s| s.to_string()),
        quarantine_id: quarantine_id.map(|s| s.to_string()),
        task_id: if task_id.is_empty() {
            None
        } else {
            Some(task_id)
        },
        task_parent_id: if task_parent_id.is_empty() {
            None
        } else {
            Some(task_parent_id)
        },
        input_tokens: input,
        cached_input_tokens: cached,
        effective_input_tokens: effective,
        output_tokens: output,
        system_output_len_raw: cap.system_output_len_raw,
        system_output_len_processed: cap.system_output_len_processed,
        system_output_len_clipped: cap.system_output_len_clipped,
        system_output_lines_raw: cap.system_output_lines_raw,
        system_output_lines_processed: cap.system_output_lines_processed,
        system_output_lines_clipped: cap.system_output_lines_clipped,
        clipped: cap.clipped,
        budget_chars: cap.budget_chars,
        budget_lines: cap.budget_lines,
        clip_mode: cap.clip_mode,
        clip_footer: cap.clip_footer,
        rtk_used: cap.rtk_used,
        prompt_sha256: Some(sha256_hex(prompt)),
        prompt_preview: Some(prompt_preview(prompt, 180)),
        policy_blocked,
        policy_reason: policy_reason.map(|s| s.to_string()),
    };
    validate_execution_log_row(&row)?;
    let value = serde_json::to_value(row).map_err(|e| format!("failed serialize run log: {e}"))?;
    append_jsonl(&run_log, &value)
}

fn utc_now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn make_quarantine_id(tool: &str) -> String {
    let safe_tool: String = tool
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "{}_{}_{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        safe_tool,
        std::process::id()
    )
}

fn quarantine_store_with_attempts(
    tool: &str,
    reason: &str,
    raw: &str,
    schema: &str,
    prompt: &str,
    attempts: Vec<QuarantineAttempt>,
) -> Result<String, String> {
    let Some(qdir) = resolve_quarantine_dir() else {
        return Err("unable to resolve quarantine directory".to_string());
    };
    fs::create_dir_all(&qdir).map_err(|e| format!("failed to create {}: {e}", qdir.display()))?;

    let id = make_quarantine_id(tool);
    let rec = QuarantineRecord {
        id: id.clone(),
        ts: utc_now_iso(),
        tool: tool.to_string(),
        reason: reason.to_string(),
        schema: schema.to_string(),
        prompt: prompt.to_string(),
        prompt_sha256: sha256_hex(prompt),
        raw_response: raw.to_string(),
        raw_sha256: sha256_hex(raw),
        attempts,
    };
    let file = qdir.join(format!("{id}.json"));
    let serialized = serde_json::to_string_pretty(&rec)
        .map_err(|e| format!("failed to serialize quarantine record: {e}"))?;
    fs::write(&file, serialized).map_err(|e| format!("failed to write {}: {e}", file.display()))?;
    Ok(id)
}

#[allow(dead_code)]
fn quarantine_store(
    tool: &str,
    reason: &str,
    raw: &str,
    schema: &str,
    prompt: &str,
) -> Result<String, String> {
    quarantine_store_with_attempts(tool, reason, raw, schema, prompt, Vec::new())
}

fn log_schema_failure(
    tool: &str,
    reason: &str,
    raw: &str,
    schema: &str,
    prompt: &str,
    attempts: Vec<QuarantineAttempt>,
) -> Result<String, String> {
    let qid = quarantine_store_with_attempts(tool, reason, raw, schema, prompt, attempts)?;

    let schema_fail_log = resolve_schema_fail_log_file()
        .ok_or_else(|| "unable to resolve schema_failures log file".to_string())?;
    let failure_row = json!({
        "ts": utc_now_iso(),
        "tool": tool,
        "reason": reason,
        "quarantine_id": qid,
        "raw_sha256": sha256_hex(raw)
    });
    append_jsonl(&schema_fail_log, &failure_row)?;

    let run_log = resolve_log_file().ok_or_else(|| "unable to resolve run log file".to_string())?;
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let root = repo_root()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let scope = if root.is_empty() { "global" } else { "repo" };

    let ts = utc_now_iso();
    let task_id = current_task_id();
    let task_parent_id = current_task_parent_id();
    let backend = llm_backend();
    let run_failure = ExecutionLog {
        execution_id: make_execution_id(tool),
        timestamp: ts.clone(),
        ts,
        command: tool.to_string(),
        tool: tool.to_string(),
        cwd,
        scope: scope.to_string(),
        repo_root: root,
        backend_used: backend.clone(),
        llm_backend: backend,
        llm_model: {
            let m = llm_model();
            if m.is_empty() { None } else { Some(m) }
        },
        capture_provider: None,
        execution_mode: env::var("CX_MODE").unwrap_or_else(|_| "lean".to_string()),
        duration_ms: None,
        schema_enforced: true,
        schema_name: schema_name_for_tool(tool).map(|s| s.to_string()),
        schema_valid: false,
        schema_ok: false,
        schema_reason: Some(reason.to_string()),
        quarantine_id: Some(qid.clone()),
        task_id,
        task_parent_id,
        input_tokens: None,
        cached_input_tokens: None,
        effective_input_tokens: None,
        output_tokens: None,
        system_output_len_raw: None,
        system_output_len_processed: None,
        system_output_len_clipped: None,
        system_output_lines_raw: None,
        system_output_lines_processed: None,
        system_output_lines_clipped: None,
        clipped: None,
        budget_chars: None,
        budget_lines: None,
        clip_mode: None,
        clip_footer: None,
        rtk_used: None,
        prompt_sha256: None,
        prompt_preview: None,
        policy_blocked: None,
        policy_reason: None,
    };
    validate_execution_log_row(&run_failure)
        .map_err(|e| format!("schema failure log invalid: {e}"))?;
    let failure_value =
        serde_json::to_value(run_failure).map_err(|e| format!("failed serialize run log: {e}"))?;
    append_jsonl(&run_log, &failure_value)?;

    Ok(qid)
}

fn quarantine_file_by_id(id: &str) -> Option<PathBuf> {
    let qdir = resolve_quarantine_dir()?;
    let path = qdir.join(format!("{id}.json"));
    if path.exists() { Some(path) } else { None }
}

fn read_quarantine_record(id: &str) -> Result<QuarantineRecord, String> {
    let path = quarantine_file_by_id(id).ok_or_else(|| format!("quarantine id not found: {id}"))?;
    let mut s = String::new();
    File::open(&path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?
        .read_to_string(&mut s)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("invalid quarantine JSON {}: {e}", path.display()))
}

// state read/write moved to `state.rs`

fn parse_cli_value(raw: &str) -> Value {
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

fn value_at_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.split('.') {
        if seg.is_empty() {
            continue;
        }
        cur = cur.get(seg)?;
    }
    Some(cur)
}

fn value_to_display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

fn set_value_at_path(root: &mut Value, path: &str, new_value: Value) -> Result<(), String> {
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

fn current_task_id() -> Option<String> {
    if let Ok(v) = env::var("CX_TASK_ID") {
        if !v.trim().is_empty() {
            return Some(v);
        }
    }
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "runtime.current_task_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn current_task_parent_id() -> Option<String> {
    if let Ok(v) = env::var("CX_TASK_PARENT_ID") {
        if !v.trim().is_empty() {
            return Some(v);
        }
    }
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "runtime.current_task_parent_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn task_role_valid(role: &str) -> bool {
    matches!(
        role,
        "architect" | "implementer" | "reviewer" | "tester" | "doc"
    )
}

fn read_tasks() -> Result<Vec<TaskRecord>, String> {
    let path = resolve_tasks_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut s = String::new();
    File::open(&path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?
        .read_to_string(&mut s)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if s.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<TaskRecord>>(&s)
        .map_err(|e| format!("invalid JSON in {}: {e}", path.display()))
}

fn write_tasks(tasks: &[TaskRecord]) -> Result<(), String> {
    let path = resolve_tasks_file()?;
    let value = serde_json::to_value(tasks).map_err(|e| format!("failed to encode tasks: {e}"))?;
    write_json_atomic(&path, &value)
}

fn next_task_id(tasks: &[TaskRecord]) -> String {
    let mut max_id = 0u64;
    for t in tasks {
        if let Some(num) =
            t.id.strip_prefix("task_")
                .and_then(|v| v.parse::<u64>().ok())
        {
            if num > max_id {
                max_id = num;
            }
        }
    }
    format!("task_{:03}", max_id + 1)
}

fn cmd_task_add(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!(
            "Usage: {APP_NAME} task add <objective> [--role <role>] [--parent <id>] [--context <ref>]"
        );
        return 2;
    }
    let mut obj_parts: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        if args[i].starts_with("--") {
            break;
        }
        obj_parts.push(args[i].clone());
        i += 1;
    }
    let objective = obj_parts.join(" ").trim().to_string();
    if objective.is_empty() {
        eprintln!("cxrs task add: objective cannot be empty");
        return 2;
    }
    let mut role = "implementer".to_string();
    let mut parent_id: Option<String> = None;
    let mut context_ref = String::new();
    let mut i = i;
    while i < args.len() {
        match args[i].as_str() {
            "--role" => {
                let Some(v) = args.get(i + 1) else {
                    eprintln!("cxrs task add: --role requires a value");
                    return 2;
                };
                role = v.to_lowercase();
                i += 2;
            }
            "--parent" => {
                let Some(v) = args.get(i + 1) else {
                    eprintln!("cxrs task add: --parent requires a value");
                    return 2;
                };
                parent_id = Some(v.to_string());
                i += 2;
            }
            "--context" => {
                let Some(v) = args.get(i + 1) else {
                    eprintln!("cxrs task add: --context requires a value");
                    return 2;
                };
                context_ref = v.to_string();
                i += 2;
            }
            other => {
                eprintln!("cxrs task add: unknown flag '{other}'");
                return 2;
            }
        }
    }
    if !task_role_valid(&role) {
        eprintln!("cxrs task add: invalid role '{role}'");
        return 2;
    }

    let mut tasks = match read_tasks() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let id = next_task_id(&tasks);
    let now = utc_now_iso();
    tasks.push(TaskRecord {
        id: id.clone(),
        parent_id,
        role,
        objective,
        context_ref,
        status: "pending".to_string(),
        created_at: now.clone(),
        updated_at: now,
    });
    if let Err(e) = write_tasks(&tasks) {
        eprintln!("cxrs task add: {e}");
        return 1;
    }
    println!("{id}");
    0
}

fn cmd_task_list(status_filter: Option<&str>) -> i32 {
    let tasks = match read_tasks() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let filtered: Vec<TaskRecord> = match status_filter {
        Some(s) => tasks.into_iter().filter(|t| t.status == s).collect(),
        None => tasks,
    };
    if filtered.is_empty() {
        println!("No tasks.");
        return 0;
    }
    println!("id | role | status | parent_id | objective");
    println!("---|---|---|---|---");
    for t in filtered {
        println!(
            "{} | {} | {} | {} | {}",
            t.id,
            t.role,
            t.status,
            t.parent_id.unwrap_or_else(|| "-".to_string()),
            t.objective
        );
    }
    0
}

fn cmd_task_show(id: &str) -> i32 {
    let tasks = match read_tasks() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let Some(task) = tasks.into_iter().find(|t| t.id == id) else {
        eprintln!("cxrs task show: task not found: {id}");
        return 1;
    };
    match serde_json::to_string_pretty(&task) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            eprintln!("cxrs task show: render failed: {e}");
            1
        }
    }
}

fn parse_words(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn run_task_objective(task: &TaskRecord) -> Result<(i32, Option<String>), String> {
    let words = parse_words(&task.objective);
    if let Some(cmd0) = words.first().map(String::as_str) {
        let args: Vec<String> = words.iter().skip(1).cloned().collect();
        let status = match cmd0 {
            "cxcommitjson" | "commitjson" => cmd_commitjson(),
            "cxcommitmsg" | "commitmsg" => cmd_commitmsg(),
            "cxdiffsum" | "diffsum" => cmd_diffsum(false),
            "cxdiffsum_staged" | "diffsum-staged" => cmd_diffsum(true),
            "cxnext" | "next" => {
                if args.is_empty() {
                    2
                } else {
                    cmd_next(&args)
                }
            }
            "cxfix_run" | "fix-run" => {
                if args.is_empty() {
                    2
                } else {
                    cmd_fix_run(&args)
                }
            }
            "cxfix" | "fix" => {
                if args.is_empty() {
                    2
                } else {
                    cmd_fix(&args)
                }
            }
            "cx" => {
                if args.is_empty() {
                    2
                } else {
                    cmd_cx(&args)
                }
            }
            "cxj" => {
                if args.is_empty() {
                    2
                } else {
                    cmd_cxj(&args)
                }
            }
            "cxo" => {
                if args.is_empty() {
                    2
                } else {
                    cmd_cxo(&args)
                }
            }
            _ => {
                let prompt = if task.context_ref.trim().is_empty() {
                    format!(
                        "Task Objective:\n{}\n\nRespond with concise execution notes and next actions.",
                        task.objective
                    )
                } else {
                    format!(
                        "Task Objective:\n{}\n\nContext Ref:\n{}\n\nRespond with concise execution notes and next actions.",
                        task.objective, task.context_ref
                    )
                };
                let res = execute_task(TaskSpec {
                    command_name: "cxtask_run".to_string(),
                    input: TaskInput::Prompt(prompt),
                    output_kind: LlmOutputKind::AgentText,
                    schema: None,
                    schema_task_input: None,
                    logging_enabled: true,
                    capture_override: None,
                })?;
                println!("{}", res.stdout);
                return Ok((0, Some(res.execution_id)));
            }
        };
        return Ok((status, None));
    }

    let prompt = format!(
        "Task Objective:\n{}\n\nRespond with concise execution notes and next actions.",
        task.objective
    );
    let res = execute_task(TaskSpec {
        command_name: "cxtask_run".to_string(),
        input: TaskInput::Prompt(prompt),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    })?;
    println!("{}", res.stdout);
    Ok((0, Some(res.execution_id)))
}

fn run_task_by_id(
    id: &str,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
) -> Result<(i32, Option<String>), TaskRunError> {
    let mut tasks = read_tasks().map_err(TaskRunError::Critical)?;
    let idx = tasks
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| TaskRunError::Critical(format!("cxrs task run: task not found: {id}")))?;
    if matches!(tasks[idx].status.as_str(), "complete" | "failed") {
        return Ok((0, None));
    }
    tasks[idx].status = "in_progress".to_string();
    tasks[idx].updated_at = utc_now_iso();
    write_tasks(&tasks).map_err(TaskRunError::Critical)?;
    let prev_task_id = current_task_id();
    let prev_parent_id = current_task_parent_id();
    let _ = set_state_path("runtime.current_task_id", Value::String(id.to_string()));
    let _ = set_state_path(
        "runtime.current_task_parent_id",
        match tasks[idx].parent_id.as_ref() {
            Some(v) => Value::String(v.clone()),
            None => Value::Null,
        },
    );

    let prev_mode = env::var("CX_MODE").ok();
    let prev_backend = env::var("CX_LLM_BACKEND").ok();
    if let Some(m) = mode_override {
        // SAFETY: cx task run/run-all are sequential command paths; overrides are restored before return.
        unsafe { env::set_var("CX_MODE", m) };
    }
    if let Some(b) = backend_override {
        // SAFETY: cx task run/run-all are sequential command paths; overrides are restored before return.
        unsafe { env::set_var("CX_LLM_BACKEND", b) };
    }

    let exec = run_task_objective(&tasks[idx]);

    match prev_mode {
        Some(v) => {
            // SAFETY: restoring process env after scoped override.
            unsafe { env::set_var("CX_MODE", v) }
        }
        None => {
            // SAFETY: restoring process env after scoped override.
            unsafe { env::remove_var("CX_MODE") }
        }
    }
    match prev_backend {
        Some(v) => {
            // SAFETY: restoring process env after scoped override.
            unsafe { env::set_var("CX_LLM_BACKEND", v) }
        }
        None => {
            // SAFETY: restoring process env after scoped override.
            unsafe { env::remove_var("CX_LLM_BACKEND") }
        }
    }
    let _ = set_state_path(
        "runtime.current_task_id",
        match prev_task_id {
            Some(v) => Value::String(v),
            None => Value::Null,
        },
    );
    let _ = set_state_path(
        "runtime.current_task_parent_id",
        match prev_parent_id {
            Some(v) => Value::String(v),
            None => Value::Null,
        },
    );

    let (status_code, execution_id, objective_err) = match exec {
        Ok((c, eid)) => (c, eid, None),
        Err(e) => (1, None, Some(e)),
    };

    let mut tasks = read_tasks().map_err(TaskRunError::Critical)?;
    let idx = tasks
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| TaskRunError::Critical(format!("cxrs task run: task disappeared: {id}")))?;
    tasks[idx].status = if status_code == 0 {
        "complete".to_string()
    } else {
        "failed".to_string()
    };
    tasks[idx].updated_at = utc_now_iso();
    write_tasks(&tasks).map_err(TaskRunError::Critical)?;
    if current_task_id().as_deref() == Some(id) {
        let _ = set_state_path("runtime.current_task_id", Value::Null);
    }
    if let Some(e) = objective_err {
        eprintln!("cxrs task run: objective failed for {id}: {e}");
    }
    Ok((status_code, execution_id))
}

fn cmd_task_set_status(id: &str, new_status: &str) -> i32 {
    let mut tasks = match read_tasks() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        eprintln!("cxrs task: task not found: {id}");
        return 1;
    };
    task.status = new_status.to_string();
    task.updated_at = utc_now_iso();
    if let Err(e) = write_tasks(&tasks) {
        eprintln!("cxrs task: {e}");
        return 1;
    }
    if new_status == "in_progress" {
        let _ = set_state_path("runtime.current_task_id", Value::String(id.to_string()));
    } else if matches!(new_status, "complete" | "failed") {
        if current_task_id().as_deref() == Some(id) {
            let _ = set_state_path("runtime.current_task_id", Value::Null);
        }
    }
    println!("{id}: {new_status}");
    0
}

fn cmd_task_fanout(objective: &str, from: Option<&str>) -> i32 {
    let obj = objective.trim();
    if obj.is_empty() {
        eprintln!("Usage: {APP_NAME} task fanout <objective>");
        return 2;
    }
    let mut tasks = match read_tasks() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let parent_id = next_task_id(&tasks);
    let now = utc_now_iso();
    tasks.push(TaskRecord {
        id: parent_id.clone(),
        parent_id: None,
        role: "architect".to_string(),
        objective: obj.to_string(),
        context_ref: "fanout_parent".to_string(),
        status: "pending".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    });

    let source = from.unwrap_or("worktree");
    let diff = match source {
        "staged-diff" => Command::new("git")
            .args(["diff", "--staged", "--no-color"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        "worktree" => Command::new("git")
            .args(["diff", "--no-color"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        "log" => Command::new("git")
            .args(["log", "--oneline", "-n", "200"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        x if x.starts_with("file:") => {
            let p = x.trim_start_matches("file:");
            fs::read_to_string(p).unwrap_or_default()
        }
        _ => {
            eprintln!("cxrs task fanout: unsupported --from source '{source}'");
            return 2;
        }
    };
    let budget = env::var("CX_CONTEXT_BUDGET_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(12000);
    let chunks = if diff.trim().is_empty() {
        Vec::new()
    } else {
        chunk_text_by_budget(&diff, budget)
    };
    let chunk_count = chunks.len().clamp(1, 6);
    let roles_cycle = ["architect", "implementer", "reviewer", "tester", "doc"];
    let mut created: Vec<TaskRecord> = Vec::new();
    for i in 0..chunk_count {
        let ridx = (i + 1) % roles_cycle.len();
        let role = roles_cycle[ridx].to_string();
        let id = next_task_id(&tasks);
        let context_ref = if chunks.is_empty() {
            format!("objective:{obj}")
        } else {
            format!("diff_chunk_{}/{}", i + 1, chunk_count)
        };
        let sub_obj = match role.as_str() {
            "architect" => format!("Define implementation plan for: {obj}"),
            "implementer" => format!("Implement chunk {} for: {obj}", i + 1),
            "reviewer" => format!(
                "Review chunk {} changes for correctness/safety: {obj}",
                i + 1
            ),
            "tester" => format!("Create/execute tests for chunk {}: {obj}", i + 1),
            _ => format!("Document chunk {} outcomes: {obj}", i + 1),
        };
        let rec = TaskRecord {
            id: id.clone(),
            parent_id: Some(parent_id.clone()),
            role,
            objective: sub_obj,
            context_ref,
            status: "pending".to_string(),
            created_at: utc_now_iso(),
            updated_at: utc_now_iso(),
        };
        tasks.push(rec.clone());
        created.push(rec);
    }
    while created.len() < 3 {
        let role = roles_cycle[(created.len() + 1) % roles_cycle.len()].to_string();
        let id = next_task_id(&tasks);
        let rec = TaskRecord {
            id: id.clone(),
            parent_id: Some(parent_id.clone()),
            role: role.clone(),
            objective: format!("{} workstream for: {}", role, obj),
            context_ref: "objective".to_string(),
            status: "pending".to_string(),
            created_at: utc_now_iso(),
            updated_at: utc_now_iso(),
        };
        tasks.push(rec.clone());
        created.push(rec);
    }
    if created.len() > 8 {
        created.truncate(8);
    }
    if let Err(e) = write_tasks(&tasks) {
        eprintln!("cxrs task fanout: {e}");
        return 1;
    }
    println!("parent: {parent_id}");
    println!("id | role | status | context_ref | objective");
    println!("---|---|---|---|---");
    for t in created {
        println!(
            "{} | {} | {} | {} | {}",
            t.id, t.role, t.status, t.context_ref, t.objective
        );
    }
    0
}

fn cmd_task(args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "add" => cmd_task_add(&args[1..]),
        "list" => {
            let mut status_filter: Option<&str> = None;
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--status" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {APP_NAME} task list [--status pending|in_progress|complete|failed]"
                            );
                            return 2;
                        };
                        if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                            eprintln!("cxrs task list: invalid status '{v}'");
                            return 2;
                        }
                        status_filter = Some(v);
                        i += 2;
                    }
                    other => {
                        eprintln!("cxrs task list: unknown flag '{other}'");
                        return 2;
                    }
                }
            }
            cmd_task_list(status_filter)
        }
        "show" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} task show <id>");
                return 2;
            };
            cmd_task_show(id)
        }
        "claim" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} task claim <id>");
                return 2;
            };
            cmd_task_set_status(id, "in_progress")
        }
        "complete" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} task complete <id>");
                return 2;
            };
            cmd_task_set_status(id, "complete")
        }
        "fail" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} task fail <id>");
                return 2;
            };
            cmd_task_set_status(id, "failed")
        }
        "fanout" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} task fanout <objective>");
                return 2;
            }
            let mut objective_parts: Vec<String> = Vec::new();
            let mut from: Option<&str> = None;
            let mut i = 1usize;
            while i < args.len() {
                if args[i] == "--from" {
                    let Some(v) = args.get(i + 1).map(String::as_str) else {
                        eprintln!(
                            "Usage: {APP_NAME} task fanout <objective> [--from staged-diff|worktree|log|file:PATH]"
                        );
                        return 2;
                    };
                    from = Some(v);
                    i += 2;
                    continue;
                }
                objective_parts.push(args[i].clone());
                i += 1;
            }
            cmd_task_fanout(&objective_parts.join(" "), from)
        }
        "run" => {
            let Some(id) = args.get(1) else {
                eprintln!(
                    "Usage: {APP_NAME} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
                );
                return 2;
            };
            let mut mode_override: Option<&str> = None;
            let mut backend_override: Option<&str> = None;
            let mut i = 2usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--mode" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {APP_NAME} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
                            );
                            return 2;
                        };
                        mode_override = Some(v);
                        i += 2;
                    }
                    "--backend" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {APP_NAME} task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]"
                            );
                            return 2;
                        };
                        backend_override = Some(v);
                        i += 2;
                    }
                    other => {
                        eprintln!("cxrs task run: unknown flag '{other}'");
                        return 2;
                    }
                }
            }
            match run_task_by_id(id, mode_override, backend_override) {
                Ok((code, execution_id)) => {
                    if let Some(eid) = execution_id {
                        println!("task_id: {id}");
                        println!("execution_id: {eid}");
                    }
                    if code == 0 {
                        println!("{id}: complete");
                    } else {
                        println!("{id}: failed");
                    }
                    code
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        "run-all" => {
            let mut status_filter = "pending";
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--status" => {
                        let Some(v) = args.get(i + 1).map(String::as_str) else {
                            eprintln!(
                                "Usage: {APP_NAME} task run-all [--status pending|in_progress|complete|failed]"
                            );
                            return 2;
                        };
                        if !matches!(v, "pending" | "in_progress" | "complete" | "failed") {
                            eprintln!("cxrs task run-all: invalid status '{v}'");
                            return 2;
                        }
                        status_filter = v;
                        i += 2;
                    }
                    other => {
                        eprintln!("cxrs task run-all: unknown flag '{other}'");
                        return 2;
                    }
                }
            }
            let tasks = match read_tasks() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    return 1;
                }
            };
            let pending: Vec<String> = tasks
                .iter()
                .filter(|t| t.status == status_filter)
                .map(|t| t.id.clone())
                .collect();
            if pending.is_empty() {
                println!("No pending tasks.");
                return 0;
            }
            let mut ok = 0usize;
            let mut failed = 0usize;
            for id in pending {
                match run_task_by_id(&id, None, None) {
                    Ok((code, _)) => {
                        if code == 0 {
                            ok += 1;
                        } else {
                            failed += 1;
                            eprintln!("cxrs task run-all: task failed: {id}");
                        }
                    }
                    Err(e) => {
                        eprintln!("cxrs task run-all: critical error for {id}: {e}");
                        return 1;
                    }
                }
            }
            println!("run-all summary: complete={ok}, failed={failed}");
            if failed > 0 { 1 } else { 0 }
        }
        _ => {
            eprintln!(
                "Usage: {APP_NAME} task <add|list|show|claim|complete|fail|fanout|run|run-all> ..."
            );
            2
        }
    }
}

fn cmd_state_show() -> i32 {
    let (_state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs state show: {e}");
            return 1;
        }
    };
    match serde_json::to_string_pretty(&state) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            eprintln!("cxrs state show: failed to render JSON: {e}");
            1
        }
    }
}

fn cmd_state_get(key: &str) -> i32 {
    let (state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs state get: {e}");
            return 1;
        }
    };
    let Some(v) = value_at_path(&state, key) else {
        eprintln!("cxrs state get: key not found: {key}");
        eprintln!("state_file: {}", state_file.display());
        return 1;
    };
    match v {
        Value::String(s) => println!("{s}"),
        _ => println!("{}", v),
    }
    0
}

fn cmd_state_set(key: &str, raw_value: &str) -> i32 {
    let (state_file, mut state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs state set: {e}");
            return 1;
        }
    };
    if let Err(e) = set_value_at_path(&mut state, key, parse_cli_value(raw_value)) {
        eprintln!("cxrs state set: {e}");
        return 1;
    }
    if let Err(e) = write_json_atomic(&state_file, &state) {
        eprintln!("cxrs state set: {e}");
        return 1;
    }
    state_cache_clear();
    println!("ok");
    0
}

fn cmd_llm(args: &[String]) -> i32 {
    let print_usage = || {
        eprintln!(
            "Usage: {APP_NAME} llm <show|use <codex|ollama> [model]|unset <backend|model|all>|set-backend <codex|ollama>|set-model <model>|clear-model>"
        );
    };
    let sub = args.first().map(String::as_str).unwrap_or("show");
    match sub {
        "show" => {
            let backend = llm_backend();
            let model = llm_model();
            let ollama_pref = ollama_model_preference();
            println!("llm_backend: {backend}");
            println!(
                "active_model: {}",
                if model.is_empty() { "<unset>" } else { &model }
            );
            println!(
                "ollama_model: {}",
                if ollama_pref.is_empty() {
                    "<unset>"
                } else {
                    &ollama_pref
                }
            );
            0
        }
        "use" => {
            let Some(target) = args.get(1).map(|s| s.to_lowercase()) else {
                print_usage();
                return 2;
            };
            if target != "codex" && target != "ollama" {
                print_usage();
                return 2;
            }
            if let Err(e) = set_state_path("preferences.llm_backend", Value::String(target.clone()))
            {
                eprintln!("cxrs llm use: {e}");
                return 1;
            }
            if target == "ollama" {
                if let Some(model) = args.get(2) {
                    let m = model.trim();
                    if m.is_empty() {
                        print_usage();
                        return 2;
                    }
                    if let Err(e) =
                        set_state_path("preferences.ollama_model", Value::String(m.to_string()))
                    {
                        eprintln!("cxrs llm use: {e}");
                        return 1;
                    }
                }
                println!("ok");
                println!("llm_backend: ollama");
                let pref = ollama_model_preference();
                println!(
                    "ollama_model: {}",
                    if pref.is_empty() { "<unset>" } else { &pref }
                );
                return 0;
            }
            println!("ok");
            println!("llm_backend: codex");
            0
        }
        "unset" => {
            let target = args.get(1).map(String::as_str).unwrap_or("all");
            match target {
                "backend" => {
                    if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                        eprintln!("cxrs llm unset backend: {e}");
                        return 1;
                    }
                    println!("ok");
                    println!("llm_backend: <unset>");
                    0
                }
                "model" => {
                    if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                        eprintln!("cxrs llm unset model: {e}");
                        return 1;
                    }
                    println!("ok");
                    println!("ollama_model: <unset>");
                    0
                }
                "all" => {
                    if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                        eprintln!("cxrs llm unset all: {e}");
                        return 1;
                    }
                    if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                        eprintln!("cxrs llm unset all: {e}");
                        return 1;
                    }
                    println!("ok");
                    println!("llm_backend: <unset>");
                    println!("ollama_model: <unset>");
                    0
                }
                _ => {
                    print_usage();
                    2
                }
            }
        }
        "set-backend" => {
            let Some(v) = args.get(1).map(|s| s.to_lowercase()) else {
                print_usage();
                return 2;
            };
            if v != "codex" && v != "ollama" {
                print_usage();
                return 2;
            }
            if let Err(e) = set_state_path("preferences.llm_backend", Value::String(v.clone())) {
                eprintln!("cxrs llm set-backend: {e}");
                return 1;
            }
            println!("ok");
            println!("llm_backend: {v}");
            0
        }
        "set-model" => {
            let Some(model) = args.get(1) else {
                print_usage();
                return 2;
            };
            if model.trim().is_empty() {
                print_usage();
                return 2;
            }
            if let Err(e) = set_state_path(
                "preferences.ollama_model",
                Value::String(model.trim().to_string()),
            ) {
                eprintln!("cxrs llm set-model: {e}");
                return 1;
            }
            println!("ok");
            println!("ollama_model: {}", model.trim());
            0
        }
        "clear-model" => {
            if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                eprintln!("cxrs llm clear-model: {e}");
                return 1;
            }
            println!("ok");
            println!("ollama_model: <unset>");
            0
        }
        other => {
            eprintln!("{APP_NAME} llm: unknown subcommand '{other}'");
            print_usage();
            2
        }
    }
}

fn print_version() {
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let source = env::var("CX_SOURCE_LOCATION").unwrap_or_else(|_| "standalone:cxrs".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state_file = resolve_state_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let mode = env::var("CX_MODE").unwrap_or_else(|_| "lean".to_string());
    let schema_relaxed = env::var("CX_SCHEMA_RELAXED").unwrap_or_else(|_| "0".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    let capture_provider = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "auto".to_string());
    let native_reduce = env::var("CX_NATIVE_REDUCE").unwrap_or_else(|_| "1".to_string());
    let rtk_min = env::var("CX_RTK_MIN_VERSION").unwrap_or_else(|_| "0.22.1".to_string());
    let rtk_max = env::var("CX_RTK_MAX_VERSION").unwrap_or_default();
    let rtk_available = bin_in_path("rtk");
    let rtk_usable = rtk_is_usable();
    let budget_chars = env::var("CX_CONTEXT_BUDGET_CHARS").unwrap_or_else(|_| "12000".to_string());
    let budget_lines = env::var("CX_CONTEXT_BUDGET_LINES").unwrap_or_else(|_| "300".to_string());
    let clip_mode = env::var("CX_CONTEXT_CLIP_MODE").unwrap_or_else(|_| "smart".to_string());
    let quarantine_dir = resolve_quarantine_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state = read_state_value();
    let cc = state
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.conventional_commits"))
        .map(value_to_display)
        .unwrap_or_else(|| "n/a".to_string());
    let pr_fmt = state
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.pr_summary_format"))
        .map(value_to_display)
        .unwrap_or_else(|| "n/a".to_string());
    println!("name: {APP_NAME}");
    println!("version: {}", toolchain_version_string());
    println!("cwd: {cwd}");
    println!("execution_path: {execution_path}");
    println!("source: {source}");
    println!("log_file: {log_file}");
    println!("state_file: {state_file}");
    println!("quarantine_dir: {quarantine_dir}");
    println!("mode: {mode}");
    println!("llm_backend: {backend}");
    println!("llm_model: {active_model}");
    println!("backend_resolution: backend={backend} model={active_model}");
    println!("schema_relaxed: {schema_relaxed}");
    println!("capture_provider: {capture_provider}");
    println!("native_reduce: {native_reduce}");
    println!("rtk_available: {rtk_available}");
    println!("rtk_supported_range_min: {rtk_min}");
    println!(
        "rtk_supported_range_max: {}",
        if rtk_max.is_empty() {
            "<unset>"
        } else {
            &rtk_max
        }
    );
    println!("rtk_usable: {rtk_usable}");
    println!("budget_chars: {budget_chars}");
    println!("budget_lines: {budget_lines}");
    println!("clip_mode: {clip_mode}");
    println!("state.preferences.conventional_commits: {cc}");
    println!("state.preferences.pr_summary_format: {pr_fmt}");
}

fn cmd_core() -> i32 {
    let mode = env::var("CX_MODE").unwrap_or_else(|_| "lean".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    let capture_provider = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "auto".to_string());
    let rtk_enabled = env::var("CX_RTK_SYSTEM")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let rtk_available = rtk_is_usable();
    let cfg = budget_config_from_env();
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let bash_fallback = execution_path.contains("bash");

    println!("== cxcore ==");
    println!("version: {}", toolchain_version_string());
    println!("execution_path: {execution_path}");
    println!("bash_fallback_used: {bash_fallback}");
    println!("backend: {backend}");
    println!("active_model: {active_model}");
    println!("execution_mode: {mode}");
    println!("capture_provider: {capture_provider}");
    println!("capture_rtk_enabled: {rtk_enabled}");
    println!("capture_rtk_available: {rtk_available}");
    println!("budget_chars: {}", cfg.budget_chars);
    println!("budget_lines: {}", cfg.budget_lines);
    println!("clip_mode: {}", cfg.clip_mode);
    println!("clip_footer: {}", cfg.clip_footer);
    println!("schema_enforcement: true");
    println!("logging_enabled: {}", logging_enabled());
    println!("log_file: {log_file}");
    0
}

fn bin_in_path(bin: &str) -> bool {
    let path = match env::var_os("PATH") {
        Some(v) => v,
        None => return false,
    };
    env::split_paths(&path).any(|dir| {
        let candidate = dir.join(bin);
        Path::new(&candidate).is_file()
    })
}

fn print_doctor() -> i32 {
    let backend = llm_backend();
    let llm_bin = llm_bin_name();
    let required = ["git", "jq"];
    let optional = ["rtk"];
    let mut missing_required = 0;

    println!("== cxrs doctor ==");
    for bin in required {
        if bin_in_path(bin) {
            println!("OK: {bin}");
        } else {
            println!("MISSING: {bin}");
            missing_required += 1;
        }
    }
    if bin_in_path(llm_bin) {
        println!("OK: {llm_bin} (selected backend: {backend})");
    } else {
        println!("MISSING: {llm_bin} (selected backend: {backend})");
        missing_required += 1;
    }
    if backend != "codex" {
        if bin_in_path("codex") {
            println!("OK: codex (recommended primary backend)");
        } else {
            println!("WARN: codex not found (recommended primary backend)");
        }
    }
    for bin in optional {
        if bin_in_path(bin) {
            println!("OK: {bin} (optional)");
        } else {
            println!("WARN: {bin} not found (optional)");
        }
    }
    if bin_in_path("rtk") && !rtk_is_usable() {
        println!("WARN: rtk version unsupported by configured range; raw fallback will be used.");
    }
    if missing_required > 0 {
        println!("FAIL: install required binaries before using cxrs.");
        return 1;
    }

    println!();
    println!("== llm json pipeline ({backend}) ==");
    let probe = match run_llm_jsonl("ping") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("FAIL: {backend} json pipeline failed: {e}");
            return 1;
        }
    };
    let mut agent_count = 0u64;
    let mut reasoning_count = 0u64;
    for line in probe.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("item.completed") {
            continue;
        }
        let t = v
            .get("item")
            .and_then(|i| i.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if t == "agent_message" {
            agent_count += 1;
        } else if t == "reasoning" {
            reasoning_count += 1;
        }
    }
    println!("agent_message events: {agent_count}");
    println!("reasoning events:     {reasoning_count}");
    if agent_count < 1 {
        eprintln!("FAIL: expected >=1 agent_message event");
        return 1;
    }

    println!();
    println!("== _codex_text equivalent ==");
    let probe2 = match run_llm_jsonl("2+2? (just the number)") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("FAIL: {backend} text probe failed: {e}");
            return 1;
        }
    };
    let txt = extract_agent_text(&probe2).unwrap_or_default();
    println!("output: {txt}");
    if txt.trim() != "4" {
        println!("WARN: expected '4', got '{}'", txt.trim());
    }

    println!();
    println!("== git context (optional) ==");
    match Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
    {
        Ok(out) if out.status.success() => {
            println!("in git repo: yes");
            if let Ok(branch_out) = Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
            {
                let branch = String::from_utf8_lossy(&branch_out.stdout)
                    .trim()
                    .to_string();
                if !branch.is_empty() {
                    println!("branch: {branch}");
                }
            }
        }
        _ => println!("in git repo: no (skip git-based checks)"),
    }

    println!();
    println!("PASS: core pipeline looks healthy.");
    0
}

fn bash_type_of_function(repo: &Path, name: &str) -> Option<String> {
    let cx_sh = repo.join("cx.sh");
    let cmd = format!(
        "source '{}' >/dev/null 2>&1; type -a {} 2>/dev/null",
        cx_sh.display(),
        name
    );
    let out = Command::new("bash").arg("-lc").arg(cmd).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn route_handler_for(name: &str) -> Option<String> {
    if is_native_name(name) {
        Some(name.to_string())
    } else if is_compat_name(name) {
        Some(format!("cx-compat {name}"))
    } else {
        None
    }
}

fn rust_route_names() -> Vec<String> {
    let names = vec![
        "help",
        "version",
        "where",
        "routes",
        "logs",
        "ci",
        "task",
        "diag",
        "parity",
        "doctor",
        "state",
        "llm",
        "policy",
        "bench",
        "metrics",
        "prompt",
        "roles",
        "fanout",
        "promptlint",
        "cx",
        "cxj",
        "cxo",
        "cxol",
        "cxcopy",
        "fix",
        "budget",
        "log-tail",
        "health",
        "rtk-status",
        "log-on",
        "log-off",
        "alert-show",
        "alert-on",
        "alert-off",
        "chunk",
        "cx-compat",
        "profile",
        "alert",
        "optimize",
        "worklog",
        "trace",
        "next",
        "fix-run",
        "diffsum",
        "diffsum-staged",
        "commitjson",
        "commitmsg",
        "replay",
        "quarantine",
        "supports",
        "cxversion",
        "cxdoctor",
        "cxwhere",
        "cxdiag",
        "cxparity",
        "cxlogs",
        "cxmetrics",
        "cxprofile",
        "cxtrace",
        "cxalert",
        "cxoptimize",
        "cxworklog",
        "cxpolicy",
        "cxstate",
        "cxllm",
        "cxbench",
        "cxprompt",
        "cxroles",
        "cxfanout",
        "cxpromptlint",
        "cxnext",
        "cxfix",
        "cxdiffsum",
        "cxdiffsum_staged",
        "cxcommitjson",
        "cxcommitmsg",
        "cxbudget",
        "cxlog_tail",
        "cxhealth",
        "cxrtk",
        "cxlog_on",
        "cxlog_off",
        "cxalert_show",
        "cxalert_on",
        "cxalert_off",
        "cxchunk",
        "cxfix_run",
        "cxreplay",
        "cxquarantine",
        "cxtask",
    ];
    let mut out: Vec<String> = names.into_iter().map(|s| s.to_string()).collect();
    out.sort();
    out.dedup();
    out
}

fn cmd_routes(args: &[String]) -> i32 {
    let json = args.first().is_some_and(|a| a == "--json");
    let names: Vec<String> = if json {
        args[1..].to_vec()
    } else {
        args.to_vec()
    };
    let targets = if names.is_empty() {
        rust_route_names()
    } else {
        names
    };
    if json {
        let arr: Vec<Value> = targets
            .iter()
            .filter_map(|name| {
                route_handler_for(name).map(|handler| {
                    json!({
                        "name": name,
                        "route": "rust",
                        "handler": handler
                    })
                })
            })
            .collect();
        match serde_json::to_string_pretty(&arr) {
            Ok(s) => {
                println!("{s}");
                0
            }
            Err(e) => {
                eprintln!("cxrs routes: failed to render json: {e}");
                1
            }
        }
    } else {
        for name in targets {
            if let Some(handler) = route_handler_for(&name) {
                println!("{name}: rust ({handler})");
            }
        }
        0
    }
}

fn cmd_schema(args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    if sub != "list" {
        eprintln!("Usage: {APP_NAME} schema list [--json]");
        return 2;
    }
    let as_json = args.iter().any(|a| a == "--json");
    let Some(dir) = resolve_schema_dir() else {
        eprintln!("cxrs schema: unable to resolve schema directory");
        return 1;
    };
    let schemas = match list_schemas() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs schema: {e}");
            return 1;
        }
    };

    if as_json {
        let rows: Vec<Value> = schemas
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "path": s.path.display().to_string(),
                    "id": s.id.clone().unwrap_or_default()
                })
            })
            .collect();
        println!(
            "{}",
            json!({
                "schema_dir": dir.display().to_string(),
                "file_count": rows.len(),
                "schemas": rows
            })
        );
        return 0;
    }

    println!("schema_dir: {}", dir.display());
    println!("file_count: {}", schemas.len());
    for s in schemas {
        let id = s.id.unwrap_or_else(|| "<no $id>".to_string());
        println!("- {} ({}) [{}]", s.name, s.path.display(), id);
    }
    0
}

fn cmd_ci(args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("validate");
    if sub != "validate" {
        eprintln!("Usage: {APP_NAME} ci validate [--strict] [--legacy-ok] [--json]");
        return 2;
    }
    let strict = args.iter().any(|a| a == "--strict");
    let legacy_ok = args.iter().any(|a| a == "--legacy-ok");
    let json_out = args.iter().any(|a| a == "--json");

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let Some(root) = repo_root() else {
        eprintln!("cxrs ci validate: not inside a git repository");
        return 2;
    };
    let schema_dir = root.join(".codex").join("schemas");
    if !schema_dir.is_dir() {
        errors.push(format!("missing schema registry dir: {}", schema_dir.display()));
    } else {
        let required = [
            "commitjson.schema.json",
            "diffsum.schema.json",
            "next.schema.json",
            "fixrun.schema.json",
        ];
        for name in required {
            let p = schema_dir.join(name);
            if !p.is_file() {
                errors.push(format!("missing schema: {}", p.display()));
            } else {
                // Compile to ensure the schema is valid JSON Schema.
                match fs::read_to_string(&p)
                    .ok()
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                {
                    Some(v) => {
                        if let Err(e) = JSONSchema::compile(&v) {
                            errors.push(format!("schema failed to compile ({}): {e}", p.display()));
                        }
                    }
                    None => errors.push(format!("schema unreadable/invalid json: {}", p.display())),
                }
            }
        }
    }

    let log_file = resolve_log_file();
    if log_file.is_none() {
        errors.push("unable to resolve log file".to_string());
    }
    if let Some(log_file) = log_file {
        if log_file.exists() {
            match validate_runs_jsonl_file(&log_file, legacy_ok) {
                Ok(outcome) => {
                    if !outcome.issues.is_empty() {
                        if outcome.legacy_ok && outcome.invalid_json_lines == 0 {
                            warnings.extend(outcome.issues);
                        } else {
                            errors.extend(outcome.issues);
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        } else {
            warnings.push(format!("no log file at {}", log_file.display()));
        }
    }

    let budget = budget_config_from_env();
    if budget.budget_chars == 0 {
        errors.push("budget_chars must be > 0".to_string());
    }
    if budget.budget_lines == 0 {
        errors.push("budget_lines must be > 0".to_string());
    }
    if !matches!(budget.clip_mode.as_str(), "smart" | "head" | "tail") {
        warnings.push(format!(
            "clip_mode '{}' not recognized; expected smart|head|tail",
            budget.clip_mode
        ));
    }

    if strict {
        // Strict mode adds a cheap local integrity check over quarantine directory naming.
        let qdir = root.join(".codex").join("quarantine");
        if qdir.exists() && !qdir.is_dir() {
            errors.push(format!("quarantine path exists but is not a dir: {}", qdir.display()));
        }
    }

    let ok = errors.is_empty();
    if json_out {
        let v = json!({
            "ok": ok,
            "strict": strict,
            "legacy_ok": legacy_ok,
            "repo_root": root.display().to_string(),
            "schema_dir": schema_dir.display().to_string(),
            "log_file": resolve_log_file().map(|p| p.display().to_string()),
            "budget_chars": budget.budget_chars,
            "budget_lines": budget.budget_lines,
            "clip_mode": budget.clip_mode,
            "warnings": warnings,
            "errors": errors
        });
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()));
        return if ok { 0 } else { 1 };
    }

    println!("== cxrs ci validate ==");
    println!("repo_root: {}", root.display());
    println!("schema_dir: {}", schema_dir.display());
    if let Some(p) = resolve_log_file() {
        println!("log_file: {}", p.display());
    } else {
        println!("log_file: <unresolved>");
    }
    println!(
        "budget: chars={} lines={} mode={}",
        budget.budget_chars, budget.budget_lines, budget.clip_mode
    );

    if !warnings.is_empty() {
        println!("warnings: {}", warnings.len());
        for w in warnings.iter().take(10) {
            println!("- {w}");
        }
        if warnings.len() > 10 {
            println!("- ... and {} more", warnings.len() - 10);
        }
    } else {
        println!("warnings: 0");
    }

    if !errors.is_empty() {
        println!("errors: {}", errors.len());
        for e in errors.iter().take(20) {
            println!("- {e}");
        }
        if errors.len() > 20 {
            println!("- ... and {} more", errors.len() - 20);
        }
        println!("status: fail");
        return 1;
    }
    println!("status: ok");
    0
}

fn print_where(cmds: &[String]) -> i32 {
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    let exe = env::current_exe()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let bin_cx =
        env::var("CX_BIN_CX").unwrap_or_else(|_| repo.join("bin").join("cx").display().to_string());
    let bash_lib = repo.join("lib").join("cx.sh").display().to_string();
    let source = env::var("CX_SOURCE_LOCATION").unwrap_or_else(|_| "standalone:cxrs".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state_file = resolve_state_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let repo_root = repo.display().to_string();
    let bash_sourceable = repo.join("lib").join("cx.sh").is_file();
    println!("== cxwhere ==");
    println!("bin_cx: {bin_cx}");
    println!("cxrs_path: {exe}");
    println!("cxrs_version: {}", toolchain_version_string());
    println!("bash_lib: {bash_lib}");
    println!("bash_lib_sourceable: {bash_sourceable}");
    println!("repo_root: {repo_root}");
    println!("log_file: {log_file}");
    println!("source: {source}");
    println!("state_file: {state_file}");
    println!("backend: {backend}");
    println!(
        "active_model: {}",
        if model.is_empty() { "<unset>" } else { &model }
    );
    if !cmds.is_empty() {
        println!("routes:");
        for cmd in cmds {
            if let Some(handler) = route_handler_for(cmd) {
                println!("- {cmd}: route=rust handler={handler}");
                continue;
            }
            if let Some(type_out) = bash_type_of_function(&repo, cmd) {
                println!("- {cmd}: route=bash function={cmd}");
                for line in type_out.lines() {
                    println!("  {line}");
                }
            } else {
                println!("- {cmd}: route=unknown");
            }
        }
    }
    0
}

// runs.jsonl readers moved to `logs.rs`

fn parse_ts_epoch(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp())
}

fn print_profile(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("== cxrs profile (last {n} runs) ==");
        println!("Runs: 0");
        println!("Avg duration: 0ms");
        println!("Avg effective tokens: 0");
        println!("Cache hit rate: n/a");
        println!("Output/input ratio: n/a");
        println!("Slowest run: n/a");
        println!("Heaviest context: n/a");
        println!("log_file: {}", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs profile: {e}");
            return 1;
        }
    };
    let total = runs.len();
    if total == 0 {
        println!("== cxrs profile (last {n} runs) ==");
        println!("Runs: 0");
        println!("Avg duration: 0ms");
        println!("Avg effective tokens: 0");
        println!("Cache hit rate: n/a");
        println!("Output/input ratio: n/a");
        println!("Slowest run: n/a");
        println!("Heaviest context: n/a");
        println!("log_file: {}", log_file.display());
        return 0;
    }

    let sum_dur: u64 = runs.iter().map(|r| r.duration_ms.unwrap_or(0)).sum();
    let sum_eff: u64 = runs
        .iter()
        .map(|r| r.effective_input_tokens.unwrap_or(0))
        .sum();
    let sum_in: u64 = runs.iter().map(|r| r.input_tokens.unwrap_or(0)).sum();
    let sum_cached: u64 = runs
        .iter()
        .map(|r| r.cached_input_tokens.unwrap_or(0))
        .sum();
    let sum_out: u64 = runs.iter().map(|r| r.output_tokens.unwrap_or(0)).sum();

    let avg_dur = sum_dur / (total as u64);
    let avg_eff = sum_eff / (total as u64);
    let cache_hit_rate = if sum_in == 0 {
        None
    } else {
        Some((sum_cached as f64) / (sum_in as f64))
    };
    let out_in_ratio = if sum_eff == 0 {
        None
    } else {
        Some((sum_out as f64) / (sum_eff as f64))
    };

    let slowest = runs
        .iter()
        .filter_map(|r| {
            r.duration_ms
                .map(|d| (d, r.tool.clone().unwrap_or_else(|| "unknown".to_string())))
        })
        .max_by_key(|(d, _)| *d);
    let heaviest = runs
        .iter()
        .filter_map(|r| {
            r.effective_input_tokens
                .map(|e| (e, r.tool.clone().unwrap_or_else(|| "unknown".to_string())))
        })
        .max_by_key(|(e, _)| *e);

    println!("== cxrs profile (last {n} runs) ==");
    println!("Runs: {total}");
    println!("Avg duration: {avg_dur}ms");
    println!("Avg effective tokens: {avg_eff}");
    if let Some(v) = cache_hit_rate {
        println!("Cache hit rate: {}%", (v * 100.0).round() as i64);
    } else {
        println!("Cache hit rate: n/a");
    }
    if let Some(v) = out_in_ratio {
        println!("Output/input ratio: {:.2}", v);
    } else {
        println!("Output/input ratio: n/a");
    }
    if let Some((d, t)) = slowest {
        println!("Slowest run: {d}ms ({t})");
    } else {
        println!("Slowest run: n/a");
    }
    if let Some((e, t)) = heaviest {
        println!("Heaviest context: {e} effective tokens ({t})");
    } else {
        println!("Heaviest context: n/a");
    }
    println!("log_file: {}", log_file.display());
    0
}

fn print_metrics(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        let out = json!({
            "log_file": log_file.display().to_string(),
            "runs": 0,
            "avg_duration_ms": 0.0,
            "avg_input_tokens": 0.0,
            "avg_cached_input_tokens": 0.0,
            "avg_effective_input_tokens": 0.0,
            "avg_output_tokens": 0.0,
            "by_tool": []
        });
        match serde_json::to_string_pretty(&out) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("cxrs metrics: failed to render JSON: {e}");
                return 1;
            }
        }
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs metrics: {e}");
            return 1;
        }
    };

    let total = runs.len() as f64;
    let sum_dur: f64 = runs.iter().map(|r| r.duration_ms.unwrap_or(0) as f64).sum();
    let sum_in: f64 = runs
        .iter()
        .map(|r| r.input_tokens.unwrap_or(0) as f64)
        .sum();
    let sum_cached: f64 = runs
        .iter()
        .map(|r| r.cached_input_tokens.unwrap_or(0) as f64)
        .sum();
    let sum_eff: f64 = runs
        .iter()
        .map(|r| r.effective_input_tokens.unwrap_or(0) as f64)
        .sum();
    let sum_out: f64 = runs
        .iter()
        .map(|r| r.output_tokens.unwrap_or(0) as f64)
        .sum();

    let mut grouped: HashMap<String, Vec<&RunEntry>> = HashMap::new();
    for r in &runs {
        let key = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        grouped.entry(key).or_default().push(r);
    }
    let mut by_tool: Vec<Value> = grouped
        .into_iter()
        .map(|(tool, entries)| {
            let c = entries.len() as f64;
            let d: f64 = entries
                .iter()
                .map(|r| r.duration_ms.unwrap_or(0) as f64)
                .sum();
            let e: f64 = entries
                .iter()
                .map(|r| r.effective_input_tokens.unwrap_or(0) as f64)
                .sum();
            let o: f64 = entries
                .iter()
                .map(|r| r.output_tokens.unwrap_or(0) as f64)
                .sum();
            json!({
                "tool": tool,
                "runs": entries.len(),
                "avg_duration_ms": if c == 0.0 { 0.0 } else { d / c },
                "avg_effective_input_tokens": if c == 0.0 { 0.0 } else { e / c },
                "avg_output_tokens": if c == 0.0 { 0.0 } else { o / c }
            })
        })
        .collect();
    by_tool.sort_by(|a, b| {
        b.get("runs")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .cmp(&a.get("runs").and_then(Value::as_u64).unwrap_or(0))
    });

    let out = json!({
      "log_file": log_file.display().to_string(),
      "runs": runs.len(),
      "avg_duration_ms": if total == 0.0 { 0.0 } else { sum_dur / total },
      "avg_input_tokens": if total == 0.0 { 0.0 } else { sum_in / total },
      "avg_cached_input_tokens": if total == 0.0 { 0.0 } else { sum_cached / total },
      "avg_effective_input_tokens": if total == 0.0 { 0.0 } else { sum_eff / total },
      "avg_output_tokens": if total == 0.0 { 0.0 } else { sum_out / total },
      "by_tool": by_tool
    });
    match serde_json::to_string_pretty(&out) {
        Ok(s) => println!("{s}"),
        Err(e) => {
            eprintln!("cxrs metrics: failed to render JSON: {e}");
            return 1;
        }
    }
    0
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(default)
}

fn parse_semver_triplet(raw: &str) -> Option<(u64, u64, u64)> {
    let candidate = raw
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find(|s| s.chars().filter(|c| *c == '.').count() >= 1 && !s.is_empty())?;
    let mut it = candidate.split('.');
    let major = it.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let minor = it.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let patch = it.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

fn semver_cmp(a: (u64, u64, u64), b: (u64, u64, u64)) -> i8 {
    if a.0 != b.0 {
        return if a.0 > b.0 { 1 } else { -1 };
    }
    if a.1 != b.1 {
        return if a.1 > b.1 { 1 } else { -1 };
    }
    if a.2 != b.2 {
        return if a.2 > b.2 { 1 } else { -1 };
    }
    0
}

fn rtk_version_raw() -> Option<String> {
    let out = Command::new("rtk").arg("--version").output().ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    if s.trim().is_empty() {
        s = String::from_utf8_lossy(&out.stderr).to_string();
    }
    let t = s.trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

fn rtk_is_usable() -> bool {
    if Command::new("rtk")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        return false;
    }
    let min_v = env::var("CX_RTK_MIN_VERSION").unwrap_or_else(|_| "0.22.1".to_string());
    let max_v = env::var("CX_RTK_MAX_VERSION").unwrap_or_default();
    let ver_raw = rtk_version_raw().unwrap_or_default();
    let Some(cur) = parse_semver_triplet(&ver_raw) else {
        if !RTK_WARNED_UNSUPPORTED.swap(true, Ordering::SeqCst) {
            eprintln!("cxrs: unable to parse rtk version; falling back to raw command output.");
        }
        return false;
    };
    let min = parse_semver_triplet(&min_v).unwrap_or((0, 0, 0));
    if semver_cmp(cur, min) < 0 {
        if !RTK_WARNED_UNSUPPORTED.swap(true, Ordering::SeqCst) {
            eprintln!(
                "cxrs: rtk version '{}' is below supported minimum '{}'; falling back to raw command output.",
                ver_raw, min_v
            );
        }
        return false;
    }
    if !max_v.is_empty() {
        let max = parse_semver_triplet(&max_v).unwrap_or((u64::MAX, u64::MAX, u64::MAX));
        if semver_cmp(cur, max) > 0 {
            if !RTK_WARNED_UNSUPPORTED.swap(true, Ordering::SeqCst) {
                eprintln!(
                    "cxrs: rtk version '{}' is above supported maximum '{}'; falling back to raw command output.",
                    ver_raw, max_v
                );
            }
            return false;
        }
    }
    true
}

fn print_alert(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("== cxrs alert (last {n} runs) ==");
        println!("Runs: 0");
        println!("Slow threshold violations: 0");
        println!("Token threshold violations: 0");
        println!("Avg cache hit rate: n/a");
        println!("Top 5 slowest: n/a");
        println!("Top 5 heaviest: n/a");
        println!("log_file: {}", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs alert: {e}");
            return 1;
        }
    };
    let total = runs.len();
    let max_ms = env_u64("CXALERT_MAX_MS", 12000);
    let max_eff = env_u64("CXALERT_MAX_EFF_IN", 8000);

    let mut slow_violations = 0usize;
    let mut token_violations = 0usize;
    let mut sum_in: u64 = 0;
    let mut sum_cached: u64 = 0;

    for run in &runs {
        let d = run.duration_ms.unwrap_or(0);
        let eff = run.effective_input_tokens.unwrap_or(0);
        if d > max_ms {
            slow_violations += 1;
        }
        if eff > max_eff {
            token_violations += 1;
        }
        sum_in += run.input_tokens.unwrap_or(0);
        sum_cached += run.cached_input_tokens.unwrap_or(0);
    }

    let mut slowest: Vec<(u64, String, String)> = runs
        .iter()
        .filter_map(|r| {
            r.duration_ms.map(|d| {
                (
                    d,
                    r.tool.clone().unwrap_or_else(|| "unknown".to_string()),
                    r.ts.clone().unwrap_or_else(|| "n/a".to_string()),
                )
            })
        })
        .collect();
    slowest.sort_by(|a, b| b.0.cmp(&a.0));
    slowest.truncate(5);

    let mut heaviest: Vec<(u64, String, String)> = runs
        .iter()
        .filter_map(|r| {
            r.effective_input_tokens.map(|e| {
                (
                    e,
                    r.tool.clone().unwrap_or_else(|| "unknown".to_string()),
                    r.ts.clone().unwrap_or_else(|| "n/a".to_string()),
                )
            })
        })
        .collect();
    heaviest.sort_by(|a, b| b.0.cmp(&a.0));
    heaviest.truncate(5);

    let cache_hit = if sum_in == 0 {
        None
    } else {
        Some((sum_cached as f64 / sum_in as f64) * 100.0)
    };

    println!("== cxrs alert (last {n} runs) ==");
    println!("Runs: {total}");
    println!("Thresholds: max_ms={max_ms}, max_eff_in={max_eff}");
    println!("Slow threshold violations: {slow_violations}");
    println!("Token threshold violations: {token_violations}");
    match cache_hit {
        Some(v) => println!("Avg cache hit rate: {}%", v.round() as i64),
        None => println!("Avg cache hit rate: n/a"),
    }

    if slowest.is_empty() {
        println!("Top 5 slowest: n/a");
    } else {
        println!("Top 5 slowest:");
        for (d, tool, ts) in slowest {
            println!("- {d}ms | {tool} | {ts}");
        }
    }

    if heaviest.is_empty() {
        println!("Top 5 heaviest: n/a");
    } else {
        println!("Top 5 heaviest:");
        for (e, tool, ts) in heaviest {
            println!("- {e} effective tokens | {tool} | {ts}");
        }
    }
    println!("log_file: {}", log_file.display());
    0
}

fn parse_optimize_args(args: &[String], default_n: usize) -> Result<(usize, bool), String> {
    let mut n = default_n;
    let mut json_out = false;
    for a in args {
        if a == "--json" {
            json_out = true;
            continue;
        }
        if let Ok(v) = a.parse::<usize>() {
            if v > 0 {
                n = v;
                continue;
            }
        }
        return Err(format!("invalid argument: {a}"));
    }
    Ok((n, json_out))
}

fn optimize_report(n: usize) -> Result<Value, String> {
    let Some(log_file) = resolve_log_file() else {
        return Err("unable to resolve log file".to_string());
    };
    if !log_file.exists() {
        return Ok(json!({
            "window": n,
            "runs": 0,
            "scoreboard": {"runs": 0},
            "anomalies": [],
            "recommendations": ["No runs available in log window."],
            "log_file": log_file.display().to_string()
        }));
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    if runs.is_empty() {
        return Ok(json!({
            "window": n,
            "runs": 0,
            "scoreboard": {"runs": 0},
            "anomalies": [],
            "recommendations": ["No runs available in log window."],
            "log_file": log_file.display().to_string()
        }));
    }

    let max_ms = env_u64("CXALERT_MAX_MS", 12000);
    let max_eff = env_u64("CXALERT_MAX_EFF_IN", 8000);
    let total = runs.len() as u64;

    let mut tool_eff: HashMap<String, (u64, u64)> = HashMap::new();
    let mut tool_dur: HashMap<String, (u64, u64)> = HashMap::new();
    let mut alerts = 0u64;
    let mut schema_fails = 0u64;
    let mut schema_total = 0u64;
    let mut clipped_count = 0u64;
    let mut clipped_total = 0u64;
    let mut provider_stats: HashMap<String, (u64, u64, u64, u64)> = HashMap::new();
    // provider -> (raw_sum, processed_sum, clipped_sum, count)

    let mut sum_in: u64 = 0;
    let mut sum_cached: u64 = 0;

    for r in &runs {
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let eff = r.effective_input_tokens.unwrap_or(0);
        let dur = r.duration_ms.unwrap_or(0);

        let eff_entry = tool_eff.entry(tool.clone()).or_insert((0, 0));
        eff_entry.0 += eff;
        eff_entry.1 += 1;

        let dur_entry = tool_dur.entry(tool).or_insert((0, 0));
        dur_entry.0 += dur;
        dur_entry.1 += 1;

        if dur > max_ms || eff > max_eff {
            alerts += 1;
        }
        if r.schema_enforced.unwrap_or(false) {
            schema_total += 1;
            if r.schema_valid == Some(false) {
                schema_fails += 1;
            }
        }
        if r.clipped.is_some() {
            clipped_total += 1;
            if r.clipped == Some(true) {
                clipped_count += 1;
            }
        }
        if let Some(provider) = r.capture_provider.as_ref() {
            let entry = provider_stats
                .entry(provider.clone())
                .or_insert((0, 0, 0, 0));
            entry.0 += r.system_output_len_raw.unwrap_or(0);
            entry.1 += r.system_output_len_processed.unwrap_or(0);
            entry.2 += r.system_output_len_clipped.unwrap_or(0);
            entry.3 += 1;
        }

        sum_in += r.input_tokens.unwrap_or(0);
        sum_cached += r.cached_input_tokens.unwrap_or(0);
    }

    let mut top_eff: Vec<(String, u64)> = tool_eff
        .into_iter()
        .map(|(tool, (sum, count))| (tool, if count == 0 { 0 } else { sum / count }))
        .collect();
    top_eff.sort_by(|a, b| b.1.cmp(&a.1));
    top_eff.truncate(5);

    let mut top_dur: Vec<(String, u64)> = tool_dur
        .into_iter()
        .map(|(tool, (sum, count))| (tool, if count == 0 { 0 } else { sum / count }))
        .collect();
    top_dur.sort_by(|a, b| b.1.cmp(&a.1));
    top_dur.truncate(5);

    let cache_all = if sum_in == 0 {
        None
    } else {
        Some(sum_cached as f64 / sum_in as f64)
    };

    let mid = runs.len() / 2;
    let (first, second) = runs.split_at(mid.max(1).min(runs.len()));
    let first_in: u64 = first.iter().map(|r| r.input_tokens.unwrap_or(0)).sum();
    let first_cached: u64 = first
        .iter()
        .map(|r| r.cached_input_tokens.unwrap_or(0))
        .sum();
    let second_in: u64 = second.iter().map(|r| r.input_tokens.unwrap_or(0)).sum();
    let second_cached: u64 = second
        .iter()
        .map(|r| r.cached_input_tokens.unwrap_or(0))
        .sum();
    let first_cache = if first_in == 0 {
        None
    } else {
        Some(first_cached as f64 / first_in as f64)
    };
    let second_cache = if second_in == 0 {
        None
    } else {
        Some(second_cached as f64 / second_in as f64)
    };
    let clip_freq = if clipped_total == 0 {
        None
    } else {
        Some(clipped_count as f64 / clipped_total as f64)
    };
    let schema_fail_freq = if schema_total == 0 {
        None
    } else {
        Some(schema_fails as f64 / schema_total as f64)
    };
    let mut compression_rows: Vec<Value> = provider_stats
        .into_iter()
        .map(|(provider, (raw, processed, clipped, count))| {
            let processed_ratio = if raw == 0 {
                Value::Null
            } else {
                json!((processed as f64) / (raw as f64))
            };
            let clipped_ratio = if raw == 0 {
                Value::Null
            } else {
                json!((clipped as f64) / (raw as f64))
            };
            json!({
                "provider": provider,
                "runs": count,
                "raw_sum": raw,
                "processed_sum": processed,
                "clipped_sum": clipped,
                "processed_over_raw": processed_ratio,
                "clipped_over_raw": clipped_ratio
            })
        })
        .collect();
    compression_rows.sort_by(|a, b| {
        let ar = a
            .get("processed_over_raw")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        let br = b
            .get("processed_over_raw")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        ar.partial_cmp(&br).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut anomalies: Vec<String> = Vec::new();
    if let Some((tool, avg)) = top_dur.first() {
        if *avg > max_ms / 2 {
            anomalies.push(format!(
                "High latency concentration: {tool} avg_duration_ms={avg}"
            ));
        }
    }
    if let Some((tool, avg)) = top_eff.first() {
        if *avg > max_eff / 2 {
            anomalies.push(format!(
                "High token load concentration: {tool} avg_effective_input_tokens={avg}"
            ));
        }
    }
    if let (Some(a), Some(b)) = (first_cache, second_cache) {
        if b + 0.05 < a {
            anomalies.push(format!(
                "Cache hit degraded: first_half={}%, second_half={}%",
                (a * 100.0).round() as i64,
                (b * 100.0).round() as i64
            ));
        }
    }
    if let Some(freq) = schema_fail_freq {
        if freq > 0.05 {
            anomalies.push(format!(
                "Schema failure frequency elevated: {}%",
                (freq * 100.0).round() as i64
            ));
        }
    }
    if let Some(freq) = clip_freq {
        if freq > 0.30 {
            anomalies.push(format!(
                "Budget clipping frequent: {}% of captured runs",
                (freq * 100.0).round() as i64
            ));
        }
    }

    let mut recommendations: Vec<String> = Vec::new();
    if let Some((tool, avg_eff)) = top_eff.first() {
        recommendations.push(format!(
            "{tool} exceeds average token threshold ({avg_eff}); recommend lean mode."
        ));
    }
    if let (Some(a), Some(b)) = (first_cache, second_cache) {
        if b + 0.05 < a {
            recommendations.push("Cache hit rate degraded; inspect prompt drift.".to_string());
        }
    }
    if schema_fails > 0 {
        let tool = top_eff
            .first()
            .map(|v| v.0.clone())
            .unwrap_or_else(|| "schema command".to_string());
        recommendations.push(format!(
            "Schema failures detected for {tool}; enforce deterministic mode."
        ));
    }
    if recommendations.is_empty() {
        recommendations.push("No significant anomalies in this window.".to_string());
    }

    Ok(json!({
        "window": n,
        "runs": total,
        "scoreboard": {
            "runs": total,
            "alerts": alerts,
            "alerts_pct": if total == 0 { 0.0 } else { (alerts as f64 / total as f64) * 100.0 },
            "top_avg_duration_ms": top_dur,
            "top_avg_effective_input_tokens": top_eff,
            "cache_hit_rate": cache_all,
            "cache_hit_trend": {
                "first_half": first_cache,
                "second_half": second_cache,
                "delta": match (first_cache, second_cache) {
                    (Some(a), Some(b)) => Some(b - a),
                    _ => None
                }
            },
            "schema_failure_frequency": {
                "schema_runs": schema_total,
                "schema_failures": schema_fails,
                "rate": schema_fail_freq
            },
            "capture_provider_compression": compression_rows,
            "budget_clipping_frequency": {
                "captured_runs": clipped_total,
                "clipped_runs": clipped_count,
                "rate": clip_freq
            }
        },
        "anomalies": anomalies,
        "recommendations": recommendations,
        "log_file": log_file.display().to_string()
    }))
}

fn print_optimize(n: usize, json_out: bool) -> i32 {
    let report = match optimize_report(n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs optimize: {e}");
            return 1;
        }
    };
    if json_out {
        println!("{}", report);
        return 0;
    }

    println!("== cxrs optimize (last {n} runs) ==");
    println!("Section A: Scoreboard");
    let sb = report
        .get("scoreboard")
        .cloned()
        .unwrap_or_else(|| json!({}));
    println!(
        "runs: {}",
        sb.get("runs").and_then(Value::as_u64).unwrap_or(0)
    );
    println!(
        "alerts: {}",
        sb.get("alerts").and_then(Value::as_u64).unwrap_or(0)
    );
    if let Some(v) = sb.get("cache_hit_rate").and_then(Value::as_f64) {
        println!("cache_hit_rate: {}%", (v * 100.0).round() as i64);
    } else {
        println!("cache_hit_rate: n/a");
    }
    if let Some(tr) = sb.get("cache_hit_trend") {
        let a = tr.get("first_half").and_then(Value::as_f64);
        let b = tr.get("second_half").and_then(Value::as_f64);
        match (a, b) {
            (Some(x), Some(y)) => println!(
                "cache_trend: first_half={}%, second_half={}%, delta={}pp",
                (x * 100.0).round() as i64,
                (y * 100.0).round() as i64,
                ((y - x) * 100.0).round() as i64
            ),
            _ => println!("cache_trend: n/a"),
        }
    }
    println!("top_by_avg_duration_ms:");
    if let Some(arr) = sb.get("top_avg_duration_ms").and_then(Value::as_array) {
        for row in arr {
            if let Some(pair) = row.as_array() {
                if pair.len() == 2 {
                    println!(
                        "- {}: {}ms",
                        pair[0].as_str().unwrap_or("unknown"),
                        pair[1].as_u64().unwrap_or(0)
                    );
                }
            }
        }
    }
    println!("top_by_avg_effective_tokens:");
    if let Some(arr) = sb
        .get("top_avg_effective_input_tokens")
        .and_then(Value::as_array)
    {
        for row in arr {
            if let Some(pair) = row.as_array() {
                if pair.len() == 2 {
                    println!(
                        "- {}: {}",
                        pair[0].as_str().unwrap_or("unknown"),
                        pair[1].as_u64().unwrap_or(0)
                    );
                }
            }
        }
    }
    if let Some(c) = sb.get("budget_clipping_frequency") {
        let rate = c.get("rate").and_then(Value::as_f64);
        match rate {
            Some(r) => println!("budget_clipping_frequency: {}%", (r * 100.0).round() as i64),
            None => println!("budget_clipping_frequency: n/a"),
        }
    }
    println!("capture_provider_compression:");
    if let Some(arr) = sb
        .get("capture_provider_compression")
        .and_then(Value::as_array)
    {
        if arr.is_empty() {
            println!("- n/a");
        } else {
            for row in arr {
                let provider = row
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let ratio = row
                    .get("processed_over_raw")
                    .and_then(Value::as_f64)
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "n/a".to_string());
                println!("- {provider}: processed/raw={ratio}");
            }
        }
    }

    println!();
    println!("Section B: Anomaly Alerts");
    if let Some(arr) = report.get("anomalies").and_then(Value::as_array) {
        if arr.is_empty() {
            println!("- none");
        } else {
            for a in arr {
                println!("- {}", a.as_str().unwrap_or(""));
            }
        }
    }

    println!();
    println!("Section C: Actionable Recommendations");
    if let Some(arr) = report.get("recommendations").and_then(Value::as_array) {
        for r in arr {
            println!("- {}", r.as_str().unwrap_or(""));
        }
    }
    println!(
        "log_file: {}",
        report
            .get("log_file")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>")
    );
    0
}

fn print_worklog(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("# cxrs Worklog");
        println!();
        println!("Window: last {n} runs");
        println!();
        println!("No runs found.");
        println!();
        println!("_log_file: {}_", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs worklog: {e}");
            return 1;
        }
    };

    println!("# cxrs Worklog");
    println!();
    println!("Window: last {n} runs");
    println!();

    let mut by_tool: HashMap<String, (u64, u64, u64)> = HashMap::new();
    for r in &runs {
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let entry = by_tool.entry(tool).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 += r.duration_ms.unwrap_or(0);
        entry.2 += r.effective_input_tokens.unwrap_or(0);
    }

    let mut grouped: Vec<(String, u64, u64, u64)> = by_tool
        .into_iter()
        .map(|(tool, (count, sum_dur, sum_eff))| {
            let avg_dur = if count == 0 { 0 } else { sum_dur / count };
            let avg_eff = if count == 0 { 0 } else { sum_eff / count };
            (tool, count, avg_dur, avg_eff)
        })
        .collect();
    grouped.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));

    println!("## By Tool");
    println!();
    println!("| Tool | Runs | Avg Duration (ms) | Avg Effective Tokens |");
    println!("|---|---:|---:|---:|");
    for (tool, count, avg_dur, avg_eff) in grouped {
        println!("| {tool} | {count} | {avg_dur} | {avg_eff} |");
    }
    println!();

    println!("## Chronological Runs");
    println!();
    for r in &runs {
        let ts = r.ts.clone().unwrap_or_else(|| "n/a".to_string());
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let dur = r.duration_ms.unwrap_or(0);
        let eff = r.effective_input_tokens.unwrap_or(0);
        println!("- {ts} | {tool} | {dur}ms | {eff} effective tokens");
    }
    println!();
    println!("_log_file: {}_", log_file.display());
    0
}

fn show_field<T: ToString>(label: &str, value: Option<T>) {
    match value {
        Some(v) => println!("{label}: {}", v.to_string()),
        None => println!("{label}: n/a"),
    }
}

fn print_trace(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        eprintln!("cxrs trace: no log file at {}", log_file.display());
        return 1;
    }

    let runs = match load_runs(&log_file, usize::MAX) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs trace: {e}");
            return 1;
        }
    };
    if runs.is_empty() {
        eprintln!("cxrs trace: no runs in {}", log_file.display());
        return 1;
    }
    if n == 0 || n > runs.len() {
        eprintln!(
            "cxrs trace: run index out of range (requested {}, available {})",
            n,
            runs.len()
        );
        return 2;
    }
    let idx = runs.len() - n;
    let run = runs.get(idx).cloned().unwrap_or_default();

    println!("== cxrs trace (run #{n} most recent) ==");
    show_field("ts", run.ts);
    show_field("tool", run.tool);
    show_field("cwd", run.cwd);
    show_field("duration_ms", run.duration_ms);
    show_field("input_tokens", run.input_tokens);
    show_field("cached_input_tokens", run.cached_input_tokens);
    show_field("effective_input_tokens", run.effective_input_tokens);
    show_field("output_tokens", run.output_tokens);
    show_field("scope", run.scope);
    show_field("repo_root", run.repo_root);
    show_field("llm_backend", run.llm_backend);
    show_field("llm_model", run.llm_model);
    show_field("prompt_sha256", run.prompt_sha256);
    show_field("prompt_preview", run.prompt_preview);
    println!("log_file: {}", log_file.display());
    0
}

fn extract_agent_text(jsonl: &str) -> Option<String> {
    let mut last: Option<String> = None;
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let is_item_completed = v.get("type").and_then(Value::as_str) == Some("item.completed");
        if !is_item_completed {
            continue;
        }
        let item = v.get("item")?;
        if item.get("type").and_then(Value::as_str) != Some("agent_message") {
            continue;
        }
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            last = Some(text.to_string());
        }
    }
    last
}

fn run_codex_jsonl(prompt: &str) -> Result<String, String> {
    let mut child = Command::new("codex")
        .args(["exec", "--json", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to start codex: {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| format!("failed writing prompt to codex stdin: {e}"))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for codex: {e}"))?;

    if !out.status.success() {
        return Err(format!("codex exited with status {}", out.status));
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn run_codex_plain(prompt: &str) -> Result<String, String> {
    let mut child = Command::new("codex")
        .args(["exec", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to start codex: {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| format!("failed writing prompt to codex stdin: {e}"))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for codex: {e}"))?;
    if !out.status.success() {
        return Err(format!("codex exited with status {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn run_ollama_plain(prompt: &str) -> Result<String, String> {
    let model = resolve_ollama_model_for_run()?;
    let mut child = Command::new("ollama")
        .args(["run", &model])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to start ollama: {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| format!("failed writing prompt to ollama stdin: {e}"))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for ollama: {e}"))?;
    if !out.status.success() {
        return Err(format!("ollama exited with status {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn run_llm_plain(prompt: &str) -> Result<String, String> {
    if llm_backend() == "ollama" {
        run_ollama_plain(prompt)
    } else {
        run_codex_plain(prompt)
    }
}

fn run_llm_jsonl(prompt: &str) -> Result<String, String> {
    if llm_backend() != "ollama" {
        return run_codex_jsonl(prompt);
    }
    let text = run_ollama_plain(prompt)?;
    let wrapped = json!({
      "type":"item.completed",
      "item":{"type":"agent_message","text":text}
    });
    serde_json::to_string(&wrapped)
        .map_err(|e| format!("failed to serialize ollama JSONL wrapper: {e}"))
}

fn build_strict_schema_prompt(schema: &str, task_input: &str) -> String {
    if env::var("CX_SCHEMA_RELAXED").ok().as_deref() == Some("1") {
        return format!(
            "You are a structured output generator.\nReturn JSON ONLY. No markdown. No prose. No code fences.\nOutput MUST be a single valid JSON object matching the schema.\nSchema:\n{schema}\n\nTask input:\n{task_input}\n"
        );
    }
    format!(
        "You are a structured output generator.\nReturn STRICT JSON ONLY. No markdown. No prose. No code fences.\nOutput MUST be a single valid JSON object matching the schema.\nSchema-strict mode: deterministic JSON only; reject ambiguity.\nSchema:\n{schema}\n\nTask input:\n{task_input}\n"
    )
}

#[derive(Debug, Clone)]
struct BudgetConfig {
    budget_chars: usize,
    budget_lines: usize,
    clip_mode: String,
    clip_footer: bool,
}

fn budget_config_from_env() -> BudgetConfig {
    BudgetConfig {
        budget_chars: env::var("CX_CONTEXT_BUDGET_CHARS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(12000),
        budget_lines: env::var("CX_CONTEXT_BUDGET_LINES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(300),
        clip_mode: env::var("CX_CONTEXT_CLIP_MODE").unwrap_or_else(|_| "smart".to_string()),
        clip_footer: env::var("CX_CONTEXT_CLIP_FOOTER")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(1)
            == 1,
    }
}

fn is_rtk_supported_prefix(cmd0: &str) -> bool {
    matches!(
        cmd0,
        "git" | "diff" | "ls" | "tree" | "grep" | "test" | "log" | "read"
    )
}

fn normalize_generic(input: &str) -> String {
    let mut out = String::new();
    let mut blank_seen = false;
    for mut line in input.lines().map(|l| l.to_string()) {
        if line.trim().is_empty() {
            if !blank_seen {
                out.push('\n');
            }
            blank_seen = true;
            continue;
        }
        blank_seen = false;
        if line.chars().count() > 600 {
            line = format!("{}...", line.chars().take(600).collect::<String>());
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn reduce_git_status(input: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in input.lines() {
        let t = line.trim_start();
        if line.starts_with("On branch ")
            || line.starts_with("HEAD detached")
            || line.starts_with("Your branch ")
            || line.starts_with("Changes to be committed:")
            || line.starts_with("Changes not staged for commit:")
            || line.starts_with("Untracked files:")
            || line.starts_with("nothing to commit")
            || line.starts_with("no changes added to commit")
            || t.starts_with("modified:")
            || t.starts_with("new file:")
            || t.starts_with("deleted:")
            || t.starts_with("renamed:")
            || t.starts_with("both modified:")
            || t.starts_with("both added:")
            || t.starts_with("both deleted:")
        {
            out.push(line.to_string());
        }
    }
    if out.is_empty() {
        input
            .lines()
            .take(120)
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        out.join("\n")
    }
}

fn reduce_diff_like(input: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut changed = 0usize;
    for line in input.lines() {
        if line.starts_with("diff --git ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("@@ ")
            || line.starts_with("Binary files ")
            || line.starts_with("rename from ")
            || line.starts_with("rename to ")
        {
            out.push(line.to_string());
        } else if (line.starts_with('+') || line.starts_with('-')) && changed < 300 {
            out.push(line.to_string());
            changed += 1;
        }
    }
    if out.is_empty() {
        input.to_string()
    } else {
        out.join("\n")
    }
}

fn native_reduce_output(cmd: &[String], input: &str) -> String {
    let cmd0 = cmd.first().map(String::as_str).unwrap_or("");
    let cmd1 = cmd.get(1).map(String::as_str).unwrap_or("");
    let reduced = match (cmd0, cmd1) {
        ("git", "status") => reduce_git_status(input),
        ("git", "diff") | ("diff", _) => reduce_diff_like(input),
        _ => input.to_string(),
    };
    normalize_generic(&reduced)
}

fn choose_clip_mode(input: &str, configured_mode: &str) -> String {
    match configured_mode {
        "head" => "head".to_string(),
        "tail" => "tail".to_string(),
        _ => {
            let lower = input.to_lowercase();
            if lower.contains("error") || lower.contains("fail") || lower.contains("warning") {
                "tail".to_string()
            } else {
                "head".to_string()
            }
        }
    }
}

fn first_n_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn last_n_chars(s: &str, n: usize) -> String {
    let total = s.chars().count();
    if n >= total {
        return s.to_string();
    }
    s.chars().skip(total - n).collect()
}

fn clip_text_with_config(input: &str, cfg: &BudgetConfig) -> (String, CaptureStats) {
    let original_chars = input.chars().count();
    let original_lines = input.lines().count();
    let mode_used = choose_clip_mode(input, &cfg.clip_mode);
    let lines: Vec<&str> = input.lines().collect();
    let line_limited = if lines.len() <= cfg.budget_lines {
        input.to_string()
    } else if mode_used == "tail" {
        lines[lines.len().saturating_sub(cfg.budget_lines)..].join("\n")
    } else {
        lines[..cfg.budget_lines].join("\n")
    };
    let char_limited = if line_limited.chars().count() <= cfg.budget_chars {
        line_limited
    } else if mode_used == "tail" {
        last_n_chars(&line_limited, cfg.budget_chars)
    } else {
        first_n_chars(&line_limited, cfg.budget_chars)
    };
    let kept_chars = char_limited.chars().count();
    let kept_lines = char_limited.lines().count();
    let clipped = kept_chars < original_chars || kept_lines < original_lines;
    let final_text = if clipped && cfg.clip_footer {
        format!(
            "{char_limited}\n[cx] output clipped: original={}/{}, kept={}/{}, mode={}",
            original_chars, original_lines, kept_chars, kept_lines, mode_used
        )
    } else {
        char_limited
    };
    (
        final_text,
        CaptureStats {
            system_output_len_raw: Some(original_chars as u64),
            system_output_len_processed: Some(input.chars().count() as u64),
            system_output_len_clipped: Some(kept_chars as u64),
            system_output_lines_raw: Some(original_lines as u64),
            system_output_lines_processed: Some(input.lines().count() as u64),
            system_output_lines_clipped: Some(kept_lines as u64),
            clipped: Some(clipped),
            budget_chars: Some(cfg.budget_chars as u64),
            budget_lines: Some(cfg.budget_lines as u64),
            clip_mode: Some(mode_used),
            clip_footer: Some(cfg.clip_footer),
            rtk_used: None,
            capture_provider: None,
        },
    )
}

fn should_use_rtk(
    cmd: &[String],
    provider_mode: &str,
    rtk_enabled: bool,
    rtk_usable: bool,
) -> bool {
    let supported = cmd
        .first()
        .map(|c| is_rtk_supported_prefix(c))
        .unwrap_or(false);
    match provider_mode {
        "rtk" => rtk_enabled && supported && rtk_usable,
        "native" => false,
        _ => rtk_enabled && supported && rtk_usable,
    }
}

fn run_system_command_capture(cmd: &[String]) -> Result<(String, i32, CaptureStats), String> {
    if cmd.is_empty() {
        return Err("missing command".to_string());
    }

    fn run_capture(command: &[String]) -> Result<(String, i32), String> {
        if command.is_empty() {
            return Err("missing command".to_string());
        }
        let mut c = Command::new(&command[0]);
        if command.len() > 1 {
            c.args(&command[1..]);
        }
        let output = c
            .output()
            .map_err(|e| format!("failed to execute '{}': {e}", command[0]))?;
        let status = output.status.code().unwrap_or(1);
        let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !stderr.trim().is_empty() {
            if !combined.is_empty() && !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&stderr);
        }
        Ok((combined, status))
    }

    let (raw_out, status) = run_capture(cmd)?;
    let rtk_enabled = env::var("CX_RTK_SYSTEM")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let native_reduce = env::var("CX_NATIVE_REDUCE")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let provider_mode = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "auto".to_string());
    let provider_mode = match provider_mode.as_str() {
        "rtk" | "native" | "auto" => provider_mode,
        _ => "auto".to_string(),
    };
    let rtk_usable = rtk_is_usable();
    let use_rtk = should_use_rtk(cmd, &provider_mode, rtk_enabled, rtk_usable);
    let mut provider_used = if use_rtk { "rtk" } else { "native" }.to_string();
    let processed = if use_rtk {
        let mut rtk_cmd = vec!["rtk".to_string()];
        rtk_cmd.extend_from_slice(cmd);
        match run_capture(&rtk_cmd) {
            Ok((rtk_out, rtk_status)) if rtk_status == 0 && !rtk_out.trim().is_empty() => rtk_out,
            _ => {
                provider_used = "native".to_string();
                raw_out.clone()
            }
        }
    } else {
        raw_out.clone()
    };
    let reduced = if native_reduce {
        native_reduce_output(cmd, &processed)
    } else {
        processed
    };
    let (clipped_text, mut stats) = clip_text_with_config(&reduced, &budget_config_from_env());
    stats.rtk_used = Some(provider_used == "rtk");
    stats.capture_provider = Some(provider_used);
    Ok((clipped_text, status, stats))
}

fn execute_task(spec: TaskSpec) -> Result<ExecutionResult, String> {
    let started = Instant::now();
    let execution_id = make_execution_id(&spec.command_name);

    let (prompt, capture_stats, system_status) = match spec.input {
        TaskInput::Prompt(p) => (p, CaptureStats::default(), None),
        TaskInput::SystemCommand(cmd) => {
            let (captured, status, stats) = run_system_command_capture(&cmd)?;
            (captured, stats, Some(status))
        }
    };
    let capture_stats = spec.capture_override.unwrap_or(capture_stats);

    let mut schema_valid: Option<bool> = None;
    let mut quarantine_id: Option<String> = None;
    let mut usage = UsageStats::default();
    let stdout: String;
    let stderr = String::new();

    match spec.output_kind {
        LlmOutputKind::Plain => {
            stdout = run_llm_plain(&prompt)?;
        }
        LlmOutputKind::Jsonl => {
            let jsonl = run_llm_jsonl(&prompt)?;
            usage = usage_from_jsonl(&jsonl);
            stdout = jsonl;
        }
        LlmOutputKind::AgentText => {
            let jsonl = run_llm_jsonl(&prompt)?;
            usage = usage_from_jsonl(&jsonl);
            stdout = extract_agent_text(&jsonl).unwrap_or_default();
        }
        LlmOutputKind::SchemaJson => {
            let schema = spec
                .schema
                .as_ref()
                .ok_or_else(|| "schema execution missing schema".to_string())?;
            let task_input = spec
                .schema_task_input
                .as_deref()
                .unwrap_or(&prompt)
                .to_string();
            let schema_pretty = serde_json::to_string_pretty(&schema.value)
                .unwrap_or_else(|_| schema.value.to_string());
            let retry_allowed = env::var("CX_SCHEMA_RELAXED").ok().as_deref() != Some("1");
            let mut attempts: Vec<QuarantineAttempt> = Vec::new();
            let mut final_reason: Option<String> = None;
            let mut last_full_prompt = build_strict_schema_prompt(&schema_pretty, &task_input);

            let run_attempt = |full_prompt: &str| -> Result<(String, UsageStats), String> {
                let jsonl = run_llm_jsonl(full_prompt)?;
                let usage = usage_from_jsonl(&jsonl);
                let raw = extract_agent_text(&jsonl).unwrap_or_default();
                Ok((raw, usage))
            };

            let validate_raw = |raw: &str| -> Result<Value, String> {
                if raw.trim().is_empty() {
                    return Err("empty_agent_message".to_string());
                }
                validate_schema_instance(schema, raw)
            };

            // Attempt 1
            let full_prompt1 = last_full_prompt.clone();
            let (raw1, usage1) = run_attempt(&full_prompt1)?;
            usage = usage1.clone();
            match validate_raw(&raw1) {
                Ok(valid) => {
                    schema_valid = Some(true);
                    stdout = valid.to_string();
                }
                Err(reason1) => {
                    final_reason = Some(reason1.clone());
                    attempts.push(QuarantineAttempt {
                        reason: reason1.clone(),
                        prompt: full_prompt1.clone(),
                        prompt_sha256: sha256_hex(&full_prompt1),
                        raw_response: raw1.clone(),
                        raw_sha256: sha256_hex(&raw1),
                    });

                    // Attempt 2 (bounded retry) before quarantine
                    if retry_allowed {
                        let full_prompt2 = format!(
                            "You are a structured output generator.\nReturn STRICT JSON ONLY. No markdown. No prose. No code fences.\nOutput MUST be a single valid JSON object matching the schema.\n\nPrevious attempt failed validation: {reason1}\n\nSchema:\n{schema_pretty}\n\nTask input:\n{task_input}\n"
                        );
                        last_full_prompt = full_prompt2.clone();
                        let (raw2, usage2) = run_attempt(&full_prompt2)?;
                        usage = usage2;
                        match validate_raw(&raw2) {
                            Ok(valid) => {
                                schema_valid = Some(true);
                                stdout = valid.to_string();
                                final_reason = None;
                                // No quarantine when retry succeeds.
                            }
                            Err(reason2) => {
                                final_reason = Some(reason2.clone());
                                attempts.push(QuarantineAttempt {
                                    reason: reason2.clone(),
                                    prompt: full_prompt2.clone(),
                                    prompt_sha256: sha256_hex(&full_prompt2),
                                    raw_response: raw2.clone(),
                                    raw_sha256: sha256_hex(&raw2),
                                });

                                let qid = log_schema_failure(
                                    &spec.command_name,
                                    &reason2,
                                    &raw2,
                                    &schema_pretty,
                                    &task_input,
                                    attempts.clone(),
                                )
                                .unwrap_or_else(|_| "".to_string());
                                schema_valid = Some(false);
                                quarantine_id = if qid.is_empty() { None } else { Some(qid) };
                                stdout = raw2;
                            }
                        }
                    } else {
                        let qid = log_schema_failure(
                            &spec.command_name,
                            &reason1,
                            &raw1,
                            &schema_pretty,
                            &task_input,
                            attempts.clone(),
                        )
                        .unwrap_or_else(|_| "".to_string());
                        schema_valid = Some(false);
                        quarantine_id = if qid.is_empty() { None } else { Some(qid) };
                        stdout = raw1;
                    }
                }
            }
            // keep full prompt hash/logging correlation for schema tasks
            if spec.logging_enabled && logging_enabled() {
                let _ = log_codex_run(
                    &spec.command_name,
                    &last_full_prompt,
                    started.elapsed().as_millis() as u64,
                    Some(&usage),
                    Some(&capture_stats),
                    schema_valid.unwrap_or(false),
                    if schema_valid == Some(false) {
                        final_reason.as_deref().or(Some("schema_validation_failed"))
                    } else {
                        None
                    },
                    spec.schema.as_ref().map(|s| s.name.as_str()),
                    quarantine_id.as_deref(),
                    None,
                    None,
                );
            }
            return Ok(ExecutionResult {
                stdout,
                stderr,
                duration_ms: started.elapsed().as_millis() as u64,
                schema_valid,
                quarantine_id,
                capture_stats,
                execution_id,
                usage,
                system_status,
            });
        }
    }

    if spec.logging_enabled && logging_enabled() {
        let schema_name = spec.schema.as_ref().map(|s| s.name.as_str());
        let _ = log_codex_run(
            &spec.command_name,
            &prompt,
            started.elapsed().as_millis() as u64,
            Some(&usage),
            Some(&capture_stats),
            schema_valid.unwrap_or(true),
            None,
            schema_name,
            quarantine_id.as_deref(),
            None,
            None,
        );
    }

    Ok(ExecutionResult {
        stdout,
        stderr,
        duration_ms: started.elapsed().as_millis() as u64,
        schema_valid,
        quarantine_id,
        capture_stats,
        execution_id,
        usage,
        system_status,
    })
}

fn run_command_for_bench(
    command: &[String],
    disable_cx_log: bool,
    passthru: bool,
) -> Result<i32, String> {
    if command.is_empty() {
        return Err("missing command".to_string());
    }
    let mut cmd = Command::new(&command[0]);
    if command.len() > 1 {
        cmd.args(&command[1..]);
    }
    if disable_cx_log {
        cmd.env("CXLOG_ENABLED", "0");
    }
    let output = cmd
        .output()
        .map_err(|e| format!("failed to execute '{}': {e}", command[0]))?;
    if passthru {
        let mut out = std::io::stdout();
        let mut err = std::io::stderr();
        let _ = out.write_all(&output.stdout);
        let _ = err.write_all(&output.stderr);
    }
    Ok(output.status.code().unwrap_or(1))
}

fn cmd_bench(runs: usize, command: &[String]) -> i32 {
    if runs == 0 {
        eprintln!("cxrs bench: runs must be > 0");
        return 2;
    }
    if command.is_empty() {
        eprintln!("Usage: {APP_NAME} bench <runs> -- <command...>");
        return 2;
    }

    let disable_cx_log = env::var("CXBENCH_LOG").ok().as_deref() == Some("0");
    let passthru = env::var("CXBENCH_PASSTHRU").ok().as_deref() == Some("1");
    let log_file = resolve_log_file();
    let mut durations: Vec<u64> = Vec::with_capacity(runs);
    let mut eff_totals: Vec<u64> = Vec::new();
    let mut out_totals: Vec<u64> = Vec::new();
    let mut failures = 0usize;
    let mut prompt_hash_matched = 0usize;
    let mut appended_row_total = 0usize;

    for _ in 0..runs {
        let before_offset = if let Some(path) = &log_file {
            if path.exists() { file_len(path) } else { 0 }
        } else {
            0
        };

        let started = Instant::now();
        let started_epoch = Utc::now().timestamp();
        let code = match run_command_for_bench(command, disable_cx_log, passthru) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("cxrs bench: {e}");
                return 1;
            }
        };
        let ended_epoch = Utc::now().timestamp();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        durations.push(elapsed_ms);
        if code != 0 {
            failures += 1;
        }

        if let Some(path) = &log_file {
            if path.exists() && !disable_cx_log {
                let appended = load_runs_appended(path, before_offset).unwrap_or_default();
                if !appended.is_empty() {
                    let windowed: Vec<RunEntry> = appended
                        .into_iter()
                        .filter(|r| {
                            let Some(ts) = r.ts.as_deref() else {
                                return true;
                            };
                            let Some(epoch) = parse_ts_epoch(ts) else {
                                return true;
                            };
                            epoch >= started_epoch.saturating_sub(1)
                                && epoch <= ended_epoch.saturating_add(1)
                        })
                        .collect();
                    appended_row_total += windowed.len();

                    let mut hash_counts: HashMap<String, usize> = HashMap::new();
                    for r in &windowed {
                        if let Some(h) = r.prompt_sha256.as_deref() {
                            if !h.is_empty() {
                                *hash_counts.entry(h.to_string()).or_insert(0) += 1;
                            }
                        }
                    }

                    let preferred_hash = hash_counts.into_iter().max_by(|a, b| a.1.cmp(&b.1));
                    let correlated: Vec<&RunEntry> = if let Some((h, _)) = preferred_hash {
                        prompt_hash_matched += 1;
                        windowed
                            .iter()
                            .filter(|r| r.prompt_sha256.as_deref() == Some(h.as_str()))
                            .collect()
                    } else {
                        windowed.iter().collect()
                    };

                    if !correlated.is_empty() {
                        let mut eff_sum = 0u64;
                        let mut out_sum = 0u64;
                        let mut any_eff = false;
                        let mut any_out = false;
                        for r in correlated {
                            if let Some(v) = r.effective_input_tokens {
                                eff_sum += v;
                                any_eff = true;
                            }
                            if let Some(v) = r.output_tokens {
                                out_sum += v;
                                any_out = true;
                            }
                        }
                        if any_eff {
                            eff_totals.push(eff_sum);
                        }
                        if any_out {
                            out_totals.push(out_sum);
                        }
                    }
                }
            }
        }
    }

    let min = durations.iter().min().copied().unwrap_or(0);
    let max = durations.iter().max().copied().unwrap_or(0);
    let sum: u64 = durations.iter().sum();
    let avg = if durations.is_empty() {
        0
    } else {
        sum / (durations.len() as u64)
    };

    println!("== cxrs bench ==");
    println!("runs: {runs}");
    println!("command: {}", command.join(" "));
    println!("duration_ms avg/min/max: {avg}/{min}/{max}");
    println!("failures: {failures}");
    if !eff_totals.is_empty() {
        let eff_avg = eff_totals.iter().sum::<u64>() / (eff_totals.len() as u64);
        println!("avg effective_input_tokens: {eff_avg}");
    } else {
        println!("avg effective_input_tokens: n/a");
    }
    if !out_totals.is_empty() {
        let out_avg = out_totals.iter().sum::<u64>() / (out_totals.len() as u64);
        println!("avg output_tokens: {out_avg}");
    } else {
        println!("avg output_tokens: n/a");
    }
    if disable_cx_log {
        println!("cxbench_log: disabled (CXBENCH_LOG=0)");
    } else {
        println!("cxbench_log: enabled");
    }
    println!(
        "cxbench_passthru: {}",
        if passthru { "enabled" } else { "disabled" }
    );
    if !disable_cx_log {
        println!(
            "cxbench_correlation: prompt_hash_matches={}/{} runs, appended_rows={}",
            prompt_hash_matched, runs, appended_row_total
        );
    }

    if failures > 0 { 1 } else { 0 }
}

fn validate_schema_instance(schema: &LoadedSchema, raw: &str) -> Result<Value, String> {
    let instance: Value = serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;
    let compiled = {
        let mut lock = SCHEMA_COMPILED_CACHE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .map_err(|_| "schema cache poisoned".to_string())?;
        if let Some(existing) = lock.get(&schema.name) {
            existing.clone()
        } else {
            let compiled = JSONSchema::compile(&schema.value)
                .map_err(|e| format!("failed to compile schema {}: {e}", schema.path.display()))?;
            let compiled = Arc::new(compiled);
            lock.insert(schema.name.clone(), compiled.clone());
            compiled
        }
    };
    if let Err(errors) = compiled.validate(&instance) {
        let mut reasons: Vec<String> = Vec::new();
        for err in errors.take(3) {
            reasons.push(err.to_string());
        }
        let reason = if reasons.is_empty() {
            "schema_validation_failed".to_string()
        } else {
            format!("schema_validation_failed: {}", reasons.join(" | "))
        };
        return Err(reason);
    }
    Ok(instance)
}

fn parse_commands_array(raw: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;
    let arr = v
        .get("commands")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing required key 'commands' array".to_string())?;
    let mut out: Vec<String> = Vec::new();
    for item in arr {
        let Some(s) = item.as_str() else {
            return Err("commands array must contain strings".to_string());
        };
        if !s.trim().is_empty() {
            out.push(s.to_string());
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
enum SafetyDecision {
    Safe,
    Dangerous(String),
}

fn normalize_token(tok: &str) -> String {
    tok.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == ';' || c == ',')
        .to_string()
}

fn command_has_write_pattern(lower: &str) -> bool {
    lower.contains(">>")
        || lower.contains(">")
        || lower.contains("tee ")
        || lower.contains("touch ")
        || lower.contains("mkdir ")
        || lower.contains("cp ")
        || lower.contains("mv ")
        || lower.contains("install ")
        || lower.contains("dd ")
        || lower.contains("chmod ")
        || lower.contains("chown ")
}

fn write_targets_outside_repo(cmd: &str, repo_root: &Path) -> bool {
    let root_s = repo_root.to_string_lossy().to_string();
    let tokens: Vec<String> = cmd.split_whitespace().map(normalize_token).collect();
    let mut candidates: Vec<String> = Vec::new();
    let last = tokens.last().cloned().unwrap_or_default();
    for i in 0..tokens.len() {
        let t = tokens[i].as_str();
        if t == ">" || t == ">>" || t == "tee" {
            if let Some(next) = tokens.get(i + 1) {
                candidates.push(next.clone());
            }
        }
        if t == "touch" || t == "mkdir" || t == "chmod" || t == "chown" {
            if let Some(next) = tokens.get(i + 1) {
                candidates.push(next.clone());
            }
        }
        if let Some(path) = t.strip_prefix("of=") {
            candidates.push(path.to_string());
        }
        if t.starts_with('/') {
            candidates.push(t.to_string());
        }
        if t.starts_with("~/") || t == "~" || t.starts_with("$HOME") || t.starts_with("${HOME}") {
            candidates.push(t.to_string());
        }
    }
    // For cp/mv/install, treat the last argument as destination.
    if tokens.iter().any(|t| t == "cp" || t == "mv" || t == "install") && !last.is_empty() {
        candidates.push(last);
    }
    candidates.into_iter().any(|p| {
        let p = p.trim().to_string();
        if p.is_empty() {
            return false;
        }
        // Any parent traversal in a write target is treated as unsafe (can escape repo root).
        if p.contains("..") {
            return true;
        }
        if p.starts_with("~/") || p == "~" || p.starts_with("$HOME") || p.starts_with("${HOME}") {
            return true;
        }
        if !p.starts_with('/') {
            return false;
        }
        if p.starts_with(&(root_s.clone() + "/")) || p == root_s {
            return false;
        }
        true
    })
}

fn evaluate_command_safety(cmd: &str, repo_root: &Path) -> SafetyDecision {
    let compact = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = compact.to_lowercase();

    if lower.contains(" sudo ") || lower.starts_with("sudo ") || lower.ends_with(" sudo") {
        return SafetyDecision::Dangerous("contains sudo".to_string());
    }
    if lower.contains("rm -rf")
        || lower.contains("rm -fr")
        || lower.contains("rm -r -f")
        || lower.contains("rm -f -r")
    {
        return SafetyDecision::Dangerous("contains rm -rf pattern".to_string());
    }
    if lower.contains("curl ")
        && lower.contains('|')
        && (lower.contains("| bash") || lower.contains("| sh") || lower.contains("| zsh"))
    {
        return SafetyDecision::Dangerous("contains curl pipe shell pattern".to_string());
    }
    if (lower.contains("chmod ") || lower.contains("chown "))
        && (lower.contains("/system") || lower.contains("/library") || lower.contains("/usr"))
        && !lower.contains("/usr/local")
    {
        return SafetyDecision::Dangerous("chmod/chown on protected system path".to_string());
    }
    if (lower.contains("chmod ") || lower.contains("chown "))
        && command_has_write_pattern(&lower)
        && write_targets_outside_repo(&compact, repo_root)
    {
        return SafetyDecision::Dangerous("chmod/chown target outside repo root".to_string());
    }
    if (lower.contains("> /system")
        || lower.contains(">> /system")
        || lower.contains("> /library")
        || lower.contains(">> /library")
        || lower.contains("> /usr")
        || lower.contains(">> /usr")
        || (lower.contains("tee ")
            && (lower.contains(" /system")
                || lower.contains(" /library")
                || lower.contains(" /usr"))))
        && !lower.contains("/usr/local")
    {
        return SafetyDecision::Dangerous("write redirection to protected system path".to_string());
    }
    if command_has_write_pattern(&lower) && write_targets_outside_repo(&compact, repo_root) {
        return SafetyDecision::Dangerous("write target outside repo root".to_string());
    }
    SafetyDecision::Safe
}

fn cmd_policy(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("check") {
        if args.len() < 2 {
            eprintln!("Usage: {APP_NAME} policy check <command...>");
            return 2;
        }
        let candidate = args[1..].join(" ");
        let root = repo_root()
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        match evaluate_command_safety(&candidate, &root) {
            SafetyDecision::Safe => println!("safe"),
            SafetyDecision::Dangerous(reason) => println!("dangerous: {reason}"),
        }
        return 0;
    }

    if args.first().map(String::as_str) == Some("show") || args.is_empty() {
        let unsafe_flag = env::var("CX_UNSAFE").ok().as_deref() == Some("1");
        let force = env::var("CXFIX_FORCE").ok().as_deref() == Some("1");
        println!("== cxrs policy show ==");
        println!("Active safety rules:");
        println!("- Block: sudo");
        println!("- Block: rm -rf family");
        println!("- Block: curl | bash/sh/zsh");
        println!("- Block: chmod/chown on /System,/Library,/usr (except /usr/local)");
        println!("- Block: write operations outside repo root");
        println!();
        println!("Unsafe override state:");
        println!(
            "--unsafe / CX_UNSAFE=1: {}",
            if unsafe_flag { "on" } else { "off" }
        );
        println!("CXFIX_FORCE=1: {}", if force { "on" } else { "off" });
        return 0;
    }

    println!("== cxrs policy ==");
    println!("Dangerous command patterns blocked by default in fix-run:");
    println!("- sudo (any)");
    println!("- rm -rf / rm -fr forms");
    println!("- curl | bash/sh/zsh");
    println!("- chmod/chown on /System, /Library, /usr (except /usr/local)");
    println!("- shell redirection/tee writes to /System, /Library, /usr (except /usr/local)");
    println!();
    println!("Overrides:");
    println!("- --unsafe          allow dangerous execution for current command");
    println!("- CXFIX_RUN=1       execute suggested commands");
    println!("- CXFIX_FORCE=1     allow dangerous commands");
    println!();
    println!("Examples:");
    println!("- {APP_NAME} policy check \"sudo rm -rf /tmp/foo\"");
    println!("- {APP_NAME} policy check \"chmod 755 /usr/local/bin/tool\"");
    0
}

fn print_roles() -> i32 {
    println!("== cxrs roles ==");
    println!("architect   Define approach, boundaries, and tradeoffs.");
    println!("implementer Apply focused code changes with minimal blast radius.");
    println!("reviewer    Validate regressions, risks, and missing tests.");
    println!("tester      Design and run deterministic checks.");
    println!("doc         Produce concise operator-facing documentation.");
    0
}

fn role_header(role: &str) -> Option<&'static str> {
    match role {
        "architect" => Some(
            "Role: architect\nFocus: design and decomposition.\nDeliver: implementation plan, constraints, and acceptance checks.",
        ),
        "implementer" => Some(
            "Role: implementer\nFocus: minimal cohesive code change.\nDeliver: patch summary and verification commands.",
        ),
        "reviewer" => Some(
            "Role: reviewer\nFocus: bugs, regressions, and safety.\nDeliver: findings ordered by severity with file references.",
        ),
        "tester" => Some(
            "Role: tester\nFocus: deterministic validation.\nDeliver: test matrix, observed results, and failure triage.",
        ),
        "doc" => Some(
            "Role: doc\nFocus: user/operator clarity.\nDeliver: concise docs with examples and expected outputs.",
        ),
        _ => None,
    }
}

fn cmd_roles(role: Option<&str>) -> i32 {
    if let Some(r) = role {
        let Some(header) = role_header(r) else {
            eprintln!("cxrs roles: unknown role '{r}'");
            return 2;
        };
        println!("{header}");
        return 0;
    }
    print_roles()
}

fn cmd_prompt(mode: &str, request: &str) -> i32 {
    let valid = ["implement", "fix", "test", "doc", "ops"];
    if !valid.contains(&mode) {
        eprintln!("cxrs prompt: invalid mode '{mode}' (use implement|fix|test|doc|ops)");
        return 2;
    }
    let mode_goal = match mode {
        "implement" => "Implement the requested behavior with minimal risk and clear verification.",
        "fix" => "Diagnose and fix the issue with root-cause focus and regression prevention.",
        "test" => "Design and execute deterministic tests that validate behavior and edge cases.",
        "doc" => "Produce concise, accurate documentation aligned with current implementation.",
        "ops" => "Perform operational changes safely, with rollback and observability guidance.",
        _ => "",
    };
    println!("You are working on the \"cx\" toolchain.");
    println!();
    println!("Context:");
    println!("- Repo canonical implementation is the source of truth.");
    println!("- Keep behavior deterministic and non-interactive.");
    println!("- Do not contaminate stdout pipelines; diagnostics to stderr.");
    println!();
    println!("Goal:");
    println!("- {}", mode_goal);
    println!("- User request: {request}");
    println!();
    println!("Requirements:");
    println!("- Preserve backward compatibility where feasible.");
    println!("- Make minimal cohesive changes.");
    println!("- Validate structured outputs when JSON is required.");
    println!();
    println!("Constraints:");
    println!("- No automatic commands on shell startup.");
    println!("- No global shell option leakage.");
    println!("- Keep repo-aware logging behavior intact.");
    println!();
    println!("Deliverables:");
    println!("- Code changes with file paths.");
    println!("- Short explanation of what changed and why.");
    println!("- Verification command list.");
    println!();
    println!("Test Checklist:");
    println!("1. Build/check passes.");
    println!("2. Target command behavior matches requirements.");
    println!("3. No pipeline-breaking stdout noise.");
    println!("4. Repo-aware log/state paths still resolve correctly.");
    0
}

fn cmd_fanout(objective: &str) -> i32 {
    let tasks = vec![
        (
            "architect",
            "Define minimal design and split objective into independent slices.",
        ),
        (
            "implementer",
            "Implement slice A with deterministic behavior and tests.",
        ),
        (
            "implementer",
            "Implement slice B with minimal shared-state coupling.",
        ),
        (
            "reviewer",
            "Audit for regressions, safety issues, and schema/pipeline risks.",
        ),
        (
            "tester",
            "Create execution checklist and validate outputs against expectations.",
        ),
        ("doc", "Update operator docs and examples for new behavior."),
    ];
    println!("== cxrs fanout ==");
    println!("objective: {objective}");
    println!();
    for (idx, (role, task)) in tasks.iter().enumerate() {
        println!("### Subtask {}/{} [{}]", idx + 1, tasks.len(), role);
        println!("Goal: {task}");
        println!("Scope: Keep this task independently executable.");
        println!("Deliverables: patch summary + verification commands.");
        println!("Tests: include deterministic checks for this slice.");
        println!();
    }
    0
}

fn cmd_promptlint(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("== cxrs promptlint (last {n} runs) ==");
        println!("No runs found.");
        println!("log_file: {}", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs promptlint: {e}");
            return 1;
        }
    };
    if runs.is_empty() {
        println!("== cxrs promptlint (last {n} runs) ==");
        println!("No runs found.");
        println!("log_file: {}", log_file.display());
        return 0;
    }

    let mut tool_eff: HashMap<String, (u64, u64)> = HashMap::new();
    let mut tool_cache: HashMap<String, (u64, u64)> = HashMap::new();
    for r in &runs {
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let eff = r.effective_input_tokens.unwrap_or(0);
        let e = tool_eff.entry(tool.clone()).or_insert((0, 0));
        e.0 += eff;
        e.1 += 1;
        let in_t = r.input_tokens.unwrap_or(0);
        let c_t = r.cached_input_tokens.unwrap_or(0);
        let c = tool_cache.entry(tool).or_insert((0, 0));
        c.0 += c_t;
        c.1 += in_t;
    }

    let mut top_eff: Vec<(String, u64)> = tool_eff
        .iter()
        .map(|(tool, (sum, count))| (tool.clone(), if *count == 0 { 0 } else { sum / count }))
        .collect();
    top_eff.sort_by(|a, b| b.1.cmp(&a.1));
    top_eff.truncate(5);

    let mid = runs.len() / 2;
    let (first, second) = runs.split_at(mid.max(1).min(runs.len()));
    let mut drift_rows: Vec<(String, i64, u64, u64)> = Vec::new();
    let mut tools: Vec<String> = tool_eff.keys().cloned().collect();
    tools.sort();
    tools.dedup();
    for tool in tools {
        let mut f_sum = 0u64;
        let mut f_count = 0u64;
        for r in first {
            if r.tool.as_deref().unwrap_or("unknown") == tool {
                f_sum += r.effective_input_tokens.unwrap_or(0);
                f_count += 1;
            }
        }
        let mut s_sum = 0u64;
        let mut s_count = 0u64;
        for r in second {
            if r.tool.as_deref().unwrap_or("unknown") == tool {
                s_sum += r.effective_input_tokens.unwrap_or(0);
                s_count += 1;
            }
        }
        if f_count > 0 && s_count > 0 {
            let f_avg = f_sum / f_count;
            let s_avg = s_sum / s_count;
            drift_rows.push((tool, s_avg as i64 - f_avg as i64, f_avg, s_avg));
        }
    }
    drift_rows.sort_by(|a, b| b.1.cmp(&a.1));
    drift_rows.truncate(5);

    let mut poor_cache: Vec<(String, u64)> = tool_cache
        .iter()
        .filter_map(|(tool, (cached, input))| {
            if *input == 0 {
                None
            } else {
                Some((
                    tool.clone(),
                    ((*cached as f64 / *input as f64) * 100.0).round() as u64,
                ))
            }
        })
        .collect();
    poor_cache.sort_by(|a, b| a.1.cmp(&b.1));
    poor_cache.truncate(5);

    println!("== cxrs promptlint (last {n} runs) ==");
    println!("Top token-heavy tools (avg effective_input_tokens):");
    if top_eff.is_empty() {
        println!("- n/a");
    } else {
        for (tool, avg) in &top_eff {
            println!("- {tool}: {avg}");
        }
    }

    println!("Prompt drift (same tool, avg eff tokens second-half minus first-half):");
    if drift_rows.is_empty() {
        println!("- n/a");
    } else {
        for (tool, delta, first_avg, second_avg) in &drift_rows {
            println!("- {tool}: delta={delta}, first={first_avg}, second={second_avg}");
        }
    }

    println!("Poor cache-hit tools:");
    if poor_cache.is_empty() {
        println!("- n/a");
    } else {
        for (tool, pct) in &poor_cache {
            println!("- {tool}: {pct}%");
        }
    }

    println!("Recommendations:");
    let mut rec_count = 0usize;
    if let Some((tool, avg)) = top_eff.first() {
        if *avg > 3000 {
            println!(
                "- {tool} prompts are heavy ({avg}); reduce embedded context and enforce schema-only outputs."
            );
            rec_count += 1;
        }
    }
    if let Some((tool, delta, _, _)) = drift_rows.first() {
        if *delta > 300 {
            println!(
                "- {tool} shows token drift (+{delta}); stabilize prompt templates and prompt_preview content."
            );
            rec_count += 1;
        }
    }
    if let Some((tool, pct)) = poor_cache.first() {
        if *pct < 40 {
            println!(
                "- {tool} cache hit is low ({pct}%); reduce prompt variability and reuse stable instruction blocks."
            );
            rec_count += 1;
        }
    }
    if rec_count == 0 {
        println!("- No major prompt issues detected in this window.");
    }
    println!("log_file: {}", log_file.display());
    0
}

fn cmd_cx(command: &[String]) -> i32 {
    let result = match execute_task(TaskSpec {
        command_name: "cx".to_string(),
        input: TaskInput::SystemCommand(command.to_vec()),
        output_kind: LlmOutputKind::Plain,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cx: {e}");
            return 1;
        }
    };
    print!("{}", result.stdout);
    result.system_status.unwrap_or(0)
}

fn cmd_cxj(command: &[String]) -> i32 {
    let result = match execute_task(TaskSpec {
        command_name: "cxj".to_string(),
        input: TaskInput::SystemCommand(command.to_vec()),
        output_kind: LlmOutputKind::Jsonl,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxj: {e}");
            return 1;
        }
    };
    print!("{}", result.stdout);
    result.system_status.unwrap_or(0)
}

fn cmd_cxo(command: &[String]) -> i32 {
    let result = match execute_task(TaskSpec {
        command_name: "cxo".to_string(),
        input: TaskInput::SystemCommand(command.to_vec()),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxo: {e}");
            return 1;
        }
    };
    println!("{}", result.stdout);
    result.system_status.unwrap_or(0)
}

fn cmd_cxol(command: &[String]) -> i32 {
    cmd_cx(command)
}

fn cmd_cxcopy(command: &[String]) -> i32 {
    let result = match execute_task(TaskSpec {
        command_name: "cxcopy".to_string(),
        input: TaskInput::SystemCommand(command.to_vec()),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxcopy: {e}");
            return 1;
        }
    };
    let text = result.stdout;
    if text.trim().is_empty() {
        eprintln!("cxrs cxcopy: nothing to copy");
        return 1;
    }
    let mut pb = match Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxcopy: pbcopy unavailable: {e}");
            return 1;
        }
    };
    if let Some(stdin) = pb.stdin.as_mut() {
        let _ = stdin.write_all(text.as_bytes());
    }
    match pb.wait() {
        Ok(s) if s.success() => {
            println!("Copied to clipboard.");
            result.system_status.unwrap_or(0)
        }
        Ok(s) => {
            eprintln!("cxrs cxcopy: pbcopy failed with status {}", s);
            1
        }
        Err(e) => {
            eprintln!("cxrs cxcopy: pbcopy wait failed: {e}");
            1
        }
    }
}

fn cmd_fix(command: &[String]) -> i32 {
    let (captured, status, capture_stats) = match run_system_command_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix: {e}");
            return 1;
        }
    };
    let prompt = format!(
        "You are my terminal debugging assistant.\nTask:\n1) Explain what happened (brief).\n2) If the command failed, diagnose likely cause(s).\n3) Propose the next 3 commands to run to confirm/fix.\n4) If it is a configuration issue, point to exact file/line patterns to check.\n\nCommand:\n{}\n\nExit status: {}\n\nOutput:\n{}",
        command.join(" "),
        status,
        captured
    );
    let result = match execute_task(TaskSpec {
        command_name: "cxfix".to_string(),
        input: TaskInput::Prompt(prompt),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: Some(capture_stats),
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix: {e}");
            return status;
        }
    };
    println!("{}", result.stdout);
    status
}

fn cmd_budget() -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    println!("== cxbudget ==");
    println!(
        "CX_CONTEXT_BUDGET_CHARS={}",
        env::var("CX_CONTEXT_BUDGET_CHARS").unwrap_or_else(|_| "12000".to_string())
    );
    println!(
        "CX_CONTEXT_BUDGET_LINES={}",
        env::var("CX_CONTEXT_BUDGET_LINES").unwrap_or_else(|_| "300".to_string())
    );
    println!(
        "CX_CONTEXT_CLIP_MODE={}",
        env::var("CX_CONTEXT_CLIP_MODE").unwrap_or_else(|_| "smart".to_string())
    );
    println!(
        "CX_CONTEXT_CLIP_FOOTER={}",
        env::var("CX_CONTEXT_CLIP_FOOTER").unwrap_or_else(|_| "1".to_string())
    );
    println!("log_file: {}", log_file.display());

    if !log_file.exists() {
        return 0;
    }
    let runs = load_runs(&log_file, 1).unwrap_or_default();
    if let Some(last) = runs.last() {
        println!();
        println!("Last run clip fields:");
        show_field("system_output_len_raw", last.system_output_len_raw);
        show_field(
            "system_output_len_processed",
            last.system_output_len_processed,
        );
        show_field("system_output_len_clipped", last.system_output_len_clipped);
        show_field("system_output_lines_raw", last.system_output_lines_raw);
        show_field(
            "system_output_lines_processed",
            last.system_output_lines_processed,
        );
        show_field(
            "system_output_lines_clipped",
            last.system_output_lines_clipped,
        );
        show_field("clipped", last.clipped);
        show_field("budget_chars", last.budget_chars);
        show_field("budget_lines", last.budget_lines);
        show_field("clip_mode", last.clip_mode.clone());
        show_field("clip_footer", last.clip_footer);
        show_field("rtk_used", last.rtk_used);
        show_field("capture_provider", last.capture_provider.clone());
    }
    0
}

fn cmd_log_tail(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        eprintln!("cxrs log-tail: no log file at {}", log_file.display());
        return 1;
    }
    let file = match File::open(&log_file) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs log-tail: cannot open {}: {e}", log_file.display());
            return 1;
        }
    };
    let reader = BufReader::new(file);
    let mut lines: Vec<String> = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if !line.trim().is_empty() {
            lines.push(line);
        }
    }
    let start = lines.len().saturating_sub(n);
    for line in &lines[start..] {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            match serde_json::to_string_pretty(&v) {
                Ok(s) => println!("{s}"),
                Err(_) => println!("{line}"),
            }
        } else {
            println!("{line}");
        }
    }
    0
}

fn rtk_version_string() -> String {
    let out = match Command::new("rtk").arg("--version").output() {
        Ok(v) if v.status.success() => v,
        _ => return "<unavailable>".to_string(),
    };
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        "<unavailable>".to_string()
    } else {
        s
    }
}

fn cmd_diag() -> i32 {
    let ts = utc_now_iso();
    let version = toolchain_version_string();
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    let provider = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "auto".to_string());
    let resolved_provider = match provider.as_str() {
        "rtk" => "rtk",
        "native" => "native",
        _ => {
            if bin_in_path("rtk")
                && env::var("CX_RTK_SYSTEM").unwrap_or_else(|_| "1".to_string()) == "1"
            {
                "rtk"
            } else {
                "native"
            }
        }
    };
    let mode = env::var("CX_MODE").unwrap_or_else(|_| "lean".to_string());
    let budget_chars = env::var("CX_CONTEXT_BUDGET_CHARS").unwrap_or_else(|_| "12000".to_string());
    let budget_lines = env::var("CX_CONTEXT_BUDGET_LINES").unwrap_or_else(|_| "300".to_string());
    let clip_mode = env::var("CX_CONTEXT_CLIP_MODE").unwrap_or_else(|_| "smart".to_string());
    let clip_footer = env::var("CX_CONTEXT_CLIP_FOOTER").unwrap_or_else(|_| "1".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    let schema_dir = repo.join(".codex").join("schemas");
    let schema_count = if schema_dir.is_dir() {
        fs::read_dir(&schema_dir)
            .ok()
            .map(|iter| {
                iter.filter_map(Result::ok)
                    .filter(|e| e.path().is_file())
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };
    let last_run_id = resolve_log_file()
        .and_then(|p| {
            let len = file_len(&p);
            last_appended_json_value(&p, len.saturating_sub(8192))
        })
        .and_then(|v| {
            v.get("execution_id")
                .and_then(Value::as_str)
                .map(|s| s.to_string())
                .or_else(|| {
                    v.get("prompt_sha256")
                        .and_then(Value::as_str)
                        .map(|s| s.to_string())
                })
        })
        .unwrap_or_else(|| "<none>".to_string());
    let sample_cmd = "cxo git status";
    let rust_handles = route_handler_for("cxo");
    let bash_handles = bash_type_of_function(&repo, "cxo").is_some();
    let route_reason = if let Some(h) = rust_handles.as_ref() {
        format!("rust support found ({h})")
    } else if bash_handles {
        "bash fallback function exists".to_string()
    } else {
        "no rust route and no bash fallback".to_string()
    };

    println!("== cxdiag ==");
    println!("timestamp: {ts}");
    println!("version: {version}");
    println!("mode: {mode}");
    println!("backend: {backend}");
    println!("active_model: {active_model}");
    println!("capture_provider_config: {provider}");
    println!("capture_provider_resolved: {resolved_provider}");
    println!("rtk_available: {}", bin_in_path("rtk"));
    println!("rtk_version: {}", rtk_version_string());
    println!("budget_chars: {budget_chars}");
    println!("budget_lines: {budget_lines}");
    println!("clip_mode: {clip_mode}");
    println!("clip_footer: {clip_footer}");
    println!("log_file: {log_file}");
    println!("last_run_id: {last_run_id}");
    println!("schema_registry_dir: {}", schema_dir.display());
    println!("schema_registry_files: {schema_count}");
    println!(
        "routing_trace: sample='{}' route={} reason={}",
        sample_cmd,
        if rust_handles.is_some() {
            "rust"
        } else if bash_handles {
            "bash"
        } else {
            "unknown"
        },
        route_reason
    );
    0
}

fn last_appended_json_value(log_file: &Path, offset: u64) -> Option<Value> {
    if !log_file.exists() {
        return None;
    }
    let mut file = File::open(log_file).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let start = (offset as usize).min(bytes.len());
    let tail = String::from_utf8_lossy(&bytes[start..]);
    tail.lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
}

fn has_required_log_fields(v: &Value) -> bool {
    let required = [
        "execution_id",
        "backend_used",
        "capture_provider",
        "execution_mode",
        "schema_valid",
    ];
    required.iter().all(|k| v.get(k).is_some())
}

fn bash_function_names(repo: &Path) -> Vec<String> {
    let cx_sh = repo.join("cx.sh");
    let cmd = format!(
        "source '{}' >/dev/null 2>&1; declare -F | awk '{{print $3}}'",
        cx_sh.display()
    );
    let out = match Command::new("bash").arg("-lc").arg(cmd).output() {
        Ok(v) if v.status.success() => v,
        _ => return Vec::new(),
    };
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn cmd_parity() -> i32 {
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cxparity: cannot resolve current executable: {e}");
            return 1;
        }
    };
    let budget_chars = env::var("CX_CONTEXT_BUDGET_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(12000);

    #[derive(Default)]
    struct Row {
        cmd: String,
        rust_ok: bool,
        bash_ok: bool,
        json_ok: bool,
        logs_ok: bool,
        budget_ok: bool,
        checked: bool,
    }

    let mut rows: Vec<Row> = Vec::new();
    let mut pass_all = true;
    let ts = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let temp_repo = env::temp_dir().join(format!("cxparity-{}-{}", std::process::id(), ts));
    if fs::create_dir_all(&temp_repo).is_err() {
        eprintln!(
            "cxparity: failed to create temp repo {}",
            temp_repo.display()
        );
        return 1;
    }
    let init_ok = Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&temp_repo)
        .status()
        .ok()
        .is_some_and(|s| s.success());
    if !init_ok {
        eprintln!("cxparity: git init failed in {}", temp_repo.display());
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }
    let stage_file = temp_repo.join("cxparity_tmp.txt");
    if fs::write(&stage_file, "cx parity staged change\n").is_err() {
        eprintln!("cxparity: failed to write staged file");
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }
    let stage_ok = Command::new("git")
        .arg("add")
        .arg("cxparity_tmp.txt")
        .current_dir(&temp_repo)
        .status()
        .ok()
        .is_some_and(|s| s.success());
    if !stage_ok {
        eprintln!("cxparity: git add failed");
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }
    let temp_log_file = temp_repo.join(".codex").join("cxlogs").join("runs.jsonl");
    let bash_funcs = bash_function_names(&repo);
    let parity_catalog: Vec<(&str, Vec<&str>, Option<Vec<&str>>)> = vec![
        ("cxo", vec!["echo", "hi"], None),
        (
            "cxcommitjson",
            vec![],
            Some(vec!["subject", "body", "breaking", "tests"]),
        ),
    ];
    let overlap: Vec<(&str, Vec<&str>, Option<Vec<&str>>)> = parity_catalog
        .into_iter()
        .filter(|(cmd, _, _)| {
            route_handler_for(cmd).is_some() && bash_funcs.iter().any(|f| f == cmd)
        })
        .collect();
    if overlap.is_empty() {
        eprintln!("cxparity: no overlap commands found");
        let _ = fs::remove_dir_all(&temp_repo);
        return 1;
    }

    for (cmd, args, schema_keys) in overlap {
        let mut row = Row {
            cmd: cmd.to_string(),
            ..Row::default()
        };
        row.checked = true;
        let before_rust = file_len(&temp_log_file);
        let rust_out = Command::new(&exe)
            .arg("cx-compat")
            .arg(cmd)
            .args(&args)
            .current_dir(&temp_repo)
            .env("CX_EXECUTION_PATH", "rust:cxparity")
            .output();
        let rust_ok = rust_out.as_ref().is_ok_and(|o| o.status.success());
        row.rust_ok = rust_ok;
        let rust_stdout = rust_out
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let rust_json_ok = if let Some(keys) = schema_keys.as_ref() {
            if let Ok(v) = serde_json::from_str::<Value>(rust_stdout.trim()) {
                keys.iter().all(|k| v.get(*k).is_some())
            } else {
                false
            }
        } else {
            true
        };
        let rust_row = last_appended_json_value(&temp_log_file, before_rust);
        let rust_budget_ok = rust_stdout.chars().count() <= budget_chars
            || rust_row
                .as_ref()
                .and_then(|v| v.get("clipped"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let rust_log_ok = rust_row.as_ref().is_some_and(has_required_log_fields);

        let before_bash = file_len(&temp_log_file);
        let bash_cmd = format!(
            "source '{}' >/dev/null 2>&1; {} {}",
            repo.join("cx.sh").display(),
            cmd,
            args.join(" ")
        );
        let bash_out = Command::new("bash")
            .arg("-lc")
            .arg(bash_cmd)
            .current_dir(&temp_repo)
            .env("CX_EXECUTION_PATH", "bash:cxparity")
            .output();
        let bash_ok = bash_out.as_ref().is_ok_and(|o| o.status.success());
        row.bash_ok = bash_ok;
        let bash_stdout = bash_out
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let bash_json_ok = if let Some(keys) = schema_keys.as_ref() {
            if let Ok(v) = serde_json::from_str::<Value>(bash_stdout.trim()) {
                keys.iter().all(|k| v.get(*k).is_some())
            } else {
                false
            }
        } else {
            true
        };
        let bash_row = last_appended_json_value(&temp_log_file, before_bash);
        let bash_budget_ok = bash_stdout.chars().count() <= budget_chars
            || bash_row
                .as_ref()
                .and_then(|v| v.get("clipped"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let bash_log_ok = bash_row.as_ref().is_some_and(has_required_log_fields);

        row.json_ok = rust_json_ok && bash_json_ok;
        row.logs_ok = rust_log_ok && bash_log_ok;
        row.budget_ok = rust_budget_ok && bash_budget_ok;

        let row_pass = row.rust_ok && row.bash_ok && row.json_ok && row.logs_ok && row.budget_ok;
        if !row_pass {
            pass_all = false;
            eprintln!(
                "cxparity: FAIL {} rust_ok={} bash_ok={} json_ok={} logs_ok={} budget_ok={}",
                row.cmd, row.rust_ok, row.bash_ok, row.json_ok, row.logs_ok, row.budget_ok
            );
        }
        rows.push(row);
    }
    let _ = fs::remove_dir_all(&temp_repo);

    println!("cmd | rust | bash | json | logs | budget | result");
    println!("--- | --- | --- | --- | --- | --- | ---");
    for row in rows {
        let result = if row.checked
            && row.rust_ok
            && row.bash_ok
            && row.json_ok
            && row.logs_ok
            && row.budget_ok
        {
            "PASS"
        } else {
            "FAIL"
        };
        println!(
            "{} | {} | {} | {} | {} | {} | {}",
            row.cmd, row.rust_ok, row.bash_ok, row.json_ok, row.logs_ok, row.budget_ok, result
        );
    }
    if pass_all { 0 } else { 1 }
}

fn cmd_health() -> i32 {
    let backend = llm_backend();
    let llm_bin = llm_bin_name();
    println!("== {backend} version ==");
    let mut version_cmd = Command::new(llm_bin);
    if backend == "codex" {
        version_cmd.arg("--version");
    } else {
        version_cmd.arg("--version");
    }
    match version_cmd.output() {
        Ok(out) => print!("{}", String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            eprintln!("cxrs health: {backend} --version failed: {e}");
            return 1;
        }
    }
    println!();
    println!("== {backend} json ==");
    let jsonl = match run_llm_jsonl("ping") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs health: {backend} json failed: {e}");
            return 1;
        }
    };
    let lines: Vec<&str> = jsonl.lines().collect();
    let keep = lines.len().saturating_sub(4);
    for line in &lines[keep..] {
        println!("{line}");
    }
    println!();
    println!("== _codex_text ==");
    let txt = extract_agent_text(&jsonl).unwrap_or_default();
    println!("{txt}");
    println!();
    println!("== cxo test ==");
    let code = cmd_cxo(&["git".to_string(), "status".to_string()]);
    if code != 0 {
        return code;
    }
    println!();
    println!("All systems operational.");
    0
}

fn cmd_log_off() -> i32 {
    // Process-local only; parent shell environment is not mutated by child processes.
    // We still expose the command for parity and scripted invocation in wrapper shells.
    println!("cx logging: OFF (process-local)");
    0
}

fn cmd_log_on() -> i32 {
    println!("cx logging: ON (process-local)");
    0
}

fn cmd_alert_show() -> i32 {
    let enabled = env::var("CXALERT_ENABLED").unwrap_or_else(|_| "1".to_string());
    let max_ms = env::var("CXALERT_MAX_MS").unwrap_or_else(|_| "8000".to_string());
    let max_eff = env::var("CXALERT_MAX_EFF_IN").unwrap_or_else(|_| "5000".to_string());
    let max_out = env::var("CXALERT_MAX_OUT").unwrap_or_else(|_| "500".to_string());
    println!("cx alerts:");
    println!("enabled={enabled}");
    println!("max_ms={max_ms}");
    println!("max_eff_in={max_eff}");
    println!("max_out={max_out}");
    0
}

fn cmd_alert_off() -> i32 {
    println!("cx alerts: OFF (process-local)");
    0
}

fn cmd_alert_on() -> i32 {
    println!("cx alerts: ON (process-local)");
    0
}

fn cmd_rtk_status() -> i32 {
    let enabled = env::var("CX_RTK_ENABLED").unwrap_or_else(|_| "0".to_string());
    let system = env::var("CX_RTK_SYSTEM").unwrap_or_else(|_| "0".to_string());
    let mode = env::var("CX_RTK_MODE").unwrap_or_else(|_| "condense".to_string());
    let min = env::var("CX_RTK_MIN_VERSION").unwrap_or_else(|_| "0.22.1".to_string());
    let max = env::var("CX_RTK_MAX_VERSION").unwrap_or_default();
    let ver = rtk_version_raw().unwrap_or_else(|| "unavailable".to_string());
    let usable = rtk_is_usable();

    println!(
        "cxrtk: version={} range=[{}, {}] usable={} enabled={} system={} mode={}",
        ver,
        min,
        if max.is_empty() { "<unset>" } else { &max },
        usable,
        enabled,
        system,
        mode
    );
    println!("rtk_version: {ver}");
    println!("rtk_supported_min: {min}");
    println!(
        "rtk_supported_max: {}",
        if max.is_empty() { "<unset>" } else { &max }
    );
    println!("rtk_usable: {usable}");
    println!("rtk_enabled: {enabled}");
    println!("rtk_system: {system}");
    println!("rtk_mode: {mode}");
    println!(
        "fallback: {}",
        if usable { "none" } else { "raw command output" }
    );
    0
}

fn chunk_text_by_budget(input: &str, chunk_chars: usize) -> Vec<String> {
    if chunk_chars == 0 || input.is_empty() {
        return vec![input.to_string()];
    }
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_count = 0usize;
    for ch in input.chars() {
        cur.push(ch);
        cur_count += 1;
        if cur_count >= chunk_chars {
            chunks.push(cur);
            cur = String::new();
            cur_count = 0;
        }
    }
    if !cur.is_empty() || chunks.is_empty() {
        chunks.push(cur);
    }
    chunks
}

fn cmd_chunk() -> i32 {
    let mut buf = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
        eprintln!("cxrs chunk: failed to read stdin: {e}");
        return 1;
    }
    let budget = env::var("CX_CONTEXT_BUDGET_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(12000);
    let chunks = chunk_text_by_budget(&buf, budget);
    let total = chunks.len();
    for (i, ch) in chunks.iter().enumerate() {
        println!("----- cx chunk {}/{} -----", i + 1, total);
        print!("{ch}");
        if !ch.ends_with('\n') {
            println!();
        }
    }
    0
}

fn cmd_next(command: &[String]) -> i32 {
    let (captured, exit_status, capture_stats) = match run_system_command_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };

    let schema = match load_schema("next") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };
    let task_input = format!(
        "Based on the terminal command output below, propose the NEXT shell commands to run.\nReturn 1-6 commands in execution order.\n\nExecuted command:\n{}\nExit status: {}\n\nTERMINAL OUTPUT:\n{}",
        command.join(" "),
        exit_status,
        captured
    );
    let result = match execute_task(TaskSpec {
        command_name: "cxrs_next".to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input.clone()),
        logging_enabled: true,
        capture_override: Some(capture_stats),
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };
    if result.schema_valid == Some(false) {
        if let Some(qid) = result.quarantine_id.as_deref() {
            eprintln!("cxrs next: schema failure; quarantine_id={qid}");
        }
        eprintln!("cxrs next: raw response follows:");
        eprintln!("{}", result.stdout);
        return 1;
    }
    let schema_value: Value = match serde_json::from_str(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: invalid JSON after schema run: {e}");
            return 1;
        }
    };
    let commands = match parse_commands_array(&schema_value.to_string()) {
        Ok(v) => v,
        Err(reason) => {
            eprintln!("cxrs next: {reason}");
            return 1;
        }
    };
    for cmd in commands {
        println!("{cmd}");
    }
    0
}

fn cmd_fix_run(command: &[String]) -> i32 {
    let mut unsafe_override = false;
    let mut cmdv = command.to_vec();
    if cmdv.first().map(String::as_str) == Some("--unsafe") {
        unsafe_override = true;
        cmdv = cmdv.into_iter().skip(1).collect();
    }
    if cmdv.is_empty() {
        eprintln!("Usage: {APP_NAME} fix-run [--unsafe] <command> [args...]");
        return 2;
    }
    let (captured, exit_status, capture_stats) = match run_system_command_capture(&cmdv) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };

    let schema = match load_schema("fixrun") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };
    let task_input = format!(
        "You are my terminal debugging assistant.\nGiven the command, exit status, and output, provide concise remediation.\n\nCommand:\n{}\n\nExit status: {}\n\nOutput:\n{}",
        cmdv.join(" "),
        exit_status,
        captured
    );
    let result = match execute_task(TaskSpec {
        command_name: "cxrs_fix_run".to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input.clone()),
        logging_enabled: false,
        capture_override: Some(capture_stats),
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };
    if result.schema_valid == Some(false) {
        let _ = log_codex_run(
            "cxrs_fix_run",
            &task_input,
            result.duration_ms,
            Some(&result.usage),
            Some(&result.capture_stats),
            false,
            Some("schema_validation_failed"),
            Some(schema.name.as_str()),
            result.quarantine_id.as_deref(),
            None,
            None,
        );
        if let Some(qid) = result.quarantine_id.as_deref() {
            eprintln!("cxrs fix-run: schema failure; quarantine_id={qid}");
        }
        eprintln!("cxrs fix-run: raw response follows:");
        eprintln!("{}", result.stdout);
        return 1;
    }
    let v: Value = match serde_json::from_str(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: invalid JSON after schema run: {e}");
            return 1;
        }
    };
    let analysis = v
        .get("analysis")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let commands = match parse_commands_array(&result.stdout) {
        Ok(v) => v,
        Err(reason) => {
            eprintln!("cxrs fix-run: {reason}");
            return 1;
        }
    };

    if !analysis.is_empty() {
        println!("Analysis:");
        println!("{analysis}");
        println!();
    }
    println!("Suggested commands:");
    println!("-------------------");
    for c in &commands {
        println!("{c}");
    }
    println!("-------------------");

    let should_run = env::var("CXFIX_RUN").ok().as_deref() == Some("1");
    let force = env::var("CXFIX_FORCE").ok().as_deref() == Some("1");
    let unsafe_env = env::var("CX_UNSAFE").ok().as_deref() == Some("1");
    let allow_unsafe = unsafe_override || unsafe_env;
    if !should_run {
        println!("Not running suggested commands (set CXFIX_RUN=1 to execute).");
        let _ = log_codex_run(
            "cxrs_fix_run",
            &task_input,
            result.duration_ms,
            Some(&result.usage),
            Some(&result.capture_stats),
            true,
            None,
            Some(schema.name.as_str()),
            None,
            None,
            None,
        );
        return if exit_status == 0 { 0 } else { exit_status };
    }

    let mut policy_blocked = false;
    let mut policy_reasons: Vec<String> = Vec::new();
    for c in commands {
        let root = repo_root()
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        match evaluate_command_safety(&c, &root) {
            SafetyDecision::Safe => {}
            SafetyDecision::Dangerous(reason) => {
                if !(force || allow_unsafe) {
                    policy_blocked = true;
                    policy_reasons.push(reason.clone());
                    eprintln!(
                        "WARN blocked dangerous command ({reason}); use CXFIX_FORCE=1 or --unsafe: {c}"
                    );
                    continue;
                }
                eprintln!("WARN unsafe override active; executing: {c}");
            }
        }
        println!("-> {c}");
        let status = Command::new("bash").args(["-lc", &c]).status();
        if let Err(e) = status {
            eprintln!("cxrs fix-run: failed to execute command: {e}");
        }
    }

    let _ = log_codex_run(
        "cxrs_fix_run",
        &task_input,
        result.duration_ms,
        Some(&result.usage),
        Some(&result.capture_stats),
        true,
        None,
        Some(schema.name.as_str()),
        None,
        Some(policy_blocked),
        if policy_reasons.is_empty() {
            None
        } else {
            Some(policy_reasons.join("; "))
        }
        .as_deref(),
    );

    if exit_status == 0 { 0 } else { exit_status }
}

fn generate_commitjson_value() -> Result<Value, String> {
    let (diff_out, status, capture_stats) = run_system_command_capture(&[
        "git".to_string(),
        "diff".to_string(),
        "--staged".to_string(),
        "--no-color".to_string(),
    ])?;
    if status != 0 {
        return Err(format!(
            "git diff --staged failed with exit status {status}: {diff_out}"
        ));
    }
    if diff_out.trim().is_empty() {
        return Err("no staged changes. run: git add -p".to_string());
    }

    let conventional = read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.conventional_commits"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let style_hint = if conventional {
        "Use concise conventional-commit style subject."
    } else {
        "Use concise imperative subject (non-conventional format)."
    };
    let schema = load_schema("commitjson")?;
    let task_input = format!(
        "Generate a commit object from this STAGED diff.\n{style_hint}\n\nSTAGED DIFF:\n{diff_out}"
    );
    let result = execute_task(TaskSpec {
        command_name: "cxrs_commitjson".to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input),
        logging_enabled: true,
        capture_override: Some(capture_stats),
    })?;
    let mut v: Value = if result.schema_valid == Some(false) {
        let qid = result.quarantine_id.unwrap_or_default();
        return Err(format!(
            "schema validation failed; quarantine_id={qid}; raw={}",
            result.stdout
        ));
    } else {
        serde_json::from_str(&result.stdout).map_err(|e| format!("invalid JSON: {e}"))?
    };
    if v.get("scope").is_none() {
        if let Some(obj) = v.as_object_mut() {
            obj.insert("scope".to_string(), Value::Null);
        }
    }
    Ok(v)
}

fn render_bullets(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Some(Value::String(s)) => {
            if s.trim().is_empty() {
                Vec::new()
            } else {
                vec![s.trim().to_string()]
            }
        }
        _ => Vec::new(),
    }
}

fn print_diffsum_human(v: &Value) {
    let title = v.get("title").and_then(Value::as_str).unwrap_or("");
    let summary = render_bullets(v.get("summary"));
    let risks = render_bullets(v.get("risk_edge_cases"));
    let tests = render_bullets(v.get("suggested_tests"));

    println!("Title: {title}");
    println!();
    println!("Summary:");
    if summary.is_empty() {
        println!("- n/a");
    } else {
        for s in summary {
            println!("- {s}");
        }
    }
    println!();
    println!("Risk/edge cases:");
    if risks.is_empty() {
        println!("- n/a");
    } else {
        for s in risks {
            println!("- {s}");
        }
    }
    println!();
    println!("Suggested tests:");
    if tests.is_empty() {
        println!("- n/a");
    } else {
        for s in tests {
            println!("- {s}");
        }
    }
}

fn generate_diffsum_value(tool: &str, staged: bool) -> Result<Value, String> {
    let git_cmd = if staged {
        vec![
            "git".to_string(),
            "diff".to_string(),
            "--staged".to_string(),
            "--no-color".to_string(),
        ]
    } else {
        vec![
            "git".to_string(),
            "diff".to_string(),
            "--no-color".to_string(),
        ]
    };
    let (diff_out, status, capture_stats) = run_system_command_capture(&git_cmd)?;
    if status != 0 {
        return Err(format!("git diff failed with status {status}"));
    }
    if diff_out.trim().is_empty() {
        if staged {
            return Err("no staged changes.".to_string());
        }
        return Err("no unstaged changes.".to_string());
    }

    let pr_fmt = read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.pr_summary_format"))
        .and_then(Value::as_str)
        .unwrap_or("standard")
        .to_string();
    let schema = load_schema("diffsum")?;
    let diff_label = if staged { "STAGED DIFF" } else { "DIFF" };
    let task_input = format!(
        "Write a PR-ready summary of this diff.\nKeep bullets concise and actionable.\nPreferred PR summary format: {pr_fmt}\n\n{diff_label}:\n{diff_out}"
    );
    let result = execute_task(TaskSpec {
        command_name: tool.to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input),
        logging_enabled: true,
        capture_override: Some(capture_stats),
    })?;
    if result.schema_valid == Some(false) {
        return Err(format!(
            "schema validation failed; quarantine_id={}; raw={}",
            result.quarantine_id.unwrap_or_default(),
            result.stdout
        ));
    }
    serde_json::from_str(&result.stdout).map_err(|e| format!("invalid JSON: {e}"))
}

fn cmd_diffsum(staged: bool) -> i32 {
    let tool = if staged {
        "cxrs_diffsum_staged"
    } else {
        "cxrs_diffsum"
    };
    match generate_diffsum_value(tool, staged) {
        Ok(v) => {
            print_diffsum_human(&v);
            0
        }
        Err(e) => {
            eprintln!(
                "cxrs {}: {e}",
                if staged { "diffsum-staged" } else { "diffsum" }
            );
            1
        }
    }
}

fn cmd_commitjson() -> i32 {
    match generate_commitjson_value() {
        Ok(v) => match serde_json::to_string_pretty(&v) {
            Ok(s) => {
                println!("{s}");
                0
            }
            Err(e) => {
                eprintln!("cxrs commitjson: render failure: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("cxrs commitjson: {e}");
            1
        }
    }
}

fn cmd_commitmsg() -> i32 {
    let v = match generate_commitjson_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs commitmsg: {e}");
            return 1;
        }
    };
    let subject = v
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let body_items: Vec<String> = v
        .get("body")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    let test_items: Vec<String> = v
        .get("tests")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    println!("{subject}");
    println!();
    if !body_items.is_empty() {
        for line in body_items {
            println!("- {line}");
        }
    }
    if !test_items.is_empty() {
        println!();
        println!("Tests:");
        for line in test_items {
            println!("- {line}");
        }
    }
    0
}

fn cmd_replay(id: &str) -> i32 {
    let rec = match read_quarantine_record(id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs replay: {e}");
            return 1;
        }
    };

    if rec.schema.trim().is_empty() || rec.prompt.trim().is_empty() {
        eprintln!("cxrs replay: quarantine entry is missing schema/prompt payload");
        return 1;
    }

    let full_prompt = build_strict_schema_prompt(&rec.schema, &rec.prompt);
    let jsonl = match run_llm_jsonl(&full_prompt) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs replay: {e}");
            return 1;
        }
    };
    let raw = extract_agent_text(&jsonl).unwrap_or_default();
    if raw.trim().is_empty() {
        match log_schema_failure(
            &format!("{}_replay", rec.tool),
            "empty_agent_message",
            &raw,
            &rec.schema,
            &rec.prompt,
            Vec::new(),
        ) {
            Ok(qid) => eprintln!("cxrs replay: empty response; quarantine_id={qid}"),
            Err(e) => eprintln!("cxrs replay: failed to log schema failure: {e}"),
        }
        return 1;
    }

    if serde_json::from_str::<Value>(&raw).is_err() {
        match log_schema_failure(
            &format!("{}_replay", rec.tool),
            "invalid_json",
            &raw,
            &rec.schema,
            &rec.prompt,
            Vec::new(),
        ) {
            Ok(qid) => eprintln!("cxrs replay: invalid JSON; quarantine_id={qid}"),
            Err(e) => eprintln!("cxrs replay: failed to log schema failure: {e}"),
        }
        eprintln!("cxrs replay: raw response follows:");
        eprintln!("{raw}");
        return 1;
    }

    println!("{raw}");
    0
}

fn cmd_quarantine_list(n: usize) -> i32 {
    let Some(qdir) = resolve_quarantine_dir() else {
        eprintln!("cxrs quarantine list: unable to resolve quarantine directory");
        return 1;
    };
    if !qdir.exists() {
        println!("== cxrs quarantine list ==");
        println!("entries: 0");
        println!("quarantine_dir: {}", qdir.display());
        return 0;
    }

    let mut rows: Vec<QuarantineRecord> = Vec::new();
    let rd = match fs::read_dir(&qdir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs quarantine list: cannot read {}: {e}", qdir.display());
            return 1;
        }
    };

    for ent in rd.flatten() {
        let path = ent.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let Ok(mut s) = fs::read_to_string(&path) else {
            continue;
        };
        if s.trim().is_empty() {
            continue;
        }
        if !s.ends_with('\n') {
            s.push('\n');
        }
        if let Ok(rec) = serde_json::from_str::<QuarantineRecord>(&s) {
            rows.push(rec);
        }
    }

    rows.sort_by(|a, b| b.ts.cmp(&a.ts));
    if rows.len() > n {
        rows.truncate(n);
    }

    println!("== cxrs quarantine list ==");
    println!("entries: {}", rows.len());
    for rec in rows {
        println!("- {} | {} | {} | {}", rec.id, rec.ts, rec.tool, rec.reason);
    }
    println!("quarantine_dir: {}", qdir.display());
    0
}

fn cmd_quarantine_show(id: &str) -> i32 {
    let rec = match read_quarantine_record(id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs quarantine show: {e}");
            return 1;
        }
    };
    match serde_json::to_string_pretty(&rec) {
        Ok(v) => {
            println!("{v}");
            0
        }
        Err(e) => {
            eprintln!("cxrs quarantine show: failed to render JSON: {e}");
            1
        }
    }
}

fn cmd_cx_compat(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!("Usage: {APP_NAME} cx <command> [args...]");
        return 2;
    }
    let sub = args[0].as_str();
    match sub {
        "help" => {
            if args.get(1).map(String::as_str) == Some("task") {
                print_task_help();
            } else {
                print_help();
            }
            0
        }
        "cxversion" | "version" => {
            print_version();
            0
        }
        "cxdoctor" | "doctor" => print_doctor(),
        "cxwhere" | "where" => print_where(&args[1..]),
        "cxroutes" | "routes" => cmd_routes(&args[1..]),
        "cxdiag" | "diag" => cmd_diag(),
        "cxparity" | "parity" => cmd_parity(),
        "cxcore" | "core" => cmd_core(),
        "cxlogs" | "logs" => cmd_logs(APP_NAME, &args[1..]),
        "cxtask" | "task" => cmd_task(&args[1..]),
        "cxmetrics" | "metrics" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_metrics(n)
        }
        "cxprofile" | "profile" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_profile(n)
        }
        "cxtrace" | "trace" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(1);
            print_trace(n)
        }
        "cxalert" | "alert" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_alert(n)
        }
        "cxoptimize" | "optimize" => {
            let (n, json_out) = match parse_optimize_args(&args[1..], 200) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{APP_NAME} cx optimize: {e}");
                    return 2;
                }
            };
            print_optimize(n, json_out)
        }
        "cxworklog" | "worklog" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_worklog(n)
        }
        "cx" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx <command> [args...]");
                return 2;
            }
            cmd_cx(&args[1..])
        }
        "cxj" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cxj <command> [args...]");
                return 2;
            }
            cmd_cxj(&args[1..])
        }
        "cxo" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cxo <command> [args...]");
                return 2;
            }
            cmd_cxo(&args[1..])
        }
        "cxol" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cxol <command> [args...]");
                return 2;
            }
            cmd_cxol(&args[1..])
        }
        "cxcopy" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cxcopy <command> [args...]");
                return 2;
            }
            cmd_cxcopy(&args[1..])
        }
        "cxpolicy" | "policy" => cmd_policy(&args[1..]),
        "cxstate" | "state" => match args.get(1).map(String::as_str).unwrap_or("show") {
            "show" => cmd_state_show(),
            "get" => {
                let Some(key) = args.get(2) else {
                    eprintln!("Usage: {APP_NAME} cx state get <key>");
                    return 2;
                };
                cmd_state_get(key)
            }
            "set" => {
                let Some(key) = args.get(2) else {
                    eprintln!("Usage: {APP_NAME} cx state set <key> <value>");
                    return 2;
                };
                let Some(value) = args.get(3) else {
                    eprintln!("Usage: {APP_NAME} cx state set <key> <value>");
                    return 2;
                };
                cmd_state_set(key, value)
            }
            other => {
                eprintln!("{APP_NAME} cx state: unknown subcommand '{other}'");
                2
            }
        },
        "cxllm" | "llm" => cmd_llm(&args[1..]),
        "cxbench" | "bench" => {
            let Some(runs) = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
            else {
                eprintln!("Usage: {APP_NAME} cx bench <runs> -- <command...>");
                return 2;
            };
            let delim = args.iter().position(|v| v == "--");
            let Some(i) = delim else {
                eprintln!("Usage: {APP_NAME} cx bench <runs> -- <command...>");
                return 2;
            };
            if i + 1 >= args.len() {
                eprintln!("Usage: {APP_NAME} cx bench <runs> -- <command...>");
                return 2;
            }
            cmd_bench(runs, &args[i + 1..])
        }
        "cxprompt" | "prompt" => {
            let Some(mode) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} cx prompt <mode> <request>");
                return 2;
            };
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cx prompt <mode> <request>");
                return 2;
            }
            cmd_prompt(mode, &args[2..].join(" "))
        }
        "cxroles" | "roles" => cmd_roles(args.get(1).map(String::as_str)),
        "cxfanout" | "fanout" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx fanout <objective>");
                return 2;
            }
            cmd_fanout(&args[1..].join(" "))
        }
        "cxpromptlint" | "promptlint" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(200);
            cmd_promptlint(n)
        }
        "cxnext" | "next" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx next <command> [args...]");
                return 2;
            }
            cmd_next(&args[1..])
        }
        "cxfix" | "fix" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx fix <command> [args...]");
                return 2;
            }
            cmd_fix(&args[1..])
        }
        "cxdiffsum" | "diffsum" => cmd_diffsum(false),
        "cxdiffsum_staged" | "diffsum-staged" => cmd_diffsum(true),
        "cxcommitjson" | "commitjson" => cmd_commitjson(),
        "cxcommitmsg" | "commitmsg" => cmd_commitmsg(),
        "cxbudget" | "budget" => cmd_budget(),
        "cxlog_tail" | "log-tail" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(10);
            cmd_log_tail(n)
        }
        "cxhealth" | "health" => cmd_health(),
        "cxrtk" | "rtk-status" => cmd_rtk_status(),
        "cxlog_on" | "log-on" => cmd_log_on(),
        "cxlog_off" | "log-off" => cmd_log_off(),
        "cxalert_show" | "alert-show" => cmd_alert_show(),
        "cxalert_on" | "alert-on" => cmd_alert_on(),
        "cxalert_off" | "alert-off" => cmd_alert_off(),
        "cxchunk" | "chunk" => cmd_chunk(),
        "cxfix_run" | "fix-run" => {
            if args.len() < 2 {
                eprintln!("Usage: {APP_NAME} cx fix-run <command> [args...]");
                return 2;
            }
            cmd_fix_run(&args[1..])
        }
        "cxreplay" | "replay" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {APP_NAME} cx replay <quarantine_id>");
                return 2;
            };
            cmd_replay(id)
        }
        "cxquarantine" | "quarantine" => match args.get(1).map(String::as_str).unwrap_or("list") {
            "list" => {
                let n = args
                    .get(2)
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0)
                    .unwrap_or(20);
                cmd_quarantine_list(n)
            }
            "show" => {
                let Some(id) = args.get(2) else {
                    eprintln!("Usage: {APP_NAME} cx quarantine show <quarantine_id>");
                    return 2;
                };
                cmd_quarantine_show(id)
            }
            other => {
                eprintln!("{APP_NAME} cx quarantine: unknown subcommand '{other}'");
                2
            }
        },
        other => {
            eprintln!("{APP_NAME} cx: unsupported command '{other}'");
            2
        }
    }
}

fn is_compat_name(name: &str) -> bool {
    matches!(
        name,
        "help"
            | "cxversion"
            | "version"
            | "cxdoctor"
            | "doctor"
            | "cxwhere"
            | "where"
            | "cxroutes"
            | "routes"
            | "cxdiag"
            | "diag"
            | "cxparity"
            | "parity"
            | "cxcore"
            | "core"
            | "cxlogs"
            | "logs"
            | "cxtask"
            | "task"
            | "cxmetrics"
            | "metrics"
            | "cxprofile"
            | "profile"
            | "cxtrace"
            | "trace"
            | "cxalert"
            | "alert"
            | "cxoptimize"
            | "optimize"
            | "cxworklog"
            | "worklog"
            | "cx"
            | "cxj"
            | "cxo"
            | "cxol"
            | "cxcopy"
            | "cxpolicy"
            | "policy"
            | "cxstate"
            | "state"
            | "cxllm"
            | "llm"
            | "cxbench"
            | "bench"
            | "cxprompt"
            | "prompt"
            | "cxroles"
            | "roles"
            | "cxfanout"
            | "fanout"
            | "cxpromptlint"
            | "promptlint"
            | "cxnext"
            | "next"
            | "cxfix"
            | "fix"
            | "cxdiffsum"
            | "diffsum"
            | "cxdiffsum_staged"
            | "diffsum-staged"
            | "cxcommitjson"
            | "commitjson"
            | "cxcommitmsg"
            | "commitmsg"
            | "cxbudget"
            | "budget"
            | "cxlog_tail"
            | "log-tail"
            | "cxhealth"
            | "health"
            | "cxrtk"
            | "rtk-status"
            | "cxlog_on"
            | "log-on"
            | "cxlog_off"
            | "log-off"
            | "cxalert_show"
            | "alert-show"
            | "cxalert_on"
            | "alert-on"
            | "cxalert_off"
            | "alert-off"
            | "cxchunk"
            | "chunk"
            | "cxfix_run"
            | "fix-run"
            | "cxreplay"
            | "replay"
            | "cxquarantine"
            | "quarantine"
            | "schema"
    )
}

fn is_native_name(name: &str) -> bool {
    matches!(
        name,
        "help"
            | "-h"
            | "--help"
            | "version"
            | "-V"
            | "--version"
            | "where"
            | "routes"
            | "diag"
            | "parity"
            | "core"
            | "logs"
            | "ci"
            | "task"
            | "doctor"
            | "state"
            | "llm"
            | "policy"
            | "bench"
            | "metrics"
            | "prompt"
            | "roles"
            | "fanout"
            | "promptlint"
            | "cx"
            | "cxj"
            | "cxo"
            | "cxol"
            | "cxcopy"
            | "fix"
            | "budget"
            | "log-tail"
            | "health"
            | "rtk-status"
            | "log-on"
            | "log-off"
            | "alert-show"
            | "alert-on"
            | "alert-off"
            | "chunk"
            | "cx-compat"
            | "profile"
            | "alert"
            | "optimize"
            | "worklog"
            | "trace"
            | "next"
            | "fix-run"
            | "diffsum"
            | "diffsum-staged"
            | "commitjson"
            | "commitmsg"
            | "replay"
            | "quarantine"
            | "supports"
            | "schema"
    )
}

pub fn run() -> i32 {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    let code = match cmd {
        "help" | "-h" | "--help" => {
            if args.get(2).map(String::as_str) == Some("task") {
                print_task_help();
            } else {
                print_help();
            }
            0
        }
        "version" | "-V" | "--version" => {
            print_version();
            0
        }
        "schema" => cmd_schema(&args[2..]),
        "logs" => cmd_logs(APP_NAME, &args[2..]),
        "ci" => cmd_ci(&args[2..]),
        "core" => cmd_core(),
        "task" => cmd_task(&args[2..]),
        "where" => print_where(&args[2..]),
        "routes" => cmd_routes(&args[2..]),
        "diag" => cmd_diag(),
        "parity" => cmd_parity(),
        "supports" => {
            let Some(name) = args.get(2) else {
                eprintln!("Usage: {APP_NAME} supports <subcommand>");
                std::process::exit(2);
            };
            if is_native_name(name) || is_compat_name(name) {
                println!("true");
                0
            } else {
                println!("false");
                1
            }
        }
        "doctor" => print_doctor(),
        "state" => match args.get(2).map(String::as_str).unwrap_or("show") {
            "show" => cmd_state_show(),
            "get" => {
                let Some(key) = args.get(3) else {
                    eprintln!("Usage: {APP_NAME} state get <key>");
                    std::process::exit(2);
                };
                cmd_state_get(key)
            }
            "set" => {
                let Some(key) = args.get(3) else {
                    eprintln!("Usage: {APP_NAME} state set <key> <value>");
                    std::process::exit(2);
                };
                let Some(value) = args.get(4) else {
                    eprintln!("Usage: {APP_NAME} state set <key> <value>");
                    std::process::exit(2);
                };
                cmd_state_set(key, value)
            }
            other => {
                eprintln!("{APP_NAME}: unknown state subcommand '{other}'");
                eprintln!("Usage: {APP_NAME} state <show|get <key>|set <key> <value>>");
                2
            }
        },
        "llm" => cmd_llm(&args[2..]),
        "policy" => cmd_policy(&args[2..]),
        "bench" => {
            if args.len() < 5 {
                eprintln!("Usage: {APP_NAME} bench <runs> -- <command...>");
                std::process::exit(2);
            }
            let runs = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(0);
            let delim = args.iter().position(|v| v == "--");
            let Some(i) = delim else {
                eprintln!("Usage: {APP_NAME} bench <runs> -- <command...>");
                std::process::exit(2);
            };
            if i + 1 >= args.len() {
                eprintln!("Usage: {APP_NAME} bench <runs> -- <command...>");
                std::process::exit(2);
            }
            cmd_bench(runs, &args[i + 1..])
        }
        "metrics" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_metrics(n)
        }
        "prompt" => {
            let Some(mode) = args.get(2) else {
                eprintln!("Usage: {APP_NAME} prompt <implement|fix|test|doc|ops> <request>");
                std::process::exit(2);
            };
            if args.len() < 4 {
                eprintln!("Usage: {APP_NAME} prompt <implement|fix|test|doc|ops> <request>");
                std::process::exit(2);
            }
            let request = args[3..].join(" ");
            cmd_prompt(mode, &request)
        }
        "roles" => cmd_roles(args.get(2).map(String::as_str)),
        "fanout" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} fanout <objective>");
                std::process::exit(2);
            }
            cmd_fanout(&args[2..].join(" "))
        }
        "promptlint" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(200);
            cmd_promptlint(n)
        }
        "cx" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cx <command> [args...]");
                std::process::exit(2);
            }
            if is_compat_name(&args[2]) {
                cmd_cx_compat(&args[2..])
            } else {
                cmd_cx(&args[2..])
            }
        }
        "cxj" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cxj <command> [args...]");
                std::process::exit(2);
            }
            cmd_cxj(&args[2..])
        }
        "cxo" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cxo <command> [args...]");
                std::process::exit(2);
            }
            cmd_cxo(&args[2..])
        }
        "cxol" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cxol <command> [args...]");
                std::process::exit(2);
            }
            cmd_cxol(&args[2..])
        }
        "cxcopy" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} cxcopy <command> [args...]");
                std::process::exit(2);
            }
            cmd_cxcopy(&args[2..])
        }
        "fix" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} fix <command> [args...]");
                std::process::exit(2);
            }
            cmd_fix(&args[2..])
        }
        "budget" => cmd_budget(),
        "log-tail" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(10);
            cmd_log_tail(n)
        }
        "health" => cmd_health(),
        "rtk-status" => cmd_rtk_status(),
        "log-on" => cmd_log_on(),
        "log-off" => cmd_log_off(),
        "alert-show" => cmd_alert_show(),
        "alert-on" => cmd_alert_on(),
        "alert-off" => cmd_alert_off(),
        "chunk" => cmd_chunk(),
        "cx-compat" => cmd_cx_compat(&args[2..]),
        "profile" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_profile(n)
        }
        "alert" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_alert(n)
        }
        "optimize" => {
            let (n, json_out) = match parse_optimize_args(&args[2..], 200) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{APP_NAME} optimize: {e}");
                    std::process::exit(2);
                }
            };
            print_optimize(n, json_out)
        }
        "worklog" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_worklog(n)
        }
        "trace" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(1);
            print_trace(n)
        }
        "next" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} next <command> [args...]");
                std::process::exit(2);
            }
            cmd_next(&args[2..])
        }
        "diffsum" => cmd_diffsum(false),
        "diffsum-staged" => cmd_diffsum(true),
        "fix-run" => {
            if args.len() < 3 {
                eprintln!("Usage: {APP_NAME} fix-run <command> [args...]");
                std::process::exit(2);
            }
            cmd_fix_run(&args[2..])
        }
        "commitjson" => cmd_commitjson(),
        "commitmsg" => cmd_commitmsg(),
        "replay" => {
            let Some(id) = args.get(2) else {
                eprintln!("Usage: {APP_NAME} replay <quarantine_id>");
                std::process::exit(2);
            };
            cmd_replay(id)
        }
        "quarantine" => match args.get(2).map(String::as_str).unwrap_or("list") {
            "list" => {
                let n = args
                    .get(3)
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0)
                    .unwrap_or(20);
                cmd_quarantine_list(n)
            }
            "show" => {
                let Some(id) = args.get(3) else {
                    eprintln!("Usage: {APP_NAME} quarantine show <quarantine_id>");
                    std::process::exit(2);
                };
                cmd_quarantine_show(id)
            }
            other => {
                eprintln!("{APP_NAME}: unknown quarantine subcommand '{other}'");
                eprintln!("Usage: {APP_NAME} quarantine <list [N]|show <id>>");
                2
            }
        },
        _ => {
            eprintln!("{APP_NAME}: unknown command '{cmd}'");
            eprintln!("Run '{APP_NAME} help' for usage.");
            2
        }
    };
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn smart_mode_prefers_tail_on_error_keywords() {
        assert_eq!(choose_clip_mode("all good", "smart"), "head");
        assert_eq!(choose_clip_mode("WARNING: issue", "smart"), "tail");
        assert_eq!(choose_clip_mode("failed to run", "smart"), "tail");
    }

    #[test]
    fn clip_text_respects_line_and_char_budget() {
        let cfg = BudgetConfig {
            budget_chars: 12,
            budget_lines: 2,
            clip_mode: "head".to_string(),
            clip_footer: false,
        };
        let (out, stats) = clip_text_with_config("line1\nline2\nline3\n", &cfg);
        assert!(out.starts_with("line1\nline2"));
        assert_eq!(stats.budget_chars, Some(12));
        assert_eq!(stats.budget_lines, Some(2));
        assert_eq!(stats.clipped, Some(true));
    }

    #[test]
    fn jsonl_append_integrity() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("runs.jsonl");
        append_jsonl(&file, &json!({"a":1})).expect("append 1");
        append_jsonl(&file, &json!({"b":2})).expect("append 2");
        let content = fs::read_to_string(&file).expect("read");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        let v1: Value = serde_json::from_str(lines[0]).expect("line1 json");
        let v2: Value = serde_json::from_str(lines[1]).expect("line2 json");
        assert_eq!(v1.get("a").and_then(Value::as_i64), Some(1));
        assert_eq!(v2.get("b").and_then(Value::as_i64), Some(2));
    }

    #[test]
    fn rtk_unavailable_path_uses_native() {
        let cmd = vec!["git".to_string(), "status".to_string()];
        assert!(!should_use_rtk(&cmd, "auto", true, false));
        assert!(!should_use_rtk(&cmd, "native", true, true));
    }

    #[test]
    fn schema_failure_writes_quarantine_and_logs() {
        let _guard = cwd_lock().lock().expect("lock");
        let dir = tempdir().expect("tempdir");
        let prev = env::current_dir().expect("cwd");
        env::set_current_dir(dir.path()).expect("cd temp");
        let _ = Command::new("git")
            .args(["init"])
            .output()
            .expect("git init");

        let qid = log_schema_failure("cxrs_next", "invalid_json", "raw", "{}", "prompt", Vec::new())
            .expect("schema failure log");
        assert!(!qid.is_empty());

        let qfile = dir
            .path()
            .join(".codex")
            .join("quarantine")
            .join(format!("{qid}.json"));
        assert!(qfile.exists());

        let sf_log = dir
            .path()
            .join(".codex")
            .join("cxlogs")
            .join("schema_failures.jsonl");
        let sf = fs::read_to_string(&sf_log).expect("read schema fail log");
        let last_sf: Value =
            serde_json::from_str(sf.lines().last().expect("sf line")).expect("sf json");
        assert_eq!(
            last_sf.get("quarantine_id").and_then(Value::as_str),
            Some(qid.as_str())
        );

        let runs_log = dir.path().join(".codex").join("cxlogs").join("runs.jsonl");
        let runs = fs::read_to_string(&runs_log).expect("read runs");
        let last_run: Value =
            serde_json::from_str(runs.lines().last().expect("run line")).expect("run json");
        assert_eq!(
            last_run.get("quarantine_id").and_then(Value::as_str),
            Some(qid.as_str())
        );
        assert_eq!(
            last_run.get("schema_valid").and_then(Value::as_bool),
            Some(false)
        );

        env::set_current_dir(prev).expect("restore cwd");
    }
}

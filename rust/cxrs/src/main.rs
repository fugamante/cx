use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const APP_NAME: &str = "cxrs";
const APP_DESC: &str = "Rust spike for the cx toolchain";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
static RTK_WARNED_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize, Default, Clone)]
struct RunEntry {
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    cached_input_tokens: Option<u64>,
    #[serde(default)]
    effective_input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    repo_root: Option<String>,
    #[serde(default)]
    prompt_sha256: Option<String>,
    #[serde(default)]
    prompt_preview: Option<String>,
    #[serde(default)]
    system_output_len_raw: Option<u64>,
    #[serde(default)]
    system_output_len_processed: Option<u64>,
    #[serde(default)]
    system_output_len_clipped: Option<u64>,
    #[serde(default)]
    system_output_lines_raw: Option<u64>,
    #[serde(default)]
    system_output_lines_processed: Option<u64>,
    #[serde(default)]
    system_output_lines_clipped: Option<u64>,
    #[serde(default)]
    clipped: Option<bool>,
    #[serde(default)]
    budget_chars: Option<u64>,
    #[serde(default)]
    budget_lines: Option<u64>,
    #[serde(default)]
    clip_mode: Option<String>,
    #[serde(default)]
    clip_footer: Option<bool>,
    #[serde(default)]
    rtk_used: Option<bool>,
    #[serde(default)]
    capture_provider: Option<String>,
    #[serde(default)]
    llm_backend: Option<String>,
    #[serde(default)]
    llm_model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct QuarantineRecord {
    #[serde(default)]
    id: String,
    #[serde(default)]
    ts: String,
    #[serde(default)]
    tool: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    schema: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    prompt_sha256: String,
    #[serde(default)]
    raw_response: String,
    #[serde(default)]
    raw_sha256: String,
}

#[derive(Debug, Default, Clone)]
struct CaptureStats {
    system_output_len_raw: Option<u64>,
    system_output_len_processed: Option<u64>,
    system_output_len_clipped: Option<u64>,
    system_output_lines_raw: Option<u64>,
    system_output_lines_processed: Option<u64>,
    system_output_lines_clipped: Option<u64>,
    clipped: Option<bool>,
    budget_chars: Option<u64>,
    budget_lines: Option<u64>,
    clip_mode: Option<String>,
    clip_footer: Option<bool>,
    rtk_used: Option<bool>,
    capture_provider: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct UsageStats {
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
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
    println!("  doctor             Run non-interactive environment checks");
    println!(
        "  llm <op> [...]     Manage LLM backend/model defaults (show|set-backend|set-model|clear-model)"
    );
    println!("  state <op> [...]   Manage repo state JSON (show|get|set)");
    println!("  policy [check ...] Show safety rules or classify a command");
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
    println!("  log-off            Disable cx logging in this process");
    println!("  alert-show         Show active alert thresholds/toggles");
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
    println!("  optimize [N]       Recommend cost/latency improvements from last N runs");
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

fn repo_root() -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn resolve_log_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".codex").join("cxlogs").join("runs.jsonl"));
    }
    home_dir().map(|h| h.join(".codex").join("cxlogs").join("runs.jsonl"))
}

fn resolve_schema_fail_log_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(
            root.join(".codex")
                .join("cxlogs")
                .join("schema_failures.jsonl"),
        );
    }
    home_dir().map(|h| {
        h.join(".codex")
            .join("cxlogs")
            .join("schema_failures.jsonl")
    })
}

fn resolve_quarantine_dir() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".codex").join("quarantine"));
    }
    home_dir().map(|h| h.join(".codex").join("quarantine"))
}

fn resolve_state_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".codex").join("state.json"));
    }
    home_dir().map(|h| h.join(".codex").join("state.json"))
}

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", parent.display()))
}

fn append_jsonl(path: &Path, value: &Value) -> Result<(), String> {
    ensure_parent_dir(path)?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("failed opening {}: {e}", path.display()))?;
    let mut line =
        serde_json::to_string(value).map_err(|e| format!("failed json serialize for log: {e}"))?;
    line.push('\n');
    f.write_all(line.as_bytes())
        .map_err(|e| format!("failed writing {}: {e}", path.display()))
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

fn log_codex_run(
    tool: &str,
    prompt: &str,
    duration_ms: u64,
    usage: Option<&UsageStats>,
    capture: Option<&CaptureStats>,
    schema_ok: bool,
    schema_reason: Option<&str>,
    quarantine_id: Option<&str>,
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

    let row = json!({
      "ts": utc_now_iso(),
      "tool": tool,
      "cwd": cwd,
      "scope": scope,
      "repo_root": root,
      "duration_ms": duration_ms,
      "input_tokens": input,
      "cached_input_tokens": cached,
      "effective_input_tokens": effective,
      "output_tokens": output,
      "system_output_len_raw": cap.system_output_len_raw,
      "system_output_len_processed": cap.system_output_len_processed,
      "system_output_len_clipped": cap.system_output_len_clipped,
      "system_output_lines_raw": cap.system_output_lines_raw,
      "system_output_lines_processed": cap.system_output_lines_processed,
      "system_output_lines_clipped": cap.system_output_lines_clipped,
      "clipped": cap.clipped,
      "budget_chars": cap.budget_chars,
      "budget_lines": cap.budget_lines,
      "clip_mode": cap.clip_mode,
      "clip_footer": cap.clip_footer,
      "rtk_used": cap.rtk_used,
      "capture_provider": cap.capture_provider,
      "llm_backend": backend,
      "llm_model": if model.is_empty() { Value::Null } else { Value::String(model) },
      "schema_ok": schema_ok,
      "schema_reason": schema_reason.unwrap_or(""),
      "quarantine_id": quarantine_id.unwrap_or(""),
      "prompt_sha256": sha256_hex(prompt),
      "prompt_preview": prompt_preview(prompt, 180),
    });
    append_jsonl(&run_log, &row)
}

fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    format!("{:x}", digest)
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

fn quarantine_store(
    tool: &str,
    reason: &str,
    raw: &str,
    schema: &str,
    prompt: &str,
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
    };
    let file = qdir.join(format!("{id}.json"));
    let serialized = serde_json::to_string_pretty(&rec)
        .map_err(|e| format!("failed to serialize quarantine record: {e}"))?;
    fs::write(&file, serialized).map_err(|e| format!("failed to write {}: {e}", file.display()))?;
    Ok(id)
}

fn log_schema_failure(
    tool: &str,
    reason: &str,
    raw: &str,
    schema: &str,
    prompt: &str,
) -> Result<String, String> {
    let qid = quarantine_store(tool, reason, raw, schema, prompt)?;

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

    let run_failure = json!({
        "ts": utc_now_iso(),
        "tool": tool,
        "cwd": cwd,
        "scope": scope,
        "repo_root": root,
        "duration_ms": Value::Null,
        "input_tokens": Value::Null,
        "cached_input_tokens": Value::Null,
        "effective_input_tokens": Value::Null,
        "output_tokens": Value::Null,
        "schema_ok": false,
        "schema_reason": reason,
        "quarantine_id": qid
    });
    append_jsonl(&run_log, &run_failure)?;

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

fn default_state_value() -> Value {
    json!({
        "preferences": {
            "llm_backend": Value::Null,
            "ollama_model": Value::Null,
            "conventional_commits": Value::Null,
            "pr_summary_format": Value::Null
        },
        "alert_overrides": {},
        "last_model": Value::Null
    })
}

fn read_state_value() -> Option<Value> {
    let state_file = resolve_state_file()?;
    if !state_file.exists() {
        return None;
    }
    let mut s = String::new();
    File::open(state_file).ok()?.read_to_string(&mut s).ok()?;
    serde_json::from_str::<Value>(&s).ok()
}

fn ensure_state_value() -> Result<(PathBuf, Value), String> {
    let state_file =
        resolve_state_file().ok_or_else(|| "unable to resolve state file".to_string())?;
    if !state_file.exists() {
        ensure_parent_dir(&state_file)?;
        let initial = default_state_value();
        let mut serialized = serde_json::to_string_pretty(&initial)
            .map_err(|e| format!("failed to serialize default state: {e}"))?;
        serialized.push('\n');
        fs::write(&state_file, serialized)
            .map_err(|e| format!("failed to write {}: {e}", state_file.display()))?;
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

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
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
    })
}

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
    println!("ok");
    0
}

fn cmd_llm(args: &[String]) -> i32 {
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
        "set-backend" => {
            let Some(v) = args.get(1).map(|s| s.to_lowercase()) else {
                eprintln!("Usage: {APP_NAME} llm set-backend <codex|ollama>");
                return 2;
            };
            if v != "codex" && v != "ollama" {
                eprintln!("Usage: {APP_NAME} llm set-backend <codex|ollama>");
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
                eprintln!("Usage: {APP_NAME} llm set-model <ollama_model>");
                return 2;
            };
            if model.trim().is_empty() {
                eprintln!("Usage: {APP_NAME} llm set-model <ollama_model>");
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
            eprintln!(
                "Usage: {APP_NAME} llm <show|set-backend <codex|ollama>|set-model <model>|clear-model>"
            );
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
    let backend = llm_backend();
    let model = llm_model();
    let capture_provider = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "auto".to_string());
    let native_reduce = env::var("CX_NATIVE_REDUCE").unwrap_or_else(|_| "1".to_string());
    let rtk_min = env::var("CX_RTK_MIN_VERSION").unwrap_or_else(|_| "0.22.1".to_string());
    let rtk_max = env::var("CX_RTK_MAX_VERSION").unwrap_or_default();
    let rtk_usable = rtk_is_usable();
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
    println!("version: {APP_VERSION}");
    println!("cwd: {cwd}");
    println!("source: {source}");
    println!("log_file: {log_file}");
    println!("state_file: {state_file}");
    println!("quarantine_dir: {quarantine_dir}");
    println!("mode: {mode}");
    println!("llm_backend: {backend}");
    println!(
        "llm_model: {}",
        if model.is_empty() { "<unset>" } else { &model }
    );
    println!("schema_relaxed: {schema_relaxed}");
    println!("capture_provider: {capture_provider}");
    println!("native_reduce: {native_reduce}");
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
    println!("state.preferences.conventional_commits: {cc}");
    println!("state.preferences.pr_summary_format: {pr_fmt}");
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

fn print_where() -> i32 {
    let exe = env::current_exe()
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
    let backend = llm_backend();
    let model = llm_model();
    println!("binary: {exe}");
    println!("source: {source}");
    println!("llm_backend: {backend}");
    println!(
        "llm_model: {}",
        if model.is_empty() { "<unset>" } else { &model }
    );
    println!("log_file: {log_file}");
    println!("state_file: {state_file}");
    0
}

fn load_runs(log_file: &Path, limit: usize) -> Result<Vec<RunEntry>, String> {
    let file =
        File::open(log_file).map_err(|e| format!("cannot open {}: {e}", log_file.display()))?;
    let reader = BufReader::new(file);
    let mut runs: Vec<RunEntry> = Vec::new();
    for line in reader.lines() {
        let line = match line {
            Ok(v) => v,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<RunEntry>(&line) {
            runs.push(entry);
        }
    }
    if runs.len() > limit {
        let keep_from = runs.len() - limit;
        runs = runs.split_off(keep_from);
    }
    Ok(runs)
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn load_runs_appended(log_file: &Path, offset: u64) -> Result<Vec<RunEntry>, String> {
    let len = file_len(log_file);
    if len <= offset {
        return Ok(Vec::new());
    }
    let mut file =
        File::open(log_file).map_err(|e| format!("cannot open {}: {e}", log_file.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {}: {e}", log_file.display()))?;
    let start = (offset as usize).min(bytes.len());
    let tail = &bytes[start..];
    let slice = String::from_utf8_lossy(tail);
    let mut out = Vec::new();
    for line in slice.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<RunEntry>(line) {
            out.push(entry);
        }
    }
    Ok(out)
}

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

fn print_optimize(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("== cxrs optimize (last {n} runs) ==");
        println!("scoreboard: no runs");
        println!("log_file: {}", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs optimize: {e}");
            return 1;
        }
    };
    if runs.is_empty() {
        println!("== cxrs optimize (last {n} runs) ==");
        println!("scoreboard: no runs");
        println!("log_file: {}", log_file.display());
        return 0;
    }

    let max_ms = env_u64("CXALERT_MAX_MS", 12000);
    let max_eff = env_u64("CXALERT_MAX_EFF_IN", 8000);
    let total = runs.len() as u64;

    let mut tool_eff: HashMap<String, (u64, u64)> = HashMap::new();
    let mut tool_dur: HashMap<String, (u64, u64)> = HashMap::new();
    let mut alerts = 0u64;
    let mut schema_fails = 0u64;

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
        if r.tool
            .as_deref()
            .unwrap_or_default()
            .contains("schema_failure")
        {
            schema_fails += 1;
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

    println!("== cxrs optimize (last {n} runs) ==");
    println!(
        "scoreboard: runs={}, alerts={} ({}%), schema_failures={} ({}%)",
        total,
        alerts,
        ((alerts as f64 / total as f64) * 100.0).round() as i64,
        schema_fails,
        ((schema_fails as f64 / total as f64) * 100.0).round() as i64
    );
    match cache_all {
        Some(v) => println!("cache_hit_rate: {}%", (v * 100.0).round() as i64),
        None => println!("cache_hit_rate: n/a"),
    }
    match (first_cache, second_cache) {
        (Some(a), Some(b)) => println!(
            "cache_trend: first_half={}%, second_half={}%, delta={}pp",
            (a * 100.0).round() as i64,
            (b * 100.0).round() as i64,
            ((b - a) * 100.0).round() as i64
        ),
        _ => println!("cache_trend: n/a"),
    }

    println!();
    println!("Top tools by avg effective_input_tokens:");
    for (tool, avg_eff) in &top_eff {
        println!("- {tool}: {avg_eff}");
    }
    println!("Top tools by avg duration_ms:");
    for (tool, avg_dur) in &top_dur {
        println!("- {tool}: {avg_dur}ms");
    }

    println!();
    println!("Recommendations:");
    if let Some((tool, avg_eff)) = top_eff.first() {
        if *avg_eff > max_eff / 2 {
            println!(
                "- {tool} has high avg effective tokens ({avg_eff}); reduce prompt context and prefer strict schema output."
            );
        }
    }
    if let Some((tool, avg_dur)) = top_dur.first() {
        if *avg_dur > max_ms / 2 {
            println!(
                "- {tool} is latency-heavy ({avg_dur}ms avg); split large tasks and trim embedded command output."
            );
        }
    }
    if let (Some(a), Some(b)) = (first_cache, second_cache) {
        if b + 0.05 < a {
            println!(
                "- Cache hit rate dropped across the window; prompts may be drifting. Stabilize templates and reusable context."
            );
        }
    }
    if schema_fails > 0 {
        println!(
            "- Schema failures detected ({schema_fails}); inspect quarantine entries and tighten schema prompts."
        );
    }
    if alerts == 0 && schema_fails == 0 {
        println!("- No significant anomalies in this window.");
    }

    println!("log_file: {}", log_file.display());
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
    format!(
        "You are a structured output generator.\nReturn STRICT JSON ONLY. No markdown. No prose. No code fences.\nOutput MUST be a single valid JSON object matching the schema.\nSchema-strict mode: deterministic JSON only; reject ambiguity.\nSchema:\n{schema}\n\nTask input:\n{task_input}\n"
    )
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
                line = format!("{}…", line.chars().take(600).collect::<String>());
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

    fn clip_text(input: &str) -> (String, CaptureStats) {
        let budget_chars = env::var("CX_CONTEXT_BUDGET_CHARS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(12000);
        let budget_lines = env::var("CX_CONTEXT_BUDGET_LINES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(300);
        let clip_mode = env::var("CX_CONTEXT_CLIP_MODE").unwrap_or_else(|_| "smart".to_string());
        let clip_footer = env::var("CX_CONTEXT_CLIP_FOOTER")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(1)
            == 1;

        let original_chars = input.chars().count();
        let original_lines = input.lines().count();
        let lower = input.to_lowercase();
        let mode_used = match clip_mode.as_str() {
            "head" => "head",
            "tail" => "tail",
            _ => {
                if lower.contains("error") || lower.contains("fail") || lower.contains("warning") {
                    "tail"
                } else {
                    "head"
                }
            }
        };

        let lines: Vec<&str> = input.lines().collect();
        let line_limited = if lines.len() <= budget_lines {
            input.to_string()
        } else if mode_used == "tail" {
            lines[lines.len().saturating_sub(budget_lines)..].join("\n")
        } else {
            lines[..budget_lines].join("\n")
        };

        let char_limited = if line_limited.chars().count() <= budget_chars {
            line_limited
        } else if mode_used == "tail" {
            last_n_chars(&line_limited, budget_chars)
        } else {
            first_n_chars(&line_limited, budget_chars)
        };

        let kept_chars = char_limited.chars().count();
        let kept_lines = char_limited.lines().count();
        let clipped = kept_chars < original_chars || kept_lines < original_lines;
        let final_text = if clipped && clip_footer {
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
                budget_chars: Some(budget_chars as u64),
                budget_lines: Some(budget_lines as u64),
                clip_mode: Some(mode_used.to_string()),
                clip_footer: Some(clip_footer),
                rtk_used: None,
                capture_provider: None,
            },
        )
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
    let use_rtk = match provider_mode.as_str() {
        "rtk" => rtk_enabled && is_rtk_supported_prefix(&cmd[0]) && rtk_usable,
        "native" => false,
        _ => rtk_enabled && is_rtk_supported_prefix(&cmd[0]) && rtk_usable,
    };
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
    let (clipped_text, mut stats) = clip_text(&reduced);
    stats.rtk_used = Some(provider_used == "rtk");
    stats.capture_provider = Some(provider_used);
    Ok((clipped_text, status, stats))
}

fn run_git_capture(args: &[&str]) -> Result<(String, i32), String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("failed to execute git {}: {e}", args.join(" ")))?;
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

fn run_strict_schema(
    tool: &str,
    schema: &str,
    task_input: &str,
    capture: Option<&CaptureStats>,
) -> Result<String, String> {
    let full_prompt = build_strict_schema_prompt(schema, task_input);
    let started = Instant::now();
    let jsonl = run_llm_jsonl(&full_prompt)?;
    let raw = extract_agent_text(&jsonl).unwrap_or_default();
    let usage = usage_from_jsonl(&jsonl);
    if raw.trim().is_empty() {
        let qid = log_schema_failure(tool, "empty_agent_message", &raw, schema, task_input)
            .unwrap_or_else(|_| "".to_string());
        let _ = log_codex_run(
            tool,
            &full_prompt,
            started.elapsed().as_millis() as u64,
            Some(&usage),
            capture,
            false,
            Some("empty_agent_message"),
            Some(&qid),
        );
        return Err(format!(
            "empty response from {}; quarantine_id={qid}",
            llm_backend()
        ));
    }
    if serde_json::from_str::<Value>(&raw).is_err() {
        let qid = log_schema_failure(tool, "invalid_json", &raw, schema, task_input)
            .unwrap_or_else(|_| "".to_string());
        let _ = log_codex_run(
            tool,
            &full_prompt,
            started.elapsed().as_millis() as u64,
            Some(&usage),
            capture,
            false,
            Some("invalid_json"),
            Some(&qid),
        );
        return Err(format!(
            "invalid JSON response; quarantine_id={qid}; raw={raw}"
        ));
    }
    let _ = log_codex_run(
        tool,
        &full_prompt,
        started.elapsed().as_millis() as u64,
        Some(&usage),
        capture,
        true,
        None,
        None,
    );
    Ok(raw)
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

fn is_dangerous_cmd(cmd: &str) -> bool {
    let compact = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = compact.to_lowercase();

    if lower.contains(" sudo ") || lower.starts_with("sudo ") || lower.ends_with(" sudo") {
        return true;
    }
    if lower.contains("rm -rf")
        || lower.contains("rm -fr")
        || lower.contains("rm -r -f")
        || lower.contains("rm -f -r")
    {
        return true;
    }
    if lower.contains("curl ")
        && lower.contains('|')
        && (lower.contains("| bash") || lower.contains("| sh") || lower.contains("| zsh"))
    {
        return true;
    }
    if (lower.contains("chmod ") || lower.contains("chown "))
        && (lower.contains("/system") || lower.contains("/library") || lower.contains("/usr"))
        && !lower.contains("/usr/local")
    {
        return true;
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
        return true;
    }
    false
}

fn cmd_policy(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("check") {
        if args.len() < 2 {
            eprintln!("Usage: {APP_NAME} policy check <command...>");
            return 2;
        }
        let candidate = args[1..].join(" ");
        if is_dangerous_cmd(&candidate) {
            println!("dangerous");
            return 0;
        }
        println!("safe");
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
    let (captured, status, capture_stats) = match run_system_command_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cx: {e}");
            return 1;
        }
    };
    let started = Instant::now();
    let out = match run_llm_plain(&captured) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cx: {e}");
            return if status == 0 { 1 } else { status };
        }
    };
    let _ = log_codex_run(
        "cx",
        &captured,
        started.elapsed().as_millis() as u64,
        None,
        Some(&capture_stats),
        true,
        None,
        None,
    );
    print!("{out}");
    if status == 0 { 0 } else { status }
}

fn cmd_cxj(command: &[String]) -> i32 {
    let (captured, status, capture_stats) = match run_system_command_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxj: {e}");
            return 1;
        }
    };
    let started = Instant::now();
    let out = match run_llm_jsonl(&captured) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxj: {e}");
            return if status == 0 { 1 } else { status };
        }
    };
    let usage = usage_from_jsonl(&out);
    let _ = log_codex_run(
        "cxj",
        &captured,
        started.elapsed().as_millis() as u64,
        Some(&usage),
        Some(&capture_stats),
        true,
        None,
        None,
    );
    print!("{out}");
    if status == 0 { 0 } else { status }
}

fn cmd_cxo(command: &[String]) -> i32 {
    let (captured, status, capture_stats) = match run_system_command_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxo: {e}");
            return 1;
        }
    };
    let started = Instant::now();
    let out = match run_llm_jsonl(&captured) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxo: {e}");
            return if status == 0 { 1 } else { status };
        }
    };
    let usage = usage_from_jsonl(&out);
    let _ = log_codex_run(
        "cxo",
        &captured,
        started.elapsed().as_millis() as u64,
        Some(&usage),
        Some(&capture_stats),
        true,
        None,
        None,
    );
    let text = extract_agent_text(&out).unwrap_or_default();
    println!("{text}");
    if status == 0 { 0 } else { status }
}

fn cmd_cxol(command: &[String]) -> i32 {
    cmd_cx(command)
}

fn cmd_cxcopy(command: &[String]) -> i32 {
    let (captured, status, capture_stats) = match run_system_command_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxcopy: {e}");
            return 1;
        }
    };
    let started = Instant::now();
    let out = match run_llm_jsonl(&captured) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs cxcopy: {e}");
            return if status == 0 { 1 } else { status };
        }
    };
    let usage = usage_from_jsonl(&out);
    let _ = log_codex_run(
        "cxcopy",
        &captured,
        started.elapsed().as_millis() as u64,
        Some(&usage),
        Some(&capture_stats),
        true,
        None,
        None,
    );
    let text = extract_agent_text(&out).unwrap_or_default();
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
            if status == 0 { 0 } else { status }
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
    let started = Instant::now();
    let jsonl = match run_llm_jsonl(&prompt) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix: {e}");
            return status;
        }
    };
    let usage = usage_from_jsonl(&jsonl);
    let _ = log_codex_run(
        "cxfix",
        &prompt,
        started.elapsed().as_millis() as u64,
        Some(&usage),
        Some(&capture_stats),
        true,
        None,
        None,
    );
    let text = extract_agent_text(&jsonl).unwrap_or_default();
    println!("{text}");
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

    let schema = r#"{
  "commands": ["bash command 1", "bash command 2"]
}"#;
    let task_input = format!(
        "Based on the terminal command output below, propose the NEXT shell commands to run.\nReturn 1-6 commands in execution order.\n\nExecuted command:\n{}\nExit status: {}\n\nTERMINAL OUTPUT:\n{}",
        command.join(" "),
        exit_status,
        captured
    );
    let raw = match run_strict_schema("cxrs_next", schema, &task_input, Some(&capture_stats)) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };
    let commands = match parse_commands_array(&raw) {
        Ok(v) => v,
        Err(reason) => {
            match log_schema_failure("cxrs_next", &reason, &raw, schema, &task_input) {
                Ok(qid) => eprintln!("cxrs next: schema failure; quarantine_id={qid}"),
                Err(e) => eprintln!("cxrs next: failed to log schema failure: {e}"),
            }
            eprintln!("cxrs next: raw response follows:");
            eprintln!("{raw}");
            return 1;
        }
    };
    for cmd in commands {
        println!("{cmd}");
    }
    0
}

fn cmd_fix_run(command: &[String]) -> i32 {
    let (captured, exit_status, capture_stats) = match run_system_command_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };

    let schema = r#"{
  "analysis": "short explanation",
  "commands": ["cmd1", "cmd2"]
}"#;
    let task_input = format!(
        "You are my terminal debugging assistant.\nGiven the command, exit status, and output, provide concise remediation.\n\nCommand:\n{}\n\nExit status: {}\n\nOutput:\n{}",
        command.join(" "),
        exit_status,
        captured
    );
    let raw = match run_strict_schema("cxrs_fix_run", schema, &task_input, Some(&capture_stats)) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };
    let v: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: invalid JSON: {e}");
            return 1;
        }
    };
    let analysis = v
        .get("analysis")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let commands = match parse_commands_array(&raw) {
        Ok(v) => v,
        Err(reason) => {
            let qid = log_schema_failure("cxrs_fix_run", &reason, &raw, schema, &task_input)
                .unwrap_or_else(|_| "".to_string());
            eprintln!("cxrs fix-run: schema failure; quarantine_id={qid}");
            eprintln!("cxrs fix-run: raw response follows:");
            eprintln!("{raw}");
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
    if !should_run {
        println!("Not running suggested commands (set CXFIX_RUN=1 to execute).");
        return if exit_status == 0 { 0 } else { exit_status };
    }

    for c in commands {
        if is_dangerous_cmd(&c) && !force {
            eprintln!("WARN blocked dangerous command (set CXFIX_FORCE=1 to override): {c}");
            continue;
        }
        if is_dangerous_cmd(&c) && force {
            eprintln!("WARN force-running dangerous command due to CXFIX_FORCE=1: {c}");
        }
        println!("-> {c}");
        let status = Command::new("bash").args(["-lc", &c]).status();
        if let Err(e) = status {
            eprintln!("cxrs fix-run: failed to execute command: {e}");
        }
    }

    if exit_status == 0 { 0 } else { exit_status }
}

fn generate_commitjson_value() -> Result<Value, String> {
    let (diff_out, status) = run_git_capture(&["diff", "--staged", "--no-color"])?;
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
    let schema = r#"{
  "subject": "string <= 72 chars",
  "body": ["bullet string", "..."],
  "breaking": false,
  "scope": "optional string",
  "tests": ["bullet string", "..."]
}"#;
    let task_input = format!(
        "Generate a commit object from this STAGED diff.\n{style_hint}\n\nSTAGED DIFF:\n{diff_out}"
    );
    let raw = run_strict_schema("cxrs_commitjson", schema, &task_input, None)?;
    let mut v: Value = serde_json::from_str(&raw).map_err(|e| format!("invalid JSON: {e}"))?;

    let has_subject = v.get("subject").and_then(Value::as_str).is_some();
    let has_body = v.get("body").and_then(Value::as_array).is_some();
    let has_breaking = v.get("breaking").and_then(Value::as_bool).is_some();
    let has_tests = v.get("tests").and_then(Value::as_array).is_some();
    if !(has_subject && has_body && has_breaking && has_tests) {
        let reason = "missing_or_invalid_required_keys:subject,body,breaking,tests";
        let qid = log_schema_failure("cxrs_commitjson", reason, &raw, schema, &task_input)
            .unwrap_or_else(|_| "".to_string());
        return Err(format!(
            "schema validation failed; quarantine_id={qid}; raw={raw}"
        ));
    }
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
    let git_args = if staged {
        vec!["diff", "--staged", "--no-color"]
    } else {
        vec!["diff", "--no-color"]
    };
    let (diff_out, status) = run_git_capture(&git_args)?;
    if status != 0 {
        return Err(format!(
            "git {} failed with status {status}",
            git_args.join(" ")
        ));
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
    let schema = r#"{
  "title": "short title",
  "summary": ["bullet", "bullet"],
  "risk_edge_cases": ["bullet", "bullet"],
  "suggested_tests": ["bullet", "bullet"]
}"#;
    let diff_label = if staged { "STAGED DIFF" } else { "DIFF" };
    let task_input = format!(
        "Write a PR-ready summary of this diff.\nKeep bullets concise and actionable.\nPreferred PR summary format: {pr_fmt}\n\n{diff_label}:\n{diff_out}"
    );
    let raw = run_strict_schema(tool, schema, &task_input, None)?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("invalid JSON: {e}"))?;

    let has_title = v.get("title").and_then(Value::as_str).is_some();
    let has_summary = v.get("summary").is_some();
    let has_risks = v.get("risk_edge_cases").is_some();
    let has_tests = v.get("suggested_tests").is_some();
    if !(has_title && has_summary && has_risks && has_tests) {
        let reason =
            "missing_or_invalid_required_keys:title,summary,risk_edge_cases,suggested_tests";
        let qid = log_schema_failure(tool, reason, &raw, schema, &task_input)
            .unwrap_or_else(|_| "".to_string());
        return Err(format!(
            "schema validation failed; quarantine_id={qid}; raw={raw}"
        ));
    }
    Ok(v)
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
        "cxversion" | "version" => {
            print_version();
            0
        }
        "cxdoctor" | "doctor" => print_doctor(),
        "cxwhere" | "where" => print_where(),
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
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(200);
            print_optimize(n)
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
        "cxlog_off" | "log-off" => cmd_log_off(),
        "cxalert_show" | "alert-show" => cmd_alert_show(),
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
        "cxversion"
            | "version"
            | "cxdoctor"
            | "doctor"
            | "cxwhere"
            | "where"
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
            | "cxlog_off"
            | "log-off"
            | "cxalert_show"
            | "alert-show"
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
    )
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    let code = match cmd {
        "help" | "-h" | "--help" => {
            print_help();
            0
        }
        "version" | "-V" | "--version" => {
            print_version();
            0
        }
        "where" => print_where(),
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
        "log-off" => cmd_log_off(),
        "alert-show" => cmd_alert_show(),
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
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(200);
            print_optimize(n)
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
    std::process::exit(code);
}

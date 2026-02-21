use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const APP_NAME: &str = "cxrs";
const APP_DESC: &str = "Rust spike for the cx toolchain";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

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
}

#[derive(Debug, Deserialize, Default)]
struct CxState {
    #[serde(default)]
    preferences: Preferences,
}

#[derive(Debug, Deserialize, Default)]
struct Preferences {
    #[serde(default)]
    conventional_commits: Option<bool>,
    #[serde(default)]
    pr_summary_format: Option<String>,
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

fn print_help() {
    println!("{APP_NAME} - {APP_DESC}");
    println!();
    println!("Usage:");
    println!("  {APP_NAME} <command> [args]");
    println!();
    println!("Commands:");
    println!("  version            Print tool version");
    println!("  doctor             Run non-interactive environment checks");
    println!("  profile [N]        Summarize last N runs from resolved cx log (default 50)");
    println!("  trace [N]          Show Nth most-recent run from resolved cx log (default 1)");
    println!("  next <cmd...>      Suggest next shell commands from command output (strict JSON)");
    println!("  diffsum            Summarize unstaged diff (strict schema)");
    println!("  diffsum-staged     Summarize staged diff (strict schema)");
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

fn read_state() -> Option<CxState> {
    let state_file = resolve_state_file()?;
    if !state_file.exists() {
        return None;
    }
    let mut s = String::new();
    File::open(state_file).ok()?.read_to_string(&mut s).ok()?;
    serde_json::from_str::<CxState>(&s).ok()
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
    let quarantine_dir = resolve_quarantine_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let (cc, pr_fmt) = if let Some(state) = read_state() {
        (
            state
                .preferences
                .conventional_commits
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string()),
            state
                .preferences
                .pr_summary_format
                .unwrap_or_else(|| "null".to_string()),
        )
    } else {
        ("n/a".to_string(), "n/a".to_string())
    };
    println!("name: {APP_NAME}");
    println!("version: {APP_VERSION}");
    println!("cwd: {cwd}");
    println!("source: {source}");
    println!("log_file: {log_file}");
    println!("state_file: {state_file}");
    println!("quarantine_dir: {quarantine_dir}");
    println!("mode: {mode}");
    println!("schema_relaxed: {schema_relaxed}");
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
    let required = ["git", "jq", "codex"];
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
    for bin in optional {
        if bin_in_path(bin) {
            println!("OK: {bin} (optional)");
        } else {
            println!("WARN: {bin} not found (optional)");
        }
    }
    if missing_required == 0 {
        println!("PASS: environment is ready for cxrs spike development.");
        0
    } else {
        println!("FAIL: install required binaries before using cxrs.");
        1
    }
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

fn build_strict_schema_prompt(schema: &str, task_input: &str) -> String {
    format!(
        "You are a structured output generator.\nReturn STRICT JSON ONLY. No markdown. No prose. No code fences.\nOutput MUST be a single valid JSON object matching the schema.\nSchema-strict mode: deterministic JSON only; reject ambiguity.\nSchema:\n{schema}\n\nTask input:\n{task_input}\n"
    )
}

fn run_system_command_capture(cmd: &[String]) -> Result<(String, i32), String> {
    if cmd.is_empty() {
        return Err("missing command".to_string());
    }
    let mut command = Command::new(&cmd[0]);
    if cmd.len() > 1 {
        command.args(&cmd[1..]);
    }
    let output = command
        .output()
        .map_err(|e| format!("failed to execute '{}': {e}", cmd[0]))?;
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

fn run_strict_schema(tool: &str, schema: &str, task_input: &str) -> Result<String, String> {
    let full_prompt = build_strict_schema_prompt(schema, task_input);
    let jsonl = run_codex_jsonl(&full_prompt)?;
    let raw = extract_agent_text(&jsonl).unwrap_or_default();
    if raw.trim().is_empty() {
        let qid = log_schema_failure(tool, "empty_agent_message", &raw, schema, task_input)
            .unwrap_or_else(|_| "".to_string());
        return Err(format!("empty response from codex; quarantine_id={qid}"));
    }
    if serde_json::from_str::<Value>(&raw).is_err() {
        let qid = log_schema_failure(tool, "invalid_json", &raw, schema, task_input)
            .unwrap_or_else(|_| "".to_string());
        return Err(format!(
            "invalid JSON response; quarantine_id={qid}; raw={raw}"
        ));
    }
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

fn cmd_next(command: &[String]) -> i32 {
    let (captured, exit_status) = match run_system_command_capture(command) {
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
    let full_prompt = build_strict_schema_prompt(schema, &task_input);
    let jsonl = match run_codex_jsonl(&full_prompt) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };
    let raw = extract_agent_text(&jsonl).unwrap_or_default();
    if raw.trim().is_empty() {
        match log_schema_failure(
            "cxrs_next",
            "empty_agent_message",
            &raw,
            schema,
            &task_input,
        ) {
            Ok(qid) => eprintln!("cxrs next: empty response; quarantine_id={qid}"),
            Err(e) => eprintln!("cxrs next: failed to log schema failure: {e}"),
        }
        return 1;
    }
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

    let conventional = read_state()
        .and_then(|s| s.preferences.conventional_commits)
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
    let raw = run_strict_schema("cxrs_commitjson", schema, &task_input)?;
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

    let pr_fmt = read_state()
        .and_then(|s| s.preferences.pr_summary_format)
        .unwrap_or_else(|| "standard".to_string());
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
    let raw = run_strict_schema(tool, schema, &task_input)?;
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
    let jsonl = match run_codex_jsonl(&full_prompt) {
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
        "doctor" => print_doctor(),
        "profile" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(50);
            print_profile(n)
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

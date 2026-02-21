use serde::Deserialize;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn print_help() {
    println!("{APP_NAME} - {APP_DESC}");
    println!();
    println!("Usage:");
    println!("  {APP_NAME} <command> [args]");
    println!();
    println!("Commands:");
    println!("  version     Print tool version");
    println!("  doctor      Run non-interactive environment checks");
    println!("  profile [N] Summarize last N runs from resolved cx log (default 50)");
    println!("  trace [N]   Show Nth most-recent run from resolved cx log (default 1)");
    println!("  help        Print this help");
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

fn resolve_state_file() -> Option<PathBuf> {
    if let Some(root) = repo_root() {
        return Some(root.join(".codex").join("state.json"));
    }
    home_dir().map(|h| h.join(".codex").join("state.json"))
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
        _ => {
            eprintln!("{APP_NAME}: unknown command '{cmd}'");
            eprintln!("Run '{APP_NAME} help' for usage.");
            2
        }
    };
    std::process::exit(code);
}

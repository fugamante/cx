use std::process::Command;

use serde_json::{Value, json};

use crate::config::app_config;
use crate::logs::load_values;
use crate::paths::resolve_log_file;
use crate::runtime::{llm_backend, llm_model};
use crate::state::set_state_path;

fn valid_policy(s: &str) -> bool {
    matches!(s, "latency" | "quality" | "cost" | "balanced")
}

fn parse_set_policy(args: &[String]) -> Result<String, String> {
    let mut i = 0usize;
    let mut policy: Option<String> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--policy" => {
                let Some(v) = args.get(i + 1) else {
                    return Err("cxrs broker set: --policy requires a value".to_string());
                };
                policy = Some(v.trim().to_lowercase());
                i += 2;
            }
            other => {
                return Err(format!("cxrs broker set: unknown flag '{other}'"));
            }
        }
    }
    let Some(v) = policy else {
        return Err("cxrs broker set: missing --policy".to_string());
    };
    if !valid_policy(&v) {
        return Err(format!("cxrs broker set: invalid policy '{v}'"));
    }
    Ok(v)
}

fn backend_available(name: &str) -> bool {
    let disabled = match name {
        "codex" => std::env::var("CX_DISABLE_CODEX")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        "ollama" => std::env::var("CX_DISABLE_OLLAMA")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        _ => false,
    };
    if disabled {
        return false;
    }
    Command::new("bash")
        .args(["-lc", &format!("command -v {name} >/dev/null 2>&1")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
struct BenchmarkArgs {
    backends: Vec<String>,
    window: usize,
    as_json: bool,
    strict: bool,
    min_runs: usize,
}

#[derive(Debug, Clone, Copy)]
struct BackendStats {
    runs: u64,
    avg_duration_ms: u64,
    p95_duration_ms: u64,
    avg_effective_input_tokens: u64,
    avg_output_tokens: u64,
}

fn parse_benchmark_args(args: &[String]) -> Result<BenchmarkArgs, String> {
    let mut i = 0usize;
    let mut backends: Vec<String> = Vec::new();
    let mut window = 200usize;
    let mut as_json = false;
    let mut strict = false;
    let mut min_runs = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--backend" => {
                let Some(v) = args.get(i + 1) else {
                    return Err("cxrs broker benchmark: --backend requires a value".to_string());
                };
                let b = v.trim().to_lowercase();
                if !matches!(b.as_str(), "codex" | "ollama") {
                    return Err(format!("cxrs broker benchmark: invalid backend '{b}'"));
                }
                if !backends.iter().any(|x| x == &b) {
                    backends.push(b);
                }
                i += 2;
            }
            "--window" => {
                let Some(v) = args.get(i + 1) else {
                    return Err("cxrs broker benchmark: --window requires a value".to_string());
                };
                window = v.parse::<usize>().map_err(|_| {
                    format!(
                        "cxrs broker benchmark: --window expects a positive integer, got '{}'",
                        v
                    )
                })?;
                if window == 0 {
                    return Err("cxrs broker benchmark: --window must be >= 1".to_string());
                }
                i += 2;
            }
            "--json" => {
                as_json = true;
                i += 1;
            }
            "--strict" => {
                strict = true;
                i += 1;
            }
            "--min-runs" => {
                let Some(v) = args.get(i + 1) else {
                    return Err("cxrs broker benchmark: --min-runs requires a value".to_string());
                };
                min_runs = v.parse::<usize>().map_err(|_| {
                    format!(
                        "cxrs broker benchmark: --min-runs expects a positive integer, got '{}'",
                        v
                    )
                })?;
                if min_runs == 0 {
                    return Err("cxrs broker benchmark: --min-runs must be >= 1".to_string());
                }
                i += 2;
            }
            other => {
                return Err(format!("cxrs broker benchmark: unknown flag '{other}'"));
            }
        }
    }
    if backends.is_empty() {
        backends = vec!["codex".to_string(), "ollama".to_string()];
    }
    Ok(BenchmarkArgs {
        backends,
        window,
        as_json,
        strict,
        min_runs,
    })
}

fn field_backend(row: &Value) -> Option<String> {
    row.get("backend_selected")
        .and_then(Value::as_str)
        .or_else(|| row.get("backend_used").and_then(Value::as_str))
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

fn metric_u64(row: &Value, key: &str) -> Option<u64> {
    row.get(key).and_then(Value::as_u64).or_else(|| {
        row.get(key)
            .and_then(Value::as_i64)
            .and_then(|n| u64::try_from(n).ok())
    })
}

fn average_u64(sum: u128, count: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    u64::try_from(sum / count as u128).unwrap_or(u64::MAX)
}

fn percentile_95(values: &mut [u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let idx = ((values.len() - 1) * 95) / 100;
    values[idx]
}

fn compute_backend_stats(rows: &[Value], backend: &str) -> BackendStats {
    let mut durations: Vec<u64> = Vec::new();
    let mut sum_duration = 0u128;
    let mut sum_eff = 0u128;
    let mut sum_out = 0u128;
    for row in rows {
        if field_backend(row).as_deref() != Some(backend) {
            continue;
        }
        let duration = metric_u64(row, "duration_ms").unwrap_or(0);
        let eff = metric_u64(row, "effective_input_tokens").unwrap_or(0);
        let out = metric_u64(row, "output_tokens").unwrap_or(0);
        durations.push(duration);
        sum_duration += duration as u128;
        sum_eff += eff as u128;
        sum_out += out as u128;
    }
    let runs = durations.len() as u64;
    let p95_duration_ms = percentile_95(&mut durations);
    BackendStats {
        runs,
        avg_duration_ms: average_u64(sum_duration, runs as usize),
        p95_duration_ms,
        avg_effective_input_tokens: average_u64(sum_eff, runs as usize),
        avg_output_tokens: average_u64(sum_out, runs as usize),
    }
}

fn cmd_broker_benchmark(app_name: &str, args: &[String]) -> i32 {
    let parsed = match parse_benchmark_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!(
                "{e}\nUsage: {app_name} broker benchmark [--backend codex|ollama]... [--window N] [--json] [--strict] [--min-runs N]"
            );
            return 2;
        }
    };

    let Some(log_file) = resolve_log_file() else {
        crate::cx_eprintln!("cxrs broker benchmark: unable to resolve log file");
        return 1;
    };
    let rows = match load_values(&log_file, parsed.window) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("cxrs broker benchmark: {e}");
            return 1;
        }
    };

    let mut entries: Vec<(String, BackendStats)> = Vec::new();
    for backend in &parsed.backends {
        entries.push((backend.clone(), compute_backend_stats(&rows, backend)));
    }
    if parsed.strict {
        let mut violations: Vec<String> = Vec::new();
        for (backend, s) in &entries {
            if s.runs < parsed.min_runs as u64 {
                violations.push(format!(
                    "{backend}: runs={} below min_runs={}",
                    s.runs, parsed.min_runs
                ));
            }
        }
        if !violations.is_empty() {
            if parsed.as_json {
                let out = json!({
                    "window": parsed.window,
                    "log_file": log_file.display().to_string(),
                    "summary": entries.iter().map(|(backend, s)| {
                        json!({
                            "backend": backend,
                            "runs": s.runs,
                            "avg_duration_ms": s.avg_duration_ms,
                            "p95_duration_ms": s.p95_duration_ms,
                            "avg_effective_input_tokens": s.avg_effective_input_tokens,
                            "avg_output_tokens": s.avg_output_tokens
                        })
                    }).collect::<Vec<Value>>(),
                    "strict": true,
                    "min_runs": parsed.min_runs,
                    "violations": violations
                });
                match serde_json::to_string_pretty(&out) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        crate::cx_eprintln!("cxrs broker benchmark: failed to render json: {e}");
                    }
                }
            } else {
                crate::cx_eprintln!(
                    "cxrs broker benchmark: strict check failed (min_runs={})",
                    parsed.min_runs
                );
                for v in violations {
                    crate::cx_eprintln!("  - {v}");
                }
            }
            return 1;
        }
    }

    if parsed.as_json {
        let summary: Vec<Value> = entries
            .iter()
            .map(|(backend, s)| {
                json!({
                    "backend": backend,
                    "runs": s.runs,
                    "avg_duration_ms": s.avg_duration_ms,
                    "p95_duration_ms": s.p95_duration_ms,
                    "avg_effective_input_tokens": s.avg_effective_input_tokens,
                    "avg_output_tokens": s.avg_output_tokens
                })
            })
            .collect();
        let out = json!({
            "window": parsed.window,
            "log_file": log_file.display().to_string(),
            "summary": summary,
            "strict": parsed.strict,
            "min_runs": parsed.min_runs,
            "violations": []
        });
        match serde_json::to_string_pretty(&out) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("cxrs broker benchmark: failed to render json: {e}");
                return 1;
            }
        }
        return 0;
    }

    println!("== cx broker benchmark ==");
    println!("window: {}", parsed.window);
    println!("log_file: {}", log_file.display());
    for (backend, s) in entries {
        println!();
        println!("backend: {backend}");
        println!("  runs: {}", s.runs);
        println!("  avg_duration_ms: {}", s.avg_duration_ms);
        println!("  p95_duration_ms: {}", s.p95_duration_ms);
        println!(
            "  avg_effective_input_tokens: {}",
            s.avg_effective_input_tokens
        );
        println!("  avg_output_tokens: {}", s.avg_output_tokens);
    }
    if parsed.strict {
        println!();
        println!("strict: pass (min_runs={})", parsed.min_runs);
    }
    0
}

pub fn cmd_broker(app_name: &str, args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("show");
    match sub {
        "show" => {
            let active_backend = llm_backend();
            let active_model = llm_model();
            let policy = app_config().broker_policy.clone();
            let codex_ok = backend_available("codex");
            let ollama_ok = backend_available("ollama");

            if args.iter().any(|a| a == "--json") {
                let out = json!({
                    "broker_policy": policy,
                    "active_backend": active_backend,
                    "active_model": if active_model.is_empty() { Value::Null } else { json!(active_model) },
                    "availability": {
                        "codex": codex_ok,
                        "ollama": ollama_ok
                    }
                });
                match serde_json::to_string_pretty(&out) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        crate::cx_eprintln!("cxrs broker show: failed to render json: {e}");
                        return 1;
                    }
                }
                return 0;
            }

            println!("== cx broker ==");
            println!("policy: {policy}");
            println!("active_backend: {active_backend}");
            println!(
                "active_model: {}",
                if active_model.is_empty() {
                    "<unset>"
                } else {
                    &active_model
                }
            );
            println!(
                "availability.codex: {}",
                if codex_ok { "yes" } else { "no" }
            );
            println!(
                "availability.ollama: {}",
                if ollama_ok { "yes" } else { "no" }
            );
            0
        }
        "set" => {
            let policy = match parse_set_policy(&args[1..]) {
                Ok(v) => v,
                Err(e) => {
                    crate::cx_eprintln!(
                        "{e}\nUsage: {app_name} broker set --policy latency|quality|cost|balanced"
                    );
                    return 2;
                }
            };
            if let Err(e) =
                set_state_path("preferences.broker_policy", Value::String(policy.clone()))
            {
                crate::cx_eprintln!("cxrs broker set: {e}");
                return 1;
            }
            println!("broker_policy: {policy}");
            0
        }
        "benchmark" => cmd_broker_benchmark(app_name, &args[1..]),
        other => {
            crate::cx_eprintln!(
                "Usage: {app_name} broker <show [--json] | set --policy latency|quality|cost|balanced | benchmark [--backend codex|ollama]... [--window N] [--json] [--strict] [--min-runs N]>"
            );
            crate::cx_eprintln!("cxrs broker: unknown subcommand '{other}'");
            2
        }
    }
}

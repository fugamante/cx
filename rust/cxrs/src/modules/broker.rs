use std::process::Command;

use serde_json::{Value, json};

use crate::config::{app_config, cli_app_name};
use crate::contract_versions::{
    BROKER_BENCHMARK_JSON_CONTRACT_VERSION, BROKER_SHOW_JSON_CONTRACT_VERSION,
};
use crate::logs::load_values;
use crate::paths::resolve_log_file;
use crate::provider_adapter::adapter_policy_value;
use crate::runtime::{llm_backend, llm_model};
use crate::state::set_state_path;

fn valid_policy(s: &str) -> bool {
    matches!(
        s,
        "latency" | "quality" | "cost" | "balanced" | "quota_saver"
    )
}

fn broker_error(action: &str, detail: &str) -> String {
    format!("{} broker {action}: {detail}", cli_app_name())
}

fn parse_set_policy(args: &[String]) -> Result<String, String> {
    let mut i = 0usize;
    let mut policy: Option<String> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--policy" => {
                let Some(v) = args.get(i + 1) else {
                    return Err(broker_error("set", "--policy requires a value"));
                };
                policy = Some(v.trim().to_lowercase());
                i += 2;
            }
            other => {
                return Err(broker_error("set", &format!("unknown flag '{other}'")));
            }
        }
    }
    let Some(v) = policy else {
        return Err(broker_error("set", "missing --policy"));
    };
    if !valid_policy(&v) {
        return Err(broker_error("set", &format!("invalid policy '{v}'")));
    }
    Ok(v)
}

fn backend_available(name: &str) -> bool {
    let disabled = match name {
        "primary" => std::env::var("CX_DISABLE_CODEX")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        "ollama" => std::env::var("CX_DISABLE_OLLAMA")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        "llamacpp" => std::env::var("CX_DISABLE_LLAMA_CPP")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        "mlx" => std::env::var("CX_DISABLE_MLX")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        _ => false,
    };
    if disabled {
        return false;
    }
    if name == "mlx" {
        let python = std::env::var("CX_MLX_PYTHON")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "python3".to_string());
        return Command::new(python)
            .args(["-c", "import mlx_lm"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    let bin = if name == "llamacpp" {
        std::env::var("CX_LLAMA_CPP_BIN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "llama-cli".to_string())
    } else {
        name.to_string()
    };
    Command::new("bash")
        .args(["-lc", &format!("command -v {bin} >/dev/null 2>&1")])
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
    severity: String,
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
    let mut severity = "critical".to_string();
    while i < args.len() {
        match args[i].as_str() {
            "--backend" => {
                let Some(v) = args.get(i + 1) else {
                    return Err(broker_error("benchmark", "--backend requires a value"));
                };
                let b = match v.trim().to_lowercase().as_str() {
                    "llama.cpp" | "llama_cpp" => "llamacpp".to_string(),
                    other => other.to_string(),
                };
                if !matches!(b.as_str(), "primary" | "ollama" | "llamacpp" | "mlx") {
                    return Err(broker_error("benchmark", &format!("invalid backend '{b}'")));
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
                    broker_error(
                        "benchmark",
                        &format!("--window expects a positive integer, got '{}'", v),
                    )
                })?;
                if window == 0 {
                    return Err(broker_error("benchmark", "--window must be >= 1"));
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
                    return Err(broker_error("benchmark", "--min-runs requires a value"));
                };
                min_runs = v.parse::<usize>().map_err(|_| {
                    broker_error(
                        "benchmark",
                        &format!("--min-runs expects a positive integer, got '{}'", v),
                    )
                })?;
                if min_runs == 0 {
                    return Err(broker_error("benchmark", "--min-runs must be >= 1"));
                }
                i += 2;
            }
            "--severity" => {
                let Some(v) = args.get(i + 1) else {
                    return Err(broker_error("benchmark", "--severity requires a value"));
                };
                let parsed = v.trim().to_lowercase();
                let normalized = match parsed.as_str() {
                    "warn" | "warning" => "warn",
                    "critical" => "critical",
                    _ => "",
                };
                if normalized.is_empty() {
                    return Err(broker_error(
                        "benchmark",
                        &format!("--severity expects warn|warning|critical, got '{}'", v),
                    ));
                }
                severity = normalized.to_string();
                i += 2;
            }
            other => {
                return Err(broker_error(
                    "benchmark",
                    &format!("unknown flag '{other}'"),
                ));
            }
        }
    }
    if backends.is_empty() {
        backends = vec!["primary".to_string(), "ollama".to_string()];
    }
    Ok(BenchmarkArgs {
        backends,
        window,
        as_json,
        strict,
        min_runs,
        severity,
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

#[derive(Debug, Clone)]
struct StrictViolation {
    backend: String,
    runs: u64,
    min_runs: usize,
    severity: String,
    message: String,
}

fn strict_violations(entries: &[(String, BackendStats)], min_runs: usize) -> Vec<StrictViolation> {
    let mut out: Vec<StrictViolation> = Vec::new();
    for (backend, s) in entries {
        if s.runs < min_runs as u64 {
            let sev = if s.runs == 0 { "critical" } else { "warn" };
            out.push(StrictViolation {
                backend: backend.clone(),
                runs: s.runs,
                min_runs,
                severity: sev.to_string(),
                message: format!("{backend}: runs={} below min_runs={min_runs}", s.runs),
            });
        }
    }
    out
}

fn cmd_broker_benchmark(app_name: &str, args: &[String]) -> i32 {
    let parsed = match parse_benchmark_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!(
                "{e}\nUsage: {app_name} broker benchmark [--backend primary|ollama]... [--window N] [--json] [--strict] [--min-runs N] [--severity warn|warning|critical]"
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
        let violations = strict_violations(&entries, parsed.min_runs);
        let warn_count = violations.iter().filter(|v| v.severity == "warn").count();
        let critical_count = violations
            .iter()
            .filter(|v| v.severity == "critical")
            .count();
        let should_fail = if parsed.severity == "warn" {
            !violations.is_empty()
        } else {
            critical_count > 0
        };
        if should_fail {
            if parsed.as_json {
                let out = json!({
                    "contract_version": BROKER_BENCHMARK_JSON_CONTRACT_VERSION,
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
                    "severity": parsed.severity,
                    "violations": violations.iter().map(|v| {
                        json!({
                            "backend": v.backend,
                            "runs": v.runs,
                            "min_runs": v.min_runs,
                            "severity": v.severity,
                            "message": v.message
                        })
                    }).collect::<Vec<Value>>(),
                    "violation_counts": {
                        "warn": warn_count,
                        "critical": critical_count
                    }
                });
                match serde_json::to_string_pretty(&out) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        crate::cx_eprintln!("cxrs broker benchmark: failed to render json: {e}");
                    }
                }
            } else {
                crate::cx_eprintln!(
                    "cxrs broker benchmark: strict check failed (min_runs={}, severity={})",
                    parsed.min_runs,
                    parsed.severity
                );
                for v in &violations {
                    crate::cx_eprintln!("  - [{}] {}", v.severity, v.message);
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
            "contract_version": BROKER_BENCHMARK_JSON_CONTRACT_VERSION,
            "window": parsed.window,
            "log_file": log_file.display().to_string(),
            "summary": summary,
            "strict": parsed.strict,
            "min_runs": parsed.min_runs,
            "severity": parsed.severity,
            "violations": [],
            "violation_counts": {
                "warn": 0,
                "critical": 0
            }
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
        println!(
            "strict: pass (min_runs={}, severity={})",
            parsed.min_runs, parsed.severity
        );
    }
    0
}

fn broker_show_value() -> Value {
    let active_backend = llm_backend();
    let active_model = llm_model();
    let policy = app_config().broker_policy.clone();
    let codex_ok = backend_available("primary");
    let ollama_ok = backend_available("ollama");
    let llamacpp_ok = backend_available("llamacpp");
    let mlx_ok = backend_available("mlx");
    let adapter_rollout_policy = adapter_policy_value();
    json!({
        "contract_version": BROKER_SHOW_JSON_CONTRACT_VERSION,
        "broker_policy": policy,
        "active_backend": active_backend,
        "active_model": if active_model.is_empty() { Value::Null } else { json!(active_model) },
        "availability": {
            "primary": codex_ok,
            "ollama": ollama_ok,
            "llamacpp": llamacpp_ok,
            "mlx": mlx_ok
        },
        "adapter_rollout_policy": adapter_rollout_policy
    })
}

pub fn cmd_broker(app_name: &str, args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("show");
    match sub {
        "show" => {
            if args.iter().any(|a| a == "--json") {
                let out = broker_show_value();
                match serde_json::to_string_pretty(&out) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        crate::cx_eprintln!("cxrs broker show: failed to render json: {e}");
                        return 1;
                    }
                }
                return 0;
            }
            let active_backend = llm_backend();
            let active_model = llm_model();
            let policy = app_config().broker_policy.clone();
            let codex_ok = backend_available("primary");
            let ollama_ok = backend_available("ollama");
            let llamacpp_ok = backend_available("llamacpp");
            let mlx_ok = backend_available("mlx");

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
                "availability.primary: {}",
                if codex_ok { "yes" } else { "no" }
            );
            println!(
                "availability.ollama: {}",
                if ollama_ok { "yes" } else { "no" }
            );
            println!(
                "availability.llamacpp: {}",
                if llamacpp_ok { "yes" } else { "no" }
            );
            println!("availability.mlx: {}", if mlx_ok { "yes" } else { "no" });
            0
        }
        "set" => {
            let policy = match parse_set_policy(&args[1..]) {
                Ok(v) => v,
                Err(e) => {
                    crate::cx_eprintln!(
                        "{e}\nUsage: {app_name} broker set --policy latency|quality|cost|balanced|quota_saver"
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
                "Usage: {app_name} broker <show [--json] | set --policy latency|quality|cost|balanced|quota_saver | benchmark [--backend primary|ollama]... [--window N] [--json] [--strict] [--min-runs N] [--severity warn|warning|critical]>"
            );
            crate::cx_eprintln!("cxrs broker: unknown subcommand '{other}'");
            2
        }
    }
}

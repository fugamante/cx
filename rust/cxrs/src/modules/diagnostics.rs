use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::config::app_config;
use crate::execmeta::{toolchain_version_string, utc_now_iso};
use crate::logs::file_len;
use crate::logs::load_values;
use crate::paths::{repo_root_hint, resolve_log_file};
use crate::process::{run_command_output_with_timeout, run_command_status_with_timeout};
use crate::routing::{bash_type_of_function, route_handler_for};
use crate::runtime::{llm_backend, llm_model};

fn bin_in_path(bin: &str) -> bool {
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", &format!("command -v {} >/dev/null 2>&1", bin)]);
    run_command_status_with_timeout(cmd, "command -v")
        .ok()
        .is_some_and(|s| s.success())
}

fn rtk_version_string() -> String {
    let mut cmd = Command::new("rtk");
    cmd.arg("--version");
    let out = match run_command_output_with_timeout(cmd, "rtk --version") {
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

fn resolved_provider(cfg_provider: &str) -> &'static str {
    match cfg_provider {
        "rtk" => "rtk",
        "native" => "native",
        _ => {
            if bin_in_path("rtk")
                && std::env::var("CX_RTK_SYSTEM").unwrap_or_else(|_| "1".to_string()) == "1"
            {
                "rtk"
            } else {
                "native"
            }
        }
    }
}

fn schema_count(schema_dir: &Path) -> usize {
    if !schema_dir.is_dir() {
        return 0;
    }
    fs::read_dir(schema_dir)
        .ok()
        .map(|iter| {
            iter.filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .count()
        })
        .unwrap_or(0)
}

fn last_run_id() -> String {
    resolve_log_file()
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
        .unwrap_or_else(|| "<none>".to_string())
}

fn percentile_u64(values: &[u64], pct: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let p = pct.clamp(0.0, 1.0);
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted.get(idx).copied()
}

fn print_scheduler_diag(log_file_path: &str) {
    let path = Path::new(log_file_path);
    if !path.exists() {
        println!("scheduler_window_runs: 0");
        println!("scheduler_queue_rows: 0");
        println!("scheduler_queue_ms_avg: n/a");
        println!("scheduler_queue_ms_p95: n/a");
        println!("scheduler_workers_seen: <none>");
        println!("scheduler_worker_distribution: <none>");
        println!("scheduler_backend_distribution: <none>");
        return;
    }
    let rows = load_values(path, 200).unwrap_or_default();
    let mut queue_vals: Vec<u64> = Vec::new();
    let mut worker_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut backend_counts: BTreeMap<String, usize> = BTreeMap::new();

    for v in &rows {
        if let Some(q) = v.get("queue_ms").and_then(Value::as_u64) {
            queue_vals.push(q);
        }
        if let Some(w) = v.get("worker_id").and_then(Value::as_str) {
            *worker_counts.entry(w.to_string()).or_insert(0) += 1;
        }
        if let Some(b) = v
            .get("backend_selected")
            .and_then(Value::as_str)
            .or_else(|| v.get("backend_used").and_then(Value::as_str))
        {
            *backend_counts.entry(b.to_string()).or_insert(0) += 1;
        }
    }

    let queue_avg = if queue_vals.is_empty() {
        None
    } else {
        Some(queue_vals.iter().sum::<u64>() / queue_vals.len() as u64)
    };
    let queue_p95 = percentile_u64(&queue_vals, 0.95);
    let workers_seen = if worker_counts.is_empty() {
        "<none>".to_string()
    } else {
        worker_counts
            .keys()
            .cloned()
            .collect::<Vec<String>>()
            .join(",")
    };
    let worker_distribution = if worker_counts.is_empty() {
        "<none>".to_string()
    } else {
        worker_counts
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<String>>()
            .join(",")
    };
    let backend_distribution = if backend_counts.is_empty() {
        "<none>".to_string()
    } else {
        backend_counts
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<String>>()
            .join(",")
    };

    println!("scheduler_window_runs: {}", rows.len());
    println!("scheduler_queue_rows: {}", queue_vals.len());
    println!(
        "scheduler_queue_ms_avg: {}",
        queue_avg
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "scheduler_queue_ms_p95: {}",
        queue_p95
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!("scheduler_workers_seen: {workers_seen}");
    println!("scheduler_worker_distribution: {worker_distribution}");
    println!("scheduler_backend_distribution: {backend_distribution}");
}

fn print_diag_header(app_version: &str, cfg: &crate::config::AppConfig) {
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    println!("== cxdiag ==");
    println!("timestamp: {}", utc_now_iso());
    println!("version: {}", toolchain_version_string(app_version));
    println!("mode: {}", cfg.cx_mode);
    println!("backend: {backend}");
    println!("active_model: {active_model}");
}

pub fn cmd_diag(app_version: &str) -> i32 {
    let cfg = app_config();
    let provider = cfg.capture_provider.clone();
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    let schema_dir = repo.join(".codex").join("schemas");

    print_diag_header(app_version, cfg);
    println!("capture_provider_config: {provider}");
    println!(
        "capture_provider_resolved: {}",
        resolved_provider(&provider)
    );
    println!("rtk_available: {}", bin_in_path("rtk"));
    println!("rtk_version: {}", rtk_version_string());
    println!("budget_chars: {}", cfg.budget_chars);
    println!("budget_lines: {}", cfg.budget_lines);
    println!("clip_mode: {}", cfg.clip_mode);
    println!("clip_footer: {}", if cfg.clip_footer { "1" } else { "0" });
    println!("log_file: {log_file}");
    println!("last_run_id: {}", last_run_id());
    println!("schema_registry_dir: {}", schema_dir.display());
    println!("schema_registry_files: {}", schema_count(&schema_dir));
    print_scheduler_diag(&log_file);

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

pub fn last_appended_json_value(log_file: &Path, offset: u64) -> Option<Value> {
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

pub fn has_required_log_fields(v: &Value) -> bool {
    let required = [
        "execution_id",
        "backend_used",
        "capture_provider",
        "execution_mode",
        "schema_enforced",
        "schema_valid",
        "duration_ms",
    ];
    required.iter().all(|k| v.get(k).is_some())
}

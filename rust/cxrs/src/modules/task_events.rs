use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::config::cli_app_name;
use crate::contract_versions::TASK_EVENTS_JSONL_CONTRACT_VERSION;
use crate::execmeta::utc_now_iso;
use crate::logs::{append_jsonl, file_len, load_values};
use crate::paths::task_events_log;

#[derive(Debug, Clone)]
pub struct TaskEvent<'a> {
    pub event: &'a str,
    pub task_id: Option<&'a str>,
    pub status: Option<&'a str>,
    pub backend: Option<&'a str>,
    pub requested_backend: Option<&'a str>,
    pub execution_id: Option<&'a str>,
    pub failure_class: Option<&'a str>,
    pub queue_ms: Option<u64>,
    pub wave_index: Option<u64>,
    pub wave_mode: Option<&'a str>,
    pub wave_size: Option<u64>,
    pub scheduled: Option<u64>,
    pub complete: Option<u64>,
    pub failed: Option<u64>,
    pub blocked: Option<u64>,
    pub critical_errors: Option<u64>,
    pub halted_remaining: Option<u64>,
}

impl<'a> TaskEvent<'a> {
    pub fn new(event: &'a str) -> Self {
        Self {
            event,
            task_id: None,
            status: None,
            backend: None,
            requested_backend: None,
            execution_id: None,
            failure_class: None,
            queue_ms: None,
            wave_index: None,
            wave_mode: None,
            wave_size: None,
            scheduled: None,
            complete: None,
            failed: None,
            blocked: None,
            critical_errors: None,
            halted_remaining: None,
        }
    }
}

pub fn emit(enabled: bool, event: TaskEvent<'_>) {
    if !enabled {
        return;
    }
    let payload = event_value(event);
    if let Some(path) = task_events_log()
        && let Err(e) = append_jsonl(&path, &payload)
    {
        crate::cx_eprintln!(
            "{} task events: failed to append event: {e}",
            cli_app_name()
        );
    }
    if let Ok(line) = serde_json::to_string(&payload) {
        crate::cx_eprintln!("{line}");
    }
}

fn event_value(event: TaskEvent<'_>) -> Value {
    let mut payload = serde_json::json!({
        "contract_version": TASK_EVENTS_JSONL_CONTRACT_VERSION,
        "event": event.event,
        "at": utc_now_iso()
    });
    let Some(obj) = payload.as_object_mut() else {
        return payload;
    };
    insert_str(obj, "task_id", event.task_id);
    insert_str(obj, "status", event.status);
    insert_str(obj, "backend", event.backend);
    insert_str(obj, "requested_backend", event.requested_backend);
    insert_str(obj, "execution_id", event.execution_id);
    insert_str(obj, "failure_class", event.failure_class);
    insert_u64(obj, "queue_ms", event.queue_ms);
    insert_u64(obj, "wave_index", event.wave_index);
    insert_str(obj, "wave_mode", event.wave_mode);
    insert_u64(obj, "wave_size", event.wave_size);
    insert_u64(obj, "scheduled", event.scheduled);
    insert_u64(obj, "complete", event.complete);
    insert_u64(obj, "failed", event.failed);
    insert_u64(obj, "blocked", event.blocked);
    insert_u64(obj, "critical_errors", event.critical_errors);
    insert_u64(obj, "halted_remaining", event.halted_remaining);
    payload
}

pub fn cmd_task_events(app_name: &str, args: &[String]) -> i32 {
    let usage = format!("Usage: {app_name} task events [--limit N] [--json|--jsonl] [--follow]");
    let mut limit = 50usize;
    let mut as_json = false;
    let mut follow = false;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{usage}");
                    return 2;
                };
                let Ok(n) = v.parse::<usize>() else {
                    crate::cx_eprintln!(
                        "{} task events: --limit must be an integer",
                        cli_app_name()
                    );
                    return 2;
                };
                limit = n;
                i += 2;
            }
            "--json" => {
                as_json = true;
                i += 1;
            }
            "--jsonl" => {
                as_json = false;
                i += 1;
            }
            "--follow" => {
                follow = true;
                i += 1;
            }
            other => {
                crate::cx_eprintln!("{} task events: unknown flag '{other}'", cli_app_name());
                return 2;
            }
        }
    }

    let Some(path) = task_events_log() else {
        crate::cx_eprintln!(
            "{} task events: unable to resolve task event log",
            cli_app_name()
        );
        return 1;
    };
    if as_json && follow {
        crate::cx_eprintln!(
            "{} task events: --follow requires --jsonl output",
            cli_app_name()
        );
        return 2;
    }
    if !path.exists() {
        if as_json {
            println!("[]");
        }
        return if follow { follow_events(&path, 0) } else { 0 };
    }
    let rows = match load_values(&path, limit) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} task events: {e}", cli_app_name());
            return 1;
        }
    };
    if as_json {
        match serde_json::to_string_pretty(&rows) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("{} task events: failed to render json: {e}", cli_app_name());
                return 1;
            }
        }
    } else {
        for row in &rows {
            match serde_json::to_string(row) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    crate::cx_eprintln!(
                        "{} task events: failed to render event jsonl: {e}",
                        cli_app_name()
                    );
                    return 1;
                }
            }
        }
    }
    if follow {
        follow_events(&path, file_len(&path))
    } else {
        0
    }
}

fn follow_events(path: &Path, mut offset: u64) -> i32 {
    loop {
        let rows = read_appended_values(path, offset);
        offset = file_len(path);
        for row in rows {
            match serde_json::to_string(&row) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    crate::cx_eprintln!(
                        "{} task events: failed to render event jsonl: {e}",
                        cli_app_name()
                    );
                    return 1;
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn read_appended_values(path: &Path, offset: u64) -> Vec<Value> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut reader = BufReader::new(file);
    if offset > 0 && reader.seek(SeekFrom::Start(offset)).is_err() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        let Ok(n) = reader.read_line(&mut line) else {
            break;
        };
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            out.push(v);
        }
    }
    out
}

fn insert_str(obj: &mut serde_json::Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        obj.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn insert_u64(obj: &mut serde_json::Map<String, Value>, key: &str, value: Option<u64>) {
    if let Some(value) = value {
        obj.insert(key.to_string(), Value::from(value));
    }
}

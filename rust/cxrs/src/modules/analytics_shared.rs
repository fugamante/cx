use serde_json::Value;

use crate::logs::load_runs;
use crate::paths::resolve_log_file;
use crate::types::RunEntry;

pub fn parse_ts_epoch(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp())
}

pub(super) fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(default)
}

pub(super) fn print_json_value(prefix: &str, v: &Value) -> i32 {
    match serde_json::to_string_pretty(v) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!("{prefix}: failed to render JSON: {e}");
            1
        }
    }
}

pub(super) fn load_runs_for(
    command: &str,
    n: usize,
) -> Result<(std::path::PathBuf, Vec<RunEntry>), i32> {
    let Some(log_file) = resolve_log_file() else {
        crate::cx_eprintln!("cxrs: unable to resolve log file");
        return Err(1);
    };
    if !log_file.exists() {
        return Ok((log_file, Vec::new()));
    }
    match load_runs(&log_file, n) {
        Ok(v) => Ok((log_file, v)),
        Err(e) => {
            crate::cx_eprintln!("cxrs {command}: {e}");
            Err(1)
        }
    }
}

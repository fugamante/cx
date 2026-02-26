use chrono::Utc;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::execmeta::utc_now_iso;
use crate::paths::resolve_quarantine_dir;
use crate::types::{QuarantineAttempt, QuarantineRecord};
use crate::util::sha256_hex;

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

pub fn quarantine_store_with_attempts(
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
pub fn quarantine_store(
    tool: &str,
    reason: &str,
    raw: &str,
    schema: &str,
    prompt: &str,
) -> Result<String, String> {
    quarantine_store_with_attempts(tool, reason, raw, schema, prompt, Vec::new())
}

fn quarantine_file_by_id(id: &str) -> Option<PathBuf> {
    let qdir = resolve_quarantine_dir()?;
    let path = qdir.join(format!("{id}.json"));
    if path.exists() { Some(path) } else { None }
}

pub fn read_quarantine_record(id: &str) -> Result<QuarantineRecord, String> {
    let path = quarantine_file_by_id(id).ok_or_else(|| format!("quarantine id not found: {id}"))?;
    let mut s = String::new();
    File::open(&path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?
        .read_to_string(&mut s)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("invalid quarantine JSON {}: {e}", path.display()))
}

fn read_quarantine_rows(qdir: &std::path::Path, n: usize) -> Vec<QuarantineRecord> {
    let mut rows: Vec<QuarantineRecord> = Vec::new();
    let Ok(rd) = fs::read_dir(qdir) else {
        return rows;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let Ok(s) = fs::read_to_string(&path) else {
            continue;
        };
        if s.trim().is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<QuarantineRecord>(&s) {
            rows.push(rec);
        }
    }
    rows.sort_by(|a, b| b.ts.cmp(&a.ts));
    if rows.len() > n {
        rows.truncate(n);
    }
    rows
}

pub fn cmd_quarantine_list(n: usize) -> i32 {
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

    let rows = read_quarantine_rows(&qdir, n);
    println!("== cxrs quarantine list ==");
    println!("entries: {}", rows.len());
    for rec in rows {
        println!("- {} | {} | {} | {}", rec.id, rec.ts, rec.tool, rec.reason);
    }
    println!("quarantine_dir: {}", qdir.display());
    0
}

pub fn cmd_quarantine_show(id: &str) -> i32 {
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

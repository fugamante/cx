use serde_json::Value;
use std::path::PathBuf;

use crate::error::{EXIT_OK, EXIT_RUNTIME, format_error};
use crate::llm::extract_agent_text;
use crate::quarantine::read_quarantine_record;
use crate::runlog::log_schema_failure;
use crate::schema::{build_strict_schema_prompt, validate_schema_instance};
use crate::types::LoadedSchema;

pub type JsonlRunner = fn(&str) -> Result<String, String>;

fn log_replay_schema_failure(rec: &crate::types::QuarantineRecord, reason: &str, raw: &str) {
    match log_schema_failure(
        &format!("{}_replay", rec.tool),
        reason,
        raw,
        &rec.schema,
        &rec.prompt,
        Vec::new(),
    ) {
        Ok(qid) => crate::cx_eprintln!(
            "{}",
            format_error("replay", &format!("{reason}; quarantine_id={qid}"))
        ),
        Err(e) => crate::cx_eprintln!(
            "{}",
            format_error("replay", &format!("failed to log schema failure: {e}"))
        ),
    }
}

fn ensure_quarantine_payload(rec: &crate::types::QuarantineRecord) -> Result<(), String> {
    if rec.schema.trim().is_empty() || rec.prompt.trim().is_empty() {
        return Err("quarantine entry is missing schema/prompt payload".to_string());
    }
    Ok(())
}

fn replay_schema_from_record(rec: &crate::types::QuarantineRecord) -> Result<LoadedSchema, String> {
    let value: Value = serde_json::from_str(&rec.schema)
        .map_err(|e| format!("quarantine schema is invalid JSON: {e}"))?;
    Ok(LoadedSchema {
        name: format!("{}_replay.schema.json", rec.tool),
        path: PathBuf::from(format!("<quarantine:{}>", rec.id)),
        value,
        id: None,
    })
}

fn replay_raw_response(
    rec: &crate::types::QuarantineRecord,
    run_llm_jsonl: JsonlRunner,
) -> Result<String, String> {
    let full_prompt = build_strict_schema_prompt(&rec.schema, &rec.prompt);
    let jsonl = run_llm_jsonl(&full_prompt)?;
    Ok(extract_agent_text(&jsonl).unwrap_or_default())
}

fn validate_replay_response(rec: &crate::types::QuarantineRecord, raw: &str) -> Result<(), String> {
    if raw.trim().is_empty() {
        return Err("empty_agent_message".to_string());
    }
    if serde_json::from_str::<Value>(raw).is_err() {
        return Err("invalid_json".to_string());
    }
    let schema = replay_schema_from_record(rec)?;
    validate_schema_instance(&schema, raw).map(|_| ())
}

pub fn cmd_replay(id: &str, run_llm_jsonl: JsonlRunner) -> i32 {
    let rec = match read_quarantine_record(id) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{}", format_error("replay", &e));
            return EXIT_RUNTIME;
        }
    };

    if let Err(e) = ensure_quarantine_payload(&rec) {
        crate::cx_eprintln!("{}", format_error("replay", &e));
        return EXIT_RUNTIME;
    }

    let raw = match replay_raw_response(&rec, run_llm_jsonl) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{}", format_error("replay", &e));
            return EXIT_RUNTIME;
        }
    };
    if let Err(reason) = validate_replay_response(&rec, &raw) {
        log_replay_schema_failure(&rec, &reason, &raw);
        if reason == "invalid_json" {
            crate::cx_eprintln!("{}", format_error("replay", "raw response follows:"));
            crate::cx_eprintln!("{raw}");
        }
        crate::cx_eprintln!("{}", format_error("replay", &reason));
        return EXIT_RUNTIME;
    }

    println!("{raw}");
    EXIT_OK
}

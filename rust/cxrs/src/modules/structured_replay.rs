use serde_json::Value;

use crate::error::{EXIT_OK, EXIT_RUNTIME, format_error};
use crate::llm::extract_agent_text;
use crate::quarantine::read_quarantine_record;
use crate::runlog::log_schema_failure;
use crate::schema::build_strict_schema_prompt;

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
        Ok(qid) => eprintln!(
            "{}",
            format_error("replay", &format!("{reason}; quarantine_id={qid}"))
        ),
        Err(e) => eprintln!(
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

pub fn cmd_replay(id: &str, run_llm_jsonl: JsonlRunner) -> i32 {
    let rec = match read_quarantine_record(id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", format_error("replay", &e));
            return EXIT_RUNTIME;
        }
    };

    if let Err(e) = ensure_quarantine_payload(&rec) {
        eprintln!("{}", format_error("replay", &e));
        return EXIT_RUNTIME;
    }

    let full_prompt = build_strict_schema_prompt(&rec.schema, &rec.prompt);
    let jsonl = match run_llm_jsonl(&full_prompt) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", format_error("replay", &e));
            return EXIT_RUNTIME;
        }
    };
    let raw = extract_agent_text(&jsonl).unwrap_or_default();
    if raw.trim().is_empty() {
        log_replay_schema_failure(&rec, "empty_agent_message", &raw);
        return EXIT_RUNTIME;
    }

    if serde_json::from_str::<Value>(&raw).is_err() {
        log_replay_schema_failure(&rec, "invalid_json", &raw);
        eprintln!("{}", format_error("replay", "raw response follows:"));
        eprintln!("{raw}");
        return EXIT_RUNTIME;
    }

    println!("{raw}");
    EXIT_OK
}

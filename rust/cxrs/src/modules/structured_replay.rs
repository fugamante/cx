use serde_json::Value;

use crate::error::{EXIT_OK, EXIT_RUNTIME, format_error};
use crate::llm::extract_agent_text;
use crate::quarantine::read_quarantine_record;
use crate::runlog::log_schema_failure;
use crate::schema::build_strict_schema_prompt;

pub type JsonlRunner = fn(&str) -> Result<String, String>;

pub fn cmd_replay(id: &str, run_llm_jsonl: JsonlRunner) -> i32 {
    let rec = match read_quarantine_record(id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", format_error("replay", &e));
            return EXIT_RUNTIME;
        }
    };

    if rec.schema.trim().is_empty() || rec.prompt.trim().is_empty() {
        eprintln!(
            "{}",
            format_error(
                "replay",
                "quarantine entry is missing schema/prompt payload"
            )
        );
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
        match log_schema_failure(
            &format!("{}_replay", rec.tool),
            "empty_agent_message",
            &raw,
            &rec.schema,
            &rec.prompt,
            Vec::new(),
        ) {
            Ok(qid) => eprintln!(
                "{}",
                format_error("replay", &format!("empty response; quarantine_id={qid}"))
            ),
            Err(e) => eprintln!(
                "{}",
                format_error("replay", &format!("failed to log schema failure: {e}"))
            ),
        }
        return EXIT_RUNTIME;
    }

    if serde_json::from_str::<Value>(&raw).is_err() {
        match log_schema_failure(
            &format!("{}_replay", rec.tool),
            "invalid_json",
            &raw,
            &rec.schema,
            &rec.prompt,
            Vec::new(),
        ) {
            Ok(qid) => eprintln!(
                "{}",
                format_error("replay", &format!("invalid JSON; quarantine_id={qid}"))
            ),
            Err(e) => eprintln!(
                "{}",
                format_error("replay", &format!("failed to log schema failure: {e}"))
            ),
        }
        eprintln!("{}", format_error("replay", "raw response follows:"));
        eprintln!("{raw}");
        return EXIT_RUNTIME;
    }

    println!("{raw}");
    EXIT_OK
}

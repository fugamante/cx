use serde_json::Value;
use std::env;
use std::path::PathBuf;
use std::process::Command;

use crate::capture::run_system_command_capture;
use crate::config::app_config;
use crate::llm::extract_agent_text;
use crate::paths::repo_root;
use crate::policy::{SafetyDecision, evaluate_command_safety};
use crate::quarantine::read_quarantine_record;
use crate::runlog::{RunLogInput, log_codex_run, log_schema_failure};
use crate::schema::{build_strict_schema_prompt, load_schema};
use crate::state::{read_state_value, value_at_path};
use crate::types::{ExecutionResult, LlmOutputKind, TaskInput, TaskSpec};

pub type ExecuteTaskFn = fn(TaskSpec) -> Result<ExecutionResult, String>;
pub type JsonlRunner = fn(&str) -> Result<String, String>;

fn parse_commands_array(raw: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;
    let arr = v
        .get("commands")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing required key 'commands' array".to_string())?;
    let mut out: Vec<String> = Vec::new();
    for item in arr {
        let Some(s) = item.as_str() else {
            return Err("commands array must contain strings".to_string());
        };
        if !s.trim().is_empty() {
            out.push(s.to_string());
        }
    }
    Ok(out)
}

fn render_bullets(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Some(Value::String(s)) => {
            if s.trim().is_empty() {
                Vec::new()
            } else {
                vec![s.trim().to_string()]
            }
        }
        _ => Vec::new(),
    }
}

fn print_diffsum_human(v: &Value) {
    let title = v.get("title").and_then(Value::as_str).unwrap_or("");
    let summary = render_bullets(v.get("summary"));
    let risks = render_bullets(v.get("risk_edge_cases"));
    let tests = render_bullets(v.get("suggested_tests"));

    println!("Title: {title}");
    println!();
    println!("Summary:");
    if summary.is_empty() {
        println!("- n/a");
    } else {
        for s in summary {
            println!("- {s}");
        }
    }
    println!();
    println!("Risk/edge cases:");
    if risks.is_empty() {
        println!("- n/a");
    } else {
        for s in risks {
            println!("- {s}");
        }
    }
    println!();
    println!("Suggested tests:");
    if tests.is_empty() {
        println!("- n/a");
    } else {
        for s in tests {
            println!("- {s}");
        }
    }
}

fn generate_commitjson_value(execute_task: ExecuteTaskFn) -> Result<Value, String> {
    let (diff_out, status, capture_stats) = run_system_command_capture(&[
        "git".to_string(),
        "diff".to_string(),
        "--staged".to_string(),
        "--no-color".to_string(),
    ])?;
    if status != 0 {
        return Err(format!(
            "git diff --staged failed with exit status {status}: {diff_out}"
        ));
    }
    if diff_out.trim().is_empty() {
        return Err("no staged changes. run: git add -p".to_string());
    }

    let conventional = read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.conventional_commits"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let style_hint = if conventional {
        "Use concise conventional-commit style subject."
    } else {
        "Use concise imperative subject (non-conventional format)."
    };
    let schema = load_schema("commitjson")?;
    let task_input = format!(
        "Generate a commit object from this STAGED diff.\n{style_hint}\n\nSTAGED DIFF:\n{diff_out}"
    );
    let result = execute_task(TaskSpec {
        command_name: "cxrs_commitjson".to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input),
        logging_enabled: true,
        capture_override: Some(capture_stats),
    })?;
    let mut v: Value = if result.schema_valid == Some(false) {
        let qid = result.quarantine_id.unwrap_or_default();
        return Err(format!(
            "schema validation failed; quarantine_id={qid}; raw={}",
            result.stdout
        ));
    } else {
        serde_json::from_str(&result.stdout).map_err(|e| format!("invalid JSON: {e}"))?
    };
    if v.get("scope").is_none() {
        if let Some(obj) = v.as_object_mut() {
            obj.insert("scope".to_string(), Value::Null);
        }
    }
    Ok(v)
}

fn generate_diffsum_value(
    tool: &str,
    staged: bool,
    execute_task: ExecuteTaskFn,
) -> Result<Value, String> {
    let git_cmd = if staged {
        vec![
            "git".to_string(),
            "diff".to_string(),
            "--staged".to_string(),
            "--no-color".to_string(),
        ]
    } else {
        vec![
            "git".to_string(),
            "diff".to_string(),
            "--no-color".to_string(),
        ]
    };
    let (diff_out, status, capture_stats) = run_system_command_capture(&git_cmd)?;
    if status != 0 {
        return Err(format!("git diff failed with status {status}"));
    }
    if diff_out.trim().is_empty() {
        if staged {
            return Err("no staged changes.".to_string());
        }
        return Err("no unstaged changes.".to_string());
    }

    let pr_fmt = read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "preferences.pr_summary_format"))
        .and_then(Value::as_str)
        .unwrap_or("standard")
        .to_string();
    let schema = load_schema("diffsum")?;
    let diff_label = if staged { "STAGED DIFF" } else { "DIFF" };
    let task_input = format!(
        "Write a PR-ready summary of this diff.\nKeep bullets concise and actionable.\nPreferred PR summary format: {pr_fmt}\n\n{diff_label}:\n{diff_out}"
    );
    let result = execute_task(TaskSpec {
        command_name: tool.to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input),
        logging_enabled: true,
        capture_override: Some(capture_stats),
    })?;
    if result.schema_valid == Some(false) {
        return Err(format!(
            "schema validation failed; quarantine_id={}; raw={}",
            result.quarantine_id.unwrap_or_default(),
            result.stdout
        ));
    }
    serde_json::from_str(&result.stdout).map_err(|e| format!("invalid JSON: {e}"))
}

pub fn cmd_next(command: &[String], execute_task: ExecuteTaskFn) -> i32 {
    let (captured, exit_status, capture_stats) = match run_system_command_capture(command) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };

    let schema = match load_schema("next") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };
    let task_input = format!(
        "Based on the terminal command output below, propose the NEXT shell commands to run.\nReturn 1-6 commands in execution order.\n\nExecuted command:\n{}\nExit status: {}\n\nTERMINAL OUTPUT:\n{}",
        command.join(" "),
        exit_status,
        captured
    );
    let result = match execute_task(TaskSpec {
        command_name: "cxrs_next".to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input.clone()),
        logging_enabled: true,
        capture_override: Some(capture_stats),
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: {e}");
            return 1;
        }
    };
    if result.schema_valid == Some(false) {
        if let Some(qid) = result.quarantine_id.as_deref() {
            eprintln!("cxrs next: schema failure; quarantine_id={qid}");
        }
        eprintln!("cxrs next: raw response follows:");
        eprintln!("{}", result.stdout);
        return 1;
    }
    let schema_value: Value = match serde_json::from_str(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: invalid JSON after schema run: {e}");
            return 1;
        }
    };
    let commands = match parse_commands_array(&schema_value.to_string()) {
        Ok(v) => v,
        Err(reason) => {
            eprintln!("cxrs next: {reason}");
            return 1;
        }
    };
    for cmd in commands {
        println!("{cmd}");
    }
    0
}

pub fn cmd_fix_run(app_name: &str, command: &[String], execute_task: ExecuteTaskFn) -> i32 {
    let mut unsafe_override = false;
    let mut cmdv = command.to_vec();
    if cmdv.first().map(String::as_str) == Some("--unsafe") {
        unsafe_override = true;
        cmdv = cmdv.into_iter().skip(1).collect();
    }
    if cmdv.is_empty() {
        eprintln!("Usage: {app_name} fix-run [--unsafe] <command> [args...]");
        return 2;
    }
    let (captured, exit_status, capture_stats) = match run_system_command_capture(&cmdv) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };

    let schema = match load_schema("fixrun") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };
    let task_input = format!(
        "You are my terminal debugging assistant.\nGiven the command, exit status, and output, provide concise remediation.\n\nCommand:\n{}\n\nExit status: {}\n\nOutput:\n{}",
        cmdv.join(" "),
        exit_status,
        captured
    );
    let result = match execute_task(TaskSpec {
        command_name: "cxrs_fix_run".to_string(),
        input: TaskInput::Prompt(task_input.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema.clone()),
        schema_task_input: Some(task_input.clone()),
        logging_enabled: false,
        capture_override: Some(capture_stats),
    }) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: {e}");
            return 1;
        }
    };
    if result.schema_valid == Some(false) {
        let _ = log_codex_run(RunLogInput {
            tool: "cxrs_fix_run",
            prompt: &task_input,
            duration_ms: result.duration_ms,
            usage: Some(&result.usage),
            capture: Some(&result.capture_stats),
            schema_ok: false,
            schema_reason: Some("schema_validation_failed"),
            schema_name: Some(schema.name.as_str()),
            quarantine_id: result.quarantine_id.as_deref(),
            policy_blocked: None,
            policy_reason: None,
        });
        if let Some(qid) = result.quarantine_id.as_deref() {
            eprintln!("cxrs fix-run: schema failure; quarantine_id={qid}");
        }
        eprintln!("cxrs fix-run: raw response follows:");
        eprintln!("{}", result.stdout);
        return 1;
    }
    let v: Value = match serde_json::from_str(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs fix-run: invalid JSON after schema run: {e}");
            return 1;
        }
    };
    let analysis = v
        .get("analysis")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let commands = match parse_commands_array(&result.stdout) {
        Ok(v) => v,
        Err(reason) => {
            eprintln!("cxrs fix-run: {reason}");
            return 1;
        }
    };

    if !analysis.is_empty() {
        println!("Analysis:");
        println!("{analysis}");
        println!();
    }
    println!("Suggested commands:");
    println!("-------------------");
    for c in &commands {
        println!("{c}");
    }
    println!("-------------------");

    let cfg = app_config();
    let should_run = cfg.cxfix_run;
    let force = cfg.cxfix_force;
    let unsafe_env = cfg.cx_unsafe;
    let allow_unsafe = unsafe_override || unsafe_env;
    if !should_run {
        println!("Not running suggested commands (set CXFIX_RUN=1 to execute).");
        let _ = log_codex_run(RunLogInput {
            tool: "cxrs_fix_run",
            prompt: &task_input,
            duration_ms: result.duration_ms,
            usage: Some(&result.usage),
            capture: Some(&result.capture_stats),
            schema_ok: true,
            schema_reason: None,
            schema_name: Some(schema.name.as_str()),
            quarantine_id: None,
            policy_blocked: None,
            policy_reason: None,
        });
        return if exit_status == 0 { 0 } else { exit_status };
    }

    let mut policy_blocked = false;
    let mut policy_reasons: Vec<String> = Vec::new();
    for c in commands {
        let root = repo_root()
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        match evaluate_command_safety(&c, &root) {
            SafetyDecision::Safe => {}
            SafetyDecision::Dangerous(reason) => {
                if !(force || allow_unsafe) {
                    policy_blocked = true;
                    policy_reasons.push(reason.clone());
                    eprintln!(
                        "WARN blocked dangerous command ({reason}); use CXFIX_FORCE=1 or --unsafe: {c}"
                    );
                    continue;
                }
                eprintln!("WARN unsafe override active; executing: {c}");
            }
        }
        println!("-> {c}");
        let status = Command::new("bash").args(["-lc", &c]).status();
        if let Err(e) = status {
            eprintln!("cxrs fix-run: failed to execute command: {e}");
        }
    }

    let policy_reason_joined = if policy_reasons.is_empty() {
        None
    } else {
        Some(policy_reasons.join("; "))
    };
    let _ = log_codex_run(RunLogInput {
        tool: "cxrs_fix_run",
        prompt: &task_input,
        duration_ms: result.duration_ms,
        usage: Some(&result.usage),
        capture: Some(&result.capture_stats),
        schema_ok: true,
        schema_reason: None,
        schema_name: Some(schema.name.as_str()),
        quarantine_id: None,
        policy_blocked: Some(policy_blocked),
        policy_reason: policy_reason_joined.as_deref(),
    });

    if exit_status == 0 { 0 } else { exit_status }
}

pub fn cmd_diffsum(staged: bool, execute_task: ExecuteTaskFn) -> i32 {
    let tool = if staged {
        "cxrs_diffsum_staged"
    } else {
        "cxrs_diffsum"
    };
    match generate_diffsum_value(tool, staged, execute_task) {
        Ok(v) => {
            print_diffsum_human(&v);
            0
        }
        Err(e) => {
            eprintln!(
                "cxrs {}: {e}",
                if staged { "diffsum-staged" } else { "diffsum" }
            );
            1
        }
    }
}

pub fn cmd_commitjson(execute_task: ExecuteTaskFn) -> i32 {
    match generate_commitjson_value(execute_task) {
        Ok(v) => match serde_json::to_string_pretty(&v) {
            Ok(s) => {
                println!("{s}");
                0
            }
            Err(e) => {
                eprintln!("cxrs commitjson: render failure: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("cxrs commitjson: {e}");
            1
        }
    }
}

pub fn cmd_commitmsg(execute_task: ExecuteTaskFn) -> i32 {
    let v = match generate_commitjson_value(execute_task) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs commitmsg: {e}");
            return 1;
        }
    };
    let subject = v
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let body_items: Vec<String> = v
        .get("body")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    let test_items: Vec<String> = v
        .get("tests")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    println!("{subject}");
    println!();
    if !body_items.is_empty() {
        for line in body_items {
            println!("- {line}");
        }
    }
    if !test_items.is_empty() {
        println!();
        println!("Tests:");
        for line in test_items {
            println!("- {line}");
        }
    }
    0
}

pub fn cmd_replay(id: &str, run_llm_jsonl: JsonlRunner) -> i32 {
    let rec = match read_quarantine_record(id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs replay: {e}");
            return 1;
        }
    };

    if rec.schema.trim().is_empty() || rec.prompt.trim().is_empty() {
        eprintln!("cxrs replay: quarantine entry is missing schema/prompt payload");
        return 1;
    }

    let full_prompt = build_strict_schema_prompt(&rec.schema, &rec.prompt);
    let jsonl = match run_llm_jsonl(&full_prompt) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs replay: {e}");
            return 1;
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
            Ok(qid) => eprintln!("cxrs replay: empty response; quarantine_id={qid}"),
            Err(e) => eprintln!("cxrs replay: failed to log schema failure: {e}"),
        }
        return 1;
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
            Ok(qid) => eprintln!("cxrs replay: invalid JSON; quarantine_id={qid}"),
            Err(e) => eprintln!("cxrs replay: failed to log schema failure: {e}"),
        }
        eprintln!("cxrs replay: raw response follows:");
        eprintln!("{raw}");
        return 1;
    }

    println!("{raw}");
    0
}

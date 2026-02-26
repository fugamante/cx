use serde_json::Value;

use crate::capture::run_system_command_capture;
use crate::error::{EXIT_OK, EXIT_RUNTIME, format_error};
use crate::schema::load_schema;
use crate::state::{read_state_value, value_at_path};
use crate::types::{ExecutionResult, LlmOutputKind, TaskInput, TaskSpec};

pub type ExecuteTaskFn = fn(TaskSpec) -> Result<ExecutionResult, String>;
pub use crate::structured_fixrun::cmd_fix_run;
pub use crate::structured_replay::cmd_replay;

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
            eprintln!("{}", format_error("next", &e));
            return EXIT_RUNTIME;
        }
    };

    let schema = match load_schema("next") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", format_error("next", &e));
            return EXIT_RUNTIME;
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
            eprintln!("{}", format_error("next", &e));
            return EXIT_RUNTIME;
        }
    };
    if result.schema_valid == Some(false) {
        if let Some(qid) = result.quarantine_id.as_deref() {
            eprintln!("cxrs next: schema failure; quarantine_id={qid}");
        }
        eprintln!("cxrs next: raw response follows:");
        eprintln!("{}", result.stdout);
        return EXIT_RUNTIME;
    }
    let schema_value: Value = match serde_json::from_str(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs next: invalid JSON after schema run: {e}");
            return EXIT_RUNTIME;
        }
    };
    let commands = match parse_commands_array(&schema_value.to_string()) {
        Ok(v) => v,
        Err(reason) => {
            eprintln!("cxrs next: {reason}");
            return EXIT_RUNTIME;
        }
    };
    for cmd in commands {
        println!("{cmd}");
    }
    EXIT_OK
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
            EXIT_OK
        }
        Err(e) => {
            eprintln!(
                "cxrs {}: {e}",
                if staged { "diffsum-staged" } else { "diffsum" }
            );
            EXIT_RUNTIME
        }
    }
}

pub fn cmd_commitjson(execute_task: ExecuteTaskFn) -> i32 {
    match generate_commitjson_value(execute_task) {
        Ok(v) => match serde_json::to_string_pretty(&v) {
            Ok(s) => {
                println!("{s}");
                EXIT_OK
            }
            Err(e) => {
                eprintln!(
                    "{}",
                    format_error("commitjson", &format!("render failure: {e}"))
                );
                EXIT_RUNTIME
            }
        },
        Err(e) => {
            eprintln!("{}", format_error("commitjson", &e));
            EXIT_RUNTIME
        }
    }
}

pub fn cmd_commitmsg(execute_task: ExecuteTaskFn) -> i32 {
    let v = match generate_commitjson_value(execute_task) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", format_error("commitmsg", &e));
            return EXIT_RUNTIME;
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
    EXIT_OK
}

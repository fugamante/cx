use jsonschema::JSONSchema;
use serde_json::{Value, json};
use std::fs;

use crate::capture::budget_config_from_env;
use crate::logs::validate_runs_jsonl_file;
use crate::paths::{repo_root, resolve_log_file, resolve_schema_dir};
use crate::schema::list_schemas;

pub fn cmd_schema(app_name: &str, args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    if sub != "list" {
        eprintln!("Usage: {app_name} schema list [--json]");
        return 2;
    }
    let as_json = args.iter().any(|a| a == "--json");
    let Some(dir) = resolve_schema_dir() else {
        eprintln!("cxrs schema: unable to resolve schema directory");
        return 1;
    };
    let schemas = match list_schemas() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs schema: {e}");
            return 1;
        }
    };

    if as_json {
        let rows: Vec<Value> = schemas
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "path": s.path.display().to_string(),
                    "id": s.id.clone().unwrap_or_default()
                })
            })
            .collect();
        println!(
            "{}",
            json!({
                "schema_dir": dir.display().to_string(),
                "file_count": rows.len(),
                "schemas": rows
            })
        );
        return 0;
    }

    println!("schema_dir: {}", dir.display());
    println!("file_count: {}", schemas.len());
    for s in schemas {
        let id = s.id.unwrap_or_else(|| "<no $id>".to_string());
        println!("- {} ({}) [{}]", s.name, s.path.display(), id);
    }
    0
}

pub fn cmd_ci(app_name: &str, args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("validate");
    if sub != "validate" {
        eprintln!("Usage: {app_name} ci validate [--strict] [--legacy-ok] [--json]");
        return 2;
    }
    let strict = args.iter().any(|a| a == "--strict");
    let legacy_ok = args.iter().any(|a| a == "--legacy-ok");
    let json_out = args.iter().any(|a| a == "--json");

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let Some(root) = repo_root() else {
        eprintln!("cxrs ci validate: not inside a git repository");
        return 2;
    };
    let schema_dir = root.join(".codex").join("schemas");
    if !schema_dir.is_dir() {
        errors.push(format!(
            "missing schema registry dir: {}",
            schema_dir.display()
        ));
    } else {
        let required = [
            "commitjson.schema.json",
            "diffsum.schema.json",
            "next.schema.json",
            "fixrun.schema.json",
        ];
        for name in required {
            let p = schema_dir.join(name);
            if !p.is_file() {
                errors.push(format!("missing schema: {}", p.display()));
            } else {
                match fs::read_to_string(&p)
                    .ok()
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                {
                    Some(v) => {
                        if let Err(e) = JSONSchema::compile(&v) {
                            errors.push(format!("schema failed to compile ({}): {e}", p.display()));
                        }
                    }
                    None => errors.push(format!("schema unreadable/invalid json: {}", p.display())),
                }
            }
        }
    }

    let log_file = resolve_log_file();
    if log_file.is_none() {
        errors.push("unable to resolve log file".to_string());
    }
    if let Some(log_file) = log_file {
        if log_file.exists() {
            match validate_runs_jsonl_file(&log_file, legacy_ok) {
                Ok(outcome) => {
                    if !outcome.issues.is_empty() {
                        if outcome.legacy_ok && outcome.invalid_json_lines == 0 {
                            warnings.extend(outcome.issues);
                        } else {
                            errors.extend(outcome.issues);
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        } else {
            warnings.push(format!("no log file at {}", log_file.display()));
        }
    }

    let budget = budget_config_from_env();
    if budget.budget_chars == 0 {
        errors.push("budget_chars must be > 0".to_string());
    }
    if budget.budget_lines == 0 {
        errors.push("budget_lines must be > 0".to_string());
    }
    if !matches!(budget.clip_mode.as_str(), "smart" | "head" | "tail") {
        warnings.push(format!(
            "clip_mode '{}' not recognized; expected smart|head|tail",
            budget.clip_mode
        ));
    }

    if strict {
        let qdir = root.join(".codex").join("quarantine");
        if qdir.exists() && !qdir.is_dir() {
            errors.push(format!(
                "quarantine path exists but is not a dir: {}",
                qdir.display()
            ));
        }
    }

    let ok = errors.is_empty();
    if json_out {
        let v = json!({
            "ok": ok,
            "strict": strict,
            "legacy_ok": legacy_ok,
            "repo_root": root.display().to_string(),
            "schema_dir": schema_dir.display().to_string(),
            "log_file": resolve_log_file().map(|p| p.display().to_string()),
            "budget_chars": budget.budget_chars,
            "budget_lines": budget.budget_lines,
            "clip_mode": budget.clip_mode,
            "warnings": warnings,
            "errors": errors
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string())
        );
        return if ok { 0 } else { 1 };
    }

    println!("== cxrs ci validate ==");
    println!("repo_root: {}", root.display());
    println!("schema_dir: {}", schema_dir.display());
    if let Some(p) = resolve_log_file() {
        println!("log_file: {}", p.display());
    } else {
        println!("log_file: <unresolved>");
    }
    println!(
        "budget: chars={} lines={} mode={}",
        budget.budget_chars, budget.budget_lines, budget.clip_mode
    );

    if !warnings.is_empty() {
        println!("warnings: {}", warnings.len());
        for w in warnings.iter().take(10) {
            println!("- {w}");
        }
        if warnings.len() > 10 {
            println!("- ... and {} more", warnings.len() - 10);
        }
    } else {
        println!("warnings: 0");
    }

    if !errors.is_empty() {
        println!("errors: {}", errors.len());
        for e in errors.iter().take(20) {
            println!("- {e}");
        }
        if errors.len() > 20 {
            println!("- ... and {} more", errors.len() - 20);
        }
        println!("status: fail");
        return 1;
    }
    println!("status: ok");
    0
}

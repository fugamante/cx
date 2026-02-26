use jsonschema::JSONSchema;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

use crate::capture::budget_config_from_env;
use crate::logs::validate_runs_jsonl_file;
use crate::paths::{repo_root, resolve_log_file, resolve_schema_dir};
use crate::schema::list_schemas;

pub fn cmd_schema(app_name: &str, args: &[String]) -> i32 {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    if sub != "list" {
        crate::cx_eprintln!("Usage: {app_name} schema list [--json]");
        return 2;
    }
    let as_json = args.iter().any(|a| a == "--json");
    let Some(dir) = resolve_schema_dir() else {
        crate::cx_eprintln!("cxrs schema: unable to resolve schema directory");
        return 1;
    };
    let schemas = match list_schemas() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("cxrs schema: {e}");
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

struct CiArgs {
    strict: bool,
    legacy_ok: bool,
    json_out: bool,
}

fn parse_ci_args(app_name: &str, args: &[String]) -> Result<CiArgs, i32> {
    let sub = args.first().map(String::as_str).unwrap_or("validate");
    if sub != "validate" {
        crate::cx_eprintln!("Usage: {app_name} ci validate [--strict] [--legacy-ok] [--json]");
        return Err(2);
    }
    Ok(CiArgs {
        strict: args.iter().any(|a| a == "--strict"),
        legacy_ok: args.iter().any(|a| a == "--legacy-ok"),
        json_out: args.iter().any(|a| a == "--json"),
    })
}

fn validate_schema_file(path: &Path, errors: &mut Vec<String>) {
    let parsed = fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());
    match parsed {
        Some(v) => {
            if let Err(e) = JSONSchema::compile(&v) {
                errors.push(format!(
                    "schema failed to compile ({}): {e}",
                    path.display()
                ));
            }
        }
        None => errors.push(format!(
            "schema unreadable/invalid json: {}",
            path.display()
        )),
    }
}

fn check_required_schemas(schema_dir: &Path, errors: &mut Vec<String>) {
    if !schema_dir.is_dir() {
        errors.push(format!(
            "missing schema registry dir: {}",
            schema_dir.display()
        ));
        return;
    }
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
            validate_schema_file(&p, errors);
        }
    }
}

fn validate_logs(
    legacy_ok: bool,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Option<PathBuf> {
    let log_file = resolve_log_file();
    if log_file.is_none() {
        errors.push("unable to resolve log file".to_string());
        return None;
    }
    let log_file = log_file.expect("checked is_some");
    if !log_file.exists() {
        warnings.push(format!("no log file at {}", log_file.display()));
        return Some(log_file);
    }
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
    Some(log_file)
}

fn validate_budget(
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> crate::capture::BudgetConfig {
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
    budget
}

struct CiJsonOutput<'a> {
    ok: bool,
    args: &'a CiArgs,
    root: &'a Path,
    schema_dir: &'a Path,
    log_file: Option<PathBuf>,
    budget: &'a crate::capture::BudgetConfig,
    warnings: Vec<String>,
    errors: Vec<String>,
}

fn print_ci_json(out: CiJsonOutput<'_>) -> i32 {
    let CiJsonOutput {
        ok,
        args,
        root,
        schema_dir,
        log_file,
        budget,
        warnings,
        errors,
    } = out;
    let v = json!({
        "ok": ok,
        "strict": args.strict,
        "legacy_ok": args.legacy_ok,
        "repo_root": root.display().to_string(),
        "schema_dir": schema_dir.display().to_string(),
        "log_file": log_file.map(|p| p.display().to_string()),
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
    if ok { 0 } else { 1 }
}

fn print_sample(label: &str, items: &[String], max: usize) {
    println!("{label}: {}", items.len());
    for item in items.iter().take(max) {
        println!("- {item}");
    }
    if items.len() > max {
        println!("- ... and {} more", items.len() - max);
    }
}

fn print_ci_text(
    ok: bool,
    root: &Path,
    schema_dir: &Path,
    budget: &crate::capture::BudgetConfig,
    warnings: &[String],
    errors: &[String],
) -> i32 {
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

    if warnings.is_empty() {
        println!("warnings: 0");
    } else {
        print_sample("warnings", warnings, 10);
    }

    if errors.is_empty() {
        println!("status: ok");
        return 0;
    }
    print_sample("errors", errors, 20);
    println!("status: fail");
    if ok { 0 } else { 1 }
}

pub fn cmd_ci(app_name: &str, args: &[String]) -> i32 {
    let parsed = match parse_ci_args(app_name, args) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let Some(root) = repo_root() else {
        crate::cx_eprintln!("cxrs ci validate: not inside a git repository");
        return 2;
    };

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let schema_dir = root.join(".codex").join("schemas");

    check_required_schemas(&schema_dir, &mut errors);
    let log_file = validate_logs(parsed.legacy_ok, &mut errors, &mut warnings);
    let budget = validate_budget(&mut errors, &mut warnings);

    if parsed.strict {
        let qdir = root.join(".codex").join("quarantine");
        if qdir.exists() && !qdir.is_dir() {
            errors.push(format!(
                "quarantine path exists but is not a dir: {}",
                qdir.display()
            ));
        }
    }

    let ok = errors.is_empty();
    if parsed.json_out {
        return print_ci_json(CiJsonOutput {
            ok,
            args: &parsed,
            root: &root,
            schema_dir: &schema_dir,
            log_file,
            budget: &budget,
            warnings,
            errors,
        });
    }
    print_ci_text(ok, &root, &schema_dir, &budget, &warnings, &errors)
}

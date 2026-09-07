use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::cli_app_name;
use crate::local_models::{
    AddLocalModelInput, add_record, find_record, list_records, record_json, registry_json,
    remove_record,
};

fn print_models_usage(app_name: &str) {
    crate::cx_eprintln!(
        "Usage: {app_name} llm models <list [--json]|add <alias> --backend <ollama|llamacpp|mlx> --model <model> [--replace] [--json]|inspect <alias-or-id> [--disk-usage] [--json]|remove <alias-or-id> [--json]>"
    );
}

fn has_json_flag(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json")
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn emit_json(value: &Value) -> i32 {
    match serde_json::to_string_pretty(value) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!(
                "{} llm models: failed to serialize JSON: {e}",
                cli_app_name()
            );
            1
        }
    }
}

fn option_value(args: &[String], flag: &str) -> Result<Option<String>, String> {
    let mut out = None;
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == flag {
            let Some(v) = args.get(i + 1) else {
                return Err(format!("{flag} requires a value"));
            };
            out = Some(v.clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    Ok(out)
}

fn parse_size_bytes(args: &[String]) -> Result<Option<u64>, String> {
    let Some(raw) = option_value(args, "--size-bytes")? else {
        return Ok(None);
    };
    raw.parse::<u64>()
        .map(Some)
        .map_err(|_| format!("--size-bytes expects an integer, got '{raw}'"))
}

#[derive(Debug)]
struct PathProbe {
    configured: Option<String>,
    status: &'static str,
    kind: &'static str,
    exists: bool,
    size_bytes: Option<u64>,
    error: Option<String>,
}

impl PathProbe {
    fn render_value(&self) -> String {
        self.configured
            .as_deref()
            .filter(|v| !v.is_empty())
            .unwrap_or("<unset>")
            .to_string()
    }

    fn json(&self) -> Value {
        json!({
            "configured": self.configured,
            "status": self.status,
            "path_kind": self.kind,
            "exists": self.exists,
            "size_bytes": self.size_bytes,
            "error": self.error
        })
    }
}

fn dir_size(root: &Path) -> Result<u64, String> {
    let mut total = 0u64;
    let mut stack = vec![PathBuf::from(root)];
    while let Some(path) = stack.pop() {
        let entries = fs::read_dir(&path)
            .map_err(|e| format!("cannot read directory '{}': {e}", path.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|e| format!("cannot read directory entry in '{}': {e}", path.display()))?;
            let entry_path = entry.path();
            let meta = fs::symlink_metadata(&entry_path)
                .map_err(|e| format!("cannot stat '{}': {e}", entry_path.display()))?;
            let file_type = meta.file_type();
            if file_type.is_file() {
                total = total.saturating_add(meta.len());
            } else if file_type.is_dir() {
                stack.push(entry_path);
            }
        }
    }
    Ok(total)
}

fn probe_path(configured: Option<&str>, disk_usage: bool) -> PathProbe {
    let configured = configured.map(str::trim).filter(|v| !v.is_empty());
    let Some(raw_path) = configured else {
        return PathProbe {
            configured: None,
            status: "unset",
            kind: "unset",
            exists: false,
            size_bytes: None,
            error: None,
        };
    };
    let path = Path::new(raw_path);
    let meta = match fs::symlink_metadata(path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return PathProbe {
                configured: Some(raw_path.to_string()),
                status: "missing",
                kind: "missing",
                exists: false,
                size_bytes: None,
                error: None,
            };
        }
        Err(e) => {
            return PathProbe {
                configured: Some(raw_path.to_string()),
                status: "error",
                kind: "error",
                exists: false,
                size_bytes: None,
                error: Some(e.to_string()),
            };
        }
    };
    let file_type = meta.file_type();
    if file_type.is_file() {
        return PathProbe {
            configured: Some(raw_path.to_string()),
            status: "ok",
            kind: "file",
            exists: true,
            size_bytes: Some(meta.len()),
            error: None,
        };
    }
    if file_type.is_dir() {
        let size_bytes = if disk_usage {
            match dir_size(path) {
                Ok(v) => Some(v),
                Err(e) => {
                    return PathProbe {
                        configured: Some(raw_path.to_string()),
                        status: "error",
                        kind: "dir",
                        exists: true,
                        size_bytes: None,
                        error: Some(e),
                    };
                }
            }
        } else {
            None
        };
        return PathProbe {
            configured: Some(raw_path.to_string()),
            status: "ok",
            kind: "dir",
            exists: true,
            size_bytes,
            error: None,
        };
    }
    let kind = if file_type.is_symlink() {
        "symlink"
    } else {
        "other"
    };
    PathProbe {
        configured: Some(raw_path.to_string()),
        status: "ok",
        kind,
        exists: true,
        size_bytes: None,
        error: None,
    }
}

fn resolved_model_status(local_path: &PathProbe) -> &'static str {
    match local_path.status {
        "ok" => "resolved_local_path",
        "missing" => "missing_local_path",
        "unset" => "unresolved_local_path",
        _ => "local_path_error",
    }
}

fn parse_model_add(
    alias: &str,
    backend: &str,
    resolved_model: &str,
    args: &[String],
) -> Result<AddLocalModelInput, String> {
    let trust_remote_code =
        option_value(args, "--trust-remote-code")?.unwrap_or_else(|| "unknown".to_string());
    Ok(AddLocalModelInput {
        id: option_value(args, "--id")?,
        alias: alias.to_string(),
        backend: backend.to_string(),
        provider: option_value(args, "--provider")?,
        repo_id: option_value(args, "--repo-id")?,
        revision: option_value(args, "--revision")?,
        resolved_model: resolved_model.to_string(),
        local_path: option_value(args, "--local-path")?,
        quantization: option_value(args, "--quantization")?,
        format: option_value(args, "--format")?,
        size_bytes: parse_size_bytes(args)?,
        cache_path: option_value(args, "--cache-path")?,
        preferred_args: option_value(args, "--preferred-args")?,
        trust_remote_code,
        replace: args.iter().any(|a| a == "--replace"),
    })
}

fn llm_models_list(args: &[String]) -> i32 {
    if has_json_flag(args) {
        return match registry_json() {
            Ok(v) => emit_json(&v),
            Err(e) => {
                crate::cx_eprintln!("{} llm models list: {e}", cli_app_name());
                1
            }
        };
    }
    match list_records() {
        Ok(records) if records.is_empty() => {
            println!("local_models: <empty>");
            0
        }
        Ok(records) => {
            println!("local_models:");
            for record in records {
                println!(
                    "- alias={} backend={} id={} model={}",
                    record.alias, record.backend, record.id, record.resolved_model
                );
            }
            0
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models list: {e}", cli_app_name());
            1
        }
    }
}

fn llm_models_add(app_name: &str, args: &[String]) -> i32 {
    let Some(alias) = args.first() else {
        print_models_usage(app_name);
        return 2;
    };
    let backend = match option_value(args, "--backend") {
        Ok(Some(v)) => v,
        Ok(None) => {
            print_models_usage(app_name);
            return 2;
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models add: {e}", cli_app_name());
            return 2;
        }
    };
    let resolved_model = match option_value(args, "--model") {
        Ok(Some(v)) => v,
        Ok(None) => {
            print_models_usage(app_name);
            return 2;
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models add: {e}", cli_app_name());
            return 2;
        }
    };
    let input = match parse_model_add(alias, &backend, &resolved_model, args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} llm models add: {e}", cli_app_name());
            return 2;
        }
    };
    match add_record(input) {
        Ok(record) => {
            if has_json_flag(args) {
                emit_json(&record_json(&record))
            } else {
                println!("ok");
                println!("model_alias: {}", record.alias);
                println!("model_backend: {}", record.backend);
                println!("model_id: {}", record.id);
                println!("resolved_model: {}", record.resolved_model);
                0
            }
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models add: {e}", cli_app_name());
            1
        }
    }
}

fn llm_models_inspect(app_name: &str, args: &[String]) -> i32 {
    let query = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map(String::as_str);
    let Some(query) = query else {
        print_models_usage(app_name);
        return 2;
    };
    let disk_usage = has_flag(args, "--disk-usage");
    match find_record(query) {
        Ok(Some(record)) => {
            let local_probe = probe_path(record.local_path.as_deref(), disk_usage);
            let cache_probe = probe_path(record.cache_path.as_deref(), disk_usage);
            let mode = if disk_usage { "disk_usage" } else { "cheap" };
            let model_state = resolved_model_status(&local_probe);
            if has_json_flag(args) {
                let mut payload = record_json(&record);
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert(
                        "inspect".to_string(),
                        json!({
                            "accounting_mode": mode,
                            "resolved_model_status": model_state,
                            "local_path": local_probe.json(),
                            "cache_path": cache_probe.json()
                        }),
                    );
                }
                emit_json(&payload)
            } else {
                println!("model_id: {}", record.id);
                println!("model_alias: {}", record.alias);
                println!("model_backend: {}", record.backend);
                println!("resolved_model: {}", record.resolved_model);
                println!("inspect_accounting_mode: {mode}");
                println!("resolved_model_status: {model_state}");
                println!("local_path: {}", local_probe.render_value());
                println!("local_path_status: {}", local_probe.status);
                println!("local_path_kind: {}", local_probe.kind);
                println!(
                    "local_path_exists: {}",
                    if local_probe.exists { "yes" } else { "no" }
                );
                println!(
                    "local_path_size_bytes: {}",
                    local_probe
                        .size_bytes
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "<unknown>".to_string())
                );
                if let Some(err) = local_probe.error.as_deref() {
                    println!("local_path_error: {err}");
                }
                println!("cache_path: {}", cache_probe.render_value());
                println!("cache_path_status: {}", cache_probe.status);
                println!("cache_path_kind: {}", cache_probe.kind);
                println!(
                    "cache_path_exists: {}",
                    if cache_probe.exists { "yes" } else { "no" }
                );
                println!(
                    "cache_path_size_bytes: {}",
                    cache_probe
                        .size_bytes
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "<unknown>".to_string())
                );
                if let Some(err) = cache_probe.error.as_deref() {
                    println!("cache_path_error: {err}");
                }
                println!(
                    "last_used_at: {}",
                    record.last_used_at.as_deref().unwrap_or("<unknown>")
                );
                println!(
                    "last_smoke_status: {}",
                    record.last_smoke_status.as_deref().unwrap_or("<unknown>")
                );
                println!(
                    "preferred_args: {}",
                    record.preferred_args.as_deref().unwrap_or("<unset>")
                );
                println!("trust_remote_code: {}", record.trust_remote_code);
                0
            }
        }
        Ok(None) => {
            crate::cx_eprintln!(
                "{} llm models inspect: local model not found: {}",
                cli_app_name(),
                query
            );
            1
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models inspect: {e}", cli_app_name());
            1
        }
    }
}

fn llm_models_remove(app_name: &str, args: &[String]) -> i32 {
    let Some(query) = args.first() else {
        print_models_usage(app_name);
        return 2;
    };
    match remove_record(query) {
        Ok(Some(record)) => {
            if has_json_flag(args) {
                emit_json(&record_json(&record))
            } else {
                println!("ok");
                println!("removed_model_alias: {}", record.alias);
                println!("removed_model_id: {}", record.id);
                0
            }
        }
        Ok(None) => {
            crate::cx_eprintln!(
                "{} llm models remove: local model not found: {}",
                cli_app_name(),
                query
            );
            1
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models remove: {e}", cli_app_name());
            1
        }
    }
}

pub(super) fn dispatch(app_name: &str, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("list") {
        "list" => llm_models_list(&args[1..]),
        "add" => llm_models_add(app_name, &args[1..]),
        "inspect" => llm_models_inspect(app_name, &args[1..]),
        "remove" | "rm" => llm_models_remove(app_name, &args[1..]),
        _ => {
            print_models_usage(app_name);
            2
        }
    }
}

use std::process::Command;

use serde_json::json;

use crate::error::{EXIT_OK, EXIT_RUNTIME, EXIT_USAGE};

#[derive(Debug)]
struct LaunchArgs {
    json: bool,
    no_open: bool,
    local: bool,
    cxops_bin: String,
}

fn usage(app_name: &str) {
    crate::cx_eprintln!(
        "Usage: {app_name} launch [--json] [--no-open] [--local|--remote] [--cxops-bin PATH]"
    );
}

fn parse_args(args: &[String]) -> Result<LaunchArgs, String> {
    let mut out = LaunchArgs {
        json: false,
        no_open: false,
        local: true,
        cxops_bin: std::env::var("CXOPS_BIN").unwrap_or_else(|_| default_cxops_bin()),
    };
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                out.json = true;
                i += 1;
            }
            "--no-open" => {
                out.no_open = true;
                i += 1;
            }
            "--local" => {
                out.local = true;
                i += 1;
            }
            "--remote" => {
                out.local = false;
                i += 1;
            }
            "--cxops-bin" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--cxops-bin requires a value".to_string());
                };
                out.cxops_bin = value.clone();
                i += 2;
            }
            other => return Err(format!("unknown launch argument '{other}'")),
        }
    }
    Ok(out)
}

fn default_cxops_bin() -> String {
    if let Ok(home) = std::env::var("HOME") {
        let cargo_bin = std::path::Path::new(&home).join(".cargo/bin/cxops");
        if cargo_bin.is_file() {
            return cargo_bin.display().to_string();
        }
    }
    "cxops".to_string()
}

fn run_cxops(cxops_bin: &str, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(cxops_bin)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run {cxops_bin} {}: {e}", args.join(" ")))
}

fn print_missing_cxops(app_name: &str, cxops_bin: &str, err: &str, json_out: bool) -> i32 {
    if json_out {
        println!(
            "{}",
            json!({
                "status": "error",
                "reason": "cxops_unavailable",
                "cxops_bin": cxops_bin,
                "error": err,
                "install_hint": "Install the optional cxops companion separately, then pass --cxops-bin PATH if it is not on PATH."
            })
        );
    } else {
        crate::cx_eprintln!("{app_name} launch: {err}");
        crate::cx_eprintln!(
            "{app_name} launch: install the optional cxops companion separately, then pass --cxops-bin PATH if it is not on PATH"
        );
    }
    EXIT_RUNTIME
}

pub fn cmd_launch(app_name: &str, args: &[String]) -> i32 {
    let cfg = match parse_args(args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{app_name} launch: {e}");
            usage(app_name);
            return EXIT_USAGE;
        }
    };

    let bringup = match run_cxops(&cfg.cxops_bin, &["bringup"]) {
        Ok(v) => v,
        Err(e) => return print_missing_cxops(app_name, &cfg.cxops_bin, &e, cfg.json),
    };
    if !bringup.status.success() {
        if cfg.json {
            println!(
                "{}",
                json!({
                    "status": "error",
                    "reason": "bringup_failed",
                    "cxops_bin": cfg.cxops_bin,
                    "bringup_exit": bringup.status.code(),
                    "bringup_stdout": String::from_utf8_lossy(&bringup.stdout),
                    "bringup_stderr": String::from_utf8_lossy(&bringup.stderr),
                })
            );
        } else {
            crate::cx_eprintln!("{app_name} launch: cxops bringup failed");
            crate::cx_eprintln!("{}", String::from_utf8_lossy(&bringup.stderr));
        }
        return bringup.status.code().unwrap_or(EXIT_RUNTIME);
    }

    let mut opened = false;
    let mut ui_exit = None;
    let mut ui_stdout = String::new();
    let mut ui_stderr = String::new();
    if !cfg.no_open {
        let ui_args: &[&str] = if cfg.local {
            &["ui", "--local"]
        } else {
            &["ui"]
        };
        let ui = match run_cxops(&cfg.cxops_bin, ui_args) {
            Ok(v) => v,
            Err(e) => return print_missing_cxops(app_name, &cfg.cxops_bin, &e, cfg.json),
        };
        opened = ui.status.success();
        ui_exit = ui.status.code();
        ui_stdout = String::from_utf8_lossy(&ui.stdout).to_string();
        ui_stderr = String::from_utf8_lossy(&ui.stderr).to_string();
        if !ui.status.success() {
            if cfg.json {
                println!(
                    "{}",
                    json!({
                        "status": "error",
                        "reason": "ui_open_failed",
                        "cxops_bin": cfg.cxops_bin,
                        "bringup_exit": bringup.status.code(),
                        "ui_exit": ui_exit,
                        "ui_stdout": ui_stdout,
                        "ui_stderr": ui_stderr,
                    })
                );
            } else {
                crate::cx_eprintln!("{app_name} launch: cxops ui failed");
                crate::cx_eprintln!("{ui_stderr}");
            }
            return ui.status.code().unwrap_or(EXIT_RUNTIME);
        }
    }

    if cfg.json {
        println!(
            "{}",
            json!({
                "status": "ok",
                "cxops_bin": cfg.cxops_bin,
                "local": cfg.local,
                "opened": opened,
                "bringup_exit": bringup.status.code(),
                "ui_exit": ui_exit,
                "bringup_stdout": String::from_utf8_lossy(&bringup.stdout),
                "bringup_stderr": String::from_utf8_lossy(&bringup.stderr),
                "ui_stdout": ui_stdout,
                "ui_stderr": ui_stderr,
            })
        );
    } else if cfg.no_open {
        println!("{app_name} launch: server is running; open the cxops UI when ready.");
    } else {
        println!("{app_name} launch: server is running and UI open was requested.");
    }
    EXIT_OK
}

use std::env;
use std::path::Path;

const APP_NAME: &str = "cxrs";
const APP_DESC: &str = "Rust spike for the cx toolchain";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("{APP_NAME} - {APP_DESC}");
    println!();
    println!("Usage:");
    println!("  {APP_NAME} <command>");
    println!();
    println!("Commands:");
    println!("  version     Print tool version");
    println!("  doctor      Run non-interactive environment checks");
    println!("  help        Print this help");
}

fn print_version() {
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    println!("name: {APP_NAME}");
    println!("version: {APP_VERSION}");
    println!("cwd: {cwd}");
}

fn bin_in_path(bin: &str) -> bool {
    let path = match env::var_os("PATH") {
        Some(v) => v,
        None => return false,
    };
    env::split_paths(&path).any(|dir| {
        let candidate = dir.join(bin);
        Path::new(&candidate).is_file()
    })
}

fn print_doctor() -> i32 {
    let required = ["git", "jq", "codex"];
    let optional = ["rtk"];
    let mut missing_required = 0;

    println!("== cxrs doctor ==");
    for bin in required {
        if bin_in_path(bin) {
            println!("OK: {bin}");
        } else {
            println!("MISSING: {bin}");
            missing_required += 1;
        }
    }
    for bin in optional {
        if bin_in_path(bin) {
            println!("OK: {bin} (optional)");
        } else {
            println!("WARN: {bin} not found (optional)");
        }
    }
    if missing_required == 0 {
        println!("PASS: environment is ready for cxrs spike development.");
        0
    } else {
        println!("FAIL: install required binaries before using cxrs.");
        1
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    let code = match cmd {
        "help" | "-h" | "--help" => {
            print_help();
            0
        }
        "version" | "-V" | "--version" => {
            print_version();
            0
        }
        "doctor" => print_doctor(),
        _ => {
            eprintln!("{APP_NAME}: unknown command '{cmd}'");
            eprintln!("Run '{APP_NAME} help' for usage.");
            2
        }
    };
    std::process::exit(code);
}

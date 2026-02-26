use std::env;
use std::path::{Path, PathBuf};

use crate::paths::repo_root;

#[derive(Debug, Clone)]
pub enum SafetyDecision {
    Safe,
    Dangerous(String),
}

fn normalize_token(tok: &str) -> String {
    tok.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == ';' || c == ',')
        .to_string()
}

fn command_has_write_pattern(lower: &str) -> bool {
    lower.contains(">>")
        || lower.contains(">")
        || lower.contains("tee ")
        || lower.contains("touch ")
        || lower.contains("mkdir ")
        || lower.contains("cp ")
        || lower.contains("mv ")
        || lower.contains("install ")
        || lower.contains("dd ")
        || lower.contains("chmod ")
        || lower.contains("chown ")
}

fn write_targets_outside_repo(cmd: &str, repo_root: &Path) -> bool {
    let root_s = repo_root.to_string_lossy().to_string();
    let tokens: Vec<String> = cmd.split_whitespace().map(normalize_token).collect();
    let mut candidates: Vec<String> = Vec::new();
    let last = tokens.last().cloned().unwrap_or_default();
    for i in 0..tokens.len() {
        let t = tokens[i].as_str();
        if t == ">" || t == ">>" || t == "tee" {
            if let Some(next) = tokens.get(i + 1) {
                candidates.push(next.clone());
            }
        }
        if t == "touch" || t == "mkdir" || t == "chmod" || t == "chown" {
            if let Some(next) = tokens.get(i + 1) {
                candidates.push(next.clone());
            }
        }
        if let Some(path) = t.strip_prefix("of=") {
            candidates.push(path.to_string());
        }
        if t.starts_with('/') {
            candidates.push(t.to_string());
        }
        if t.starts_with("~/") || t == "~" || t.starts_with("$HOME") || t.starts_with("${HOME}") {
            candidates.push(t.to_string());
        }
    }
    // For cp/mv/install, treat the last argument as destination.
    if tokens.iter().any(|t| t == "cp" || t == "mv" || t == "install") && !last.is_empty() {
        candidates.push(last);
    }
    candidates.into_iter().any(|p| {
        let p = p.trim().to_string();
        if p.is_empty() {
            return false;
        }
        // Any parent traversal in a write target is treated as unsafe (can escape repo root).
        if p.contains("..") {
            return true;
        }
        if p.starts_with("~/") || p == "~" || p.starts_with("$HOME") || p.starts_with("${HOME}") {
            return true;
        }
        if !p.starts_with('/') {
            return false;
        }
        if p.starts_with(&(root_s.clone() + "/")) || p == root_s {
            return false;
        }
        true
    })
}

pub fn evaluate_command_safety(cmd: &str, repo_root: &Path) -> SafetyDecision {
    let compact = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = compact.to_lowercase();

    if lower.contains(" sudo ") || lower.starts_with("sudo ") || lower.ends_with(" sudo") {
        return SafetyDecision::Dangerous("contains sudo".to_string());
    }
    if lower.contains("rm -rf")
        || lower.contains("rm -fr")
        || lower.contains("rm -r -f")
        || lower.contains("rm -f -r")
    {
        return SafetyDecision::Dangerous("contains rm -rf pattern".to_string());
    }
    if lower.contains("curl ")
        && lower.contains('|')
        && (lower.contains("| bash") || lower.contains("| sh") || lower.contains("| zsh"))
    {
        return SafetyDecision::Dangerous("contains curl pipe shell pattern".to_string());
    }
    if (lower.contains("chmod ") || lower.contains("chown "))
        && (lower.contains("/system") || lower.contains("/library") || lower.contains("/usr"))
        && !lower.contains("/usr/local")
    {
        return SafetyDecision::Dangerous("chmod/chown on protected system path".to_string());
    }
    if (lower.contains("chmod ") || lower.contains("chown "))
        && command_has_write_pattern(&lower)
        && write_targets_outside_repo(&compact, repo_root)
    {
        return SafetyDecision::Dangerous("chmod/chown target outside repo root".to_string());
    }
    if (lower.contains("> /system")
        || lower.contains(">> /system")
        || lower.contains("> /library")
        || lower.contains(">> /library")
        || lower.contains("> /usr")
        || lower.contains(">> /usr")
        || (lower.contains("tee ")
            && (lower.contains(" /system")
                || lower.contains(" /library")
                || lower.contains(" /usr"))))
        && !lower.contains("/usr/local")
    {
        return SafetyDecision::Dangerous("write redirection to protected system path".to_string());
    }
    if command_has_write_pattern(&lower) && write_targets_outside_repo(&compact, repo_root) {
        return SafetyDecision::Dangerous("write target outside repo root".to_string());
    }
    SafetyDecision::Safe
}

pub fn cmd_policy(args: &[String], app_name: &str) -> i32 {
    if args.first().map(String::as_str) == Some("check") {
        if args.len() < 2 {
            eprintln!("Usage: {app_name} policy check <command...>");
            return 2;
        }
        let candidate = args[1..].join(" ");
        let root = repo_root()
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        match evaluate_command_safety(&candidate, &root) {
            SafetyDecision::Safe => println!("safe"),
            SafetyDecision::Dangerous(reason) => println!("dangerous: {reason}"),
        }
        return 0;
    }

    if args.first().map(String::as_str) == Some("show") || args.is_empty() {
        let unsafe_flag = env::var("CX_UNSAFE").ok().as_deref() == Some("1");
        let force = env::var("CXFIX_FORCE").ok().as_deref() == Some("1");
        println!("== cxrs policy show ==");
        println!("Active safety rules:");
        println!("- Block: sudo");
        println!("- Block: rm -rf family");
        println!("- Block: curl | bash/sh/zsh");
        println!("- Block: chmod/chown on /System,/Library,/usr (except /usr/local)");
        println!("- Block: write operations outside repo root");
        println!();
        println!("Unsafe override state:");
        println!(
            "--unsafe / CX_UNSAFE=1: {}",
            if unsafe_flag { "on" } else { "off" }
        );
        println!("CXFIX_FORCE=1: {}", if force { "on" } else { "off" });
        return 0;
    }

    println!("== cxrs policy ==");
    println!("Dangerous command patterns blocked by default in fix-run:");
    println!("- sudo (any)");
    println!("- rm -rf / rm -fr forms");
    println!("- curl | bash/sh/zsh");
    println!("- chmod/chown on /System, /Library, /usr (except /usr/local)");
    println!("- shell redirection/tee writes to /System, /Library, /usr (except /usr/local)");
    println!();
    println!("Overrides:");
    println!("- --unsafe          allow dangerous execution for current command");
    println!("- CXFIX_RUN=1       execute suggested commands");
    println!("- CXFIX_FORCE=1     allow dangerous commands");
    println!();
    println!("Examples:");
    println!("- {app_name} policy check \"sudo rm -rf /tmp/foo\"");
    println!("- {app_name} policy check \"chmod 755 /usr/local/bin/tool\"");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_rm_rf() {
        let root = Path::new("/tmp/repo");
        let decision = evaluate_command_safety("rm -rf ./target", root);
        assert!(matches!(decision, SafetyDecision::Dangerous(_)));
    }

    #[test]
    fn allows_write_inside_repo() {
        let root = Path::new("/tmp/repo");
        let decision = evaluate_command_safety("echo hi > /tmp/repo/out.txt", root);
        assert!(matches!(decision, SafetyDecision::Safe));
    }

    #[test]
    fn blocks_write_outside_repo() {
        let root = Path::new("/tmp/repo");
        let decision = evaluate_command_safety("echo hi > /etc/out.txt", root);
        assert!(matches!(decision, SafetyDecision::Dangerous(_)));
    }
}

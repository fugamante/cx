use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::app_config;
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

fn collect_write_candidates(cmd: &str) -> Vec<String> {
    let tokens: Vec<String> = cmd.split_whitespace().map(normalize_token).collect();
    let mut candidates: Vec<String> = Vec::new();
    let last = tokens.last().cloned().unwrap_or_default();

    for i in 0..tokens.len() {
        let t = tokens[i].as_str();
        if (t == ">" || t == ">>" || t == "tee")
            && let Some(next) = tokens.get(i + 1)
        {
            candidates.push(next.clone());
        }
        if (t == "touch" || t == "mkdir" || t == "chmod" || t == "chown")
            && let Some(next) = tokens.get(i + 1)
        {
            candidates.push(next.clone());
        }
        if let Some(path) = t.strip_prefix("of=") {
            candidates.push(path.to_string());
        }
        if t.starts_with('/') || t.starts_with("~/") || t == "~" {
            candidates.push(t.to_string());
        }
        if t.starts_with("$HOME") || t.starts_with("${HOME}") {
            candidates.push(t.to_string());
        }
    }

    if tokens
        .iter()
        .any(|t| t == "cp" || t == "mv" || t == "install")
        && !last.is_empty()
    {
        candidates.push(last);
    }
    candidates
}

fn path_is_outside_repo(p: &str, repo_root: &Path) -> bool {
    let path = p.trim();
    if path.is_empty() {
        return false;
    }
    if path.contains("..") || path == "~" {
        return true;
    }

    let root_abs = canonical_or_owned(repo_root);
    let candidate = resolve_candidate_path(path, repo_root);
    let Some(candidate) = candidate else {
        return true;
    };
    if candidate.exists() {
        let canon = canonical_or_owned(&candidate);
        return !canon_starts_with(&canon, &root_abs);
    }
    if let Some(parent) = candidate.parent()
        && parent.exists()
    {
        let parent_canon = canonical_or_owned(parent);
        if !canon_starts_with(&parent_canon, &root_abs) {
            return true;
        }
    }
    !lexically_inside_root(&candidate, repo_root)
}

fn write_targets_outside_repo(cmd: &str, repo_root: &Path) -> bool {
    collect_write_candidates(cmd)
        .into_iter()
        .any(|p| path_is_outside_repo(&p, repo_root))
}

fn canonical_or_owned(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn canon_starts_with(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn resolve_candidate_path(path: &str, repo_root: &Path) -> Option<PathBuf> {
    if let Some(home) = env::var_os("HOME") {
        if path == "~" {
            return Some(PathBuf::from(home));
        }
        if let Some(rest) = path.strip_prefix("~/") {
            return Some(PathBuf::from(home).join(rest));
        }
        if let Some(rest) = path.strip_prefix("$HOME/") {
            return Some(PathBuf::from(home).join(rest));
        }
        if let Some(rest) = path.strip_prefix("${HOME}/") {
            return Some(PathBuf::from(home).join(rest));
        }
    }
    if path.starts_with('$') {
        return None;
    }
    if path.starts_with('/') {
        return Some(PathBuf::from(path));
    }
    Some(repo_root.join(path))
}

fn lexically_inside_root(candidate: &Path, repo_root: &Path) -> bool {
    let root_s = repo_root.to_string_lossy().to_string();
    let cand = candidate.to_string_lossy().to_string();
    cand == root_s || cand.starts_with(&(root_s + "/"))
}

fn matches_sudo(lower: &str) -> bool {
    lower.contains(" sudo ") || lower.starts_with("sudo ") || lower.ends_with(" sudo")
}

fn matches_rm_rf(lower: &str) -> bool {
    lower.contains("rm -rf")
        || lower.contains("rm -fr")
        || lower.contains("rm -r -f")
        || lower.contains("rm -f -r")
}

fn matches_curl_pipe_shell(lower: &str) -> bool {
    lower.contains("curl ")
        && lower.contains('|')
        && (lower.contains("| bash") || lower.contains("| sh") || lower.contains("| zsh"))
}

fn matches_protected_chmod_chown(lower: &str) -> bool {
    (lower.contains("chmod ") || lower.contains("chown "))
        && (lower.contains("/system") || lower.contains("/library") || lower.contains("/usr"))
        && !lower.contains("/usr/local")
}

fn matches_protected_redirect(lower: &str) -> bool {
    let writes_protected = lower.contains("> /system")
        || lower.contains(">> /system")
        || lower.contains("> /library")
        || lower.contains(">> /library")
        || lower.contains("> /usr")
        || lower.contains(">> /usr")
        || (lower.contains("tee ")
            && (lower.contains(" /system")
                || lower.contains(" /library")
                || lower.contains(" /usr")));
    writes_protected && !lower.contains("/usr/local")
}

pub fn evaluate_command_safety(cmd: &str, repo_root: &Path) -> SafetyDecision {
    let compact = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = compact.to_lowercase();

    if matches_sudo(&lower) {
        return SafetyDecision::Dangerous("contains sudo".to_string());
    }
    if matches_rm_rf(&lower) {
        return SafetyDecision::Dangerous("contains rm -rf pattern".to_string());
    }
    if matches_curl_pipe_shell(&lower) {
        return SafetyDecision::Dangerous("contains curl pipe shell pattern".to_string());
    }
    if matches_protected_chmod_chown(&lower) {
        return SafetyDecision::Dangerous("chmod/chown on protected system path".to_string());
    }
    if matches_protected_redirect(&lower) {
        return SafetyDecision::Dangerous("write redirection to protected system path".to_string());
    }
    if command_has_write_pattern(&lower) && write_targets_outside_repo(&compact, repo_root) {
        return SafetyDecision::Dangerous("write target outside repo root".to_string());
    }
    SafetyDecision::Safe
}

fn handle_policy_check(args: &[String], app_name: &str) -> i32 {
    if args.len() < 2 {
        crate::cx_eprintln!("Usage: {app_name} policy check <command...>");
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
    0
}

fn print_policy_show() {
    let cfg = app_config();
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
        if cfg.cx_unsafe { "on" } else { "off" }
    );
    println!(
        "CXFIX_FORCE=1: {}",
        if cfg.cxfix_force { "on" } else { "off" }
    );
}

fn print_policy_help(app_name: &str) {
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
}

pub fn cmd_policy(args: &[String], app_name: &str) -> i32 {
    match args.first().map(String::as_str) {
        Some("check") => handle_policy_check(args, app_name),
        Some("show") | None => {
            print_policy_show();
            0
        }
        _ => {
            print_policy_help(app_name);
            0
        }
    }
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

    #[test]
    fn blocks_chmod_usr_and_allows_usr_local_rule_only() {
        let root = Path::new("/tmp/repo");
        let blocked = evaluate_command_safety("chmod 755 /usr/bin/tool", root);
        assert!(matches!(blocked, SafetyDecision::Dangerous(_)));
        let not_protected_rule = evaluate_command_safety("chmod 755 /usr/local/bin/tool", root);
        assert!(matches!(not_protected_rule, SafetyDecision::Dangerous(_)));
    }

    #[test]
    fn allows_write_to_repo_root_path() {
        let root = Path::new("/tmp/repo");
        let decision = evaluate_command_safety("touch /tmp/repo/output.txt", root);
        assert!(matches!(decision, SafetyDecision::Safe));
    }

    #[cfg(unix)]
    #[test]
    fn blocks_symlink_escape_write_target() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!(
            "cx-policy-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let repo = base.join("repo");
        let outside = base.join("outside");
        let _ = fs::create_dir_all(&repo);
        let _ = fs::create_dir_all(&outside);
        let link = repo.join("link");
        let _ = symlink(&outside, &link);
        let cmd = format!("echo hi > {}/escape.txt", link.display());
        let decision = evaluate_command_safety(&cmd, &repo);
        let _ = fs::remove_dir_all(&base);
        assert!(matches!(decision, SafetyDecision::Dangerous(_)));
    }
}

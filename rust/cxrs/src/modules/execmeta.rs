use chrono::Utc;
use std::fs;
use std::process::Command;

use crate::paths::repo_root_hint;
use crate::process::run_command_output_with_timeout;

pub fn prompt_preview(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

pub fn toolchain_version_string(app_version: &str) -> String {
    let mut base = app_version.to_string();
    if let Some(root) = repo_root_hint() {
        let version_file = root.join("VERSION");
        if let Ok(text) = fs::read_to_string(&version_file) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                base = trimmed.to_string();
            }
        }
        let mut git_cmd = Command::new("git");
        git_cmd
            .arg("-C")
            .arg(&root)
            .args(["rev-parse", "--short", "HEAD"]);
        if let Ok(out) = run_command_output_with_timeout(git_cmd, "git rev-parse --short HEAD")
            && out.status.success()
        {
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !sha.is_empty() {
                return format!("{base}+{sha}");
            }
        }
    }
    base
}

pub fn make_execution_id(tool: &str) -> String {
    format!(
        "{}_{}_{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        tool.replace(
            |c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-',
            "_"
        ),
        std::process::id()
    )
}

pub fn utc_now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn is_schema_tool(tool: &str) -> bool {
    matches!(
        tool,
        "cxcommitjson"
            | "cxcommitmsg"
            | "cxdiffsum"
            | "cxdiffsum_staged"
            | "cxnext"
            | "cxfix_run"
            | "cxrs_commitjson"
            | "cxrs_diffsum"
            | "cxrs_diffsum_staged"
            | "cxrs_next"
            | "cxrs_fix_run"
            | "commitjson"
            | "commitmsg"
            | "diffsum"
            | "diffsum-staged"
            | "next"
            | "fix-run"
    )
}

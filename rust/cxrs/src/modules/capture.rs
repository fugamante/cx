use std::env;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::types::CaptureStats;

static RTK_WARNED_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub struct BudgetConfig {
    pub budget_chars: usize,
    pub budget_lines: usize,
    pub clip_mode: String,
    pub clip_footer: bool,
}

fn parse_semver_triplet(raw: &str) -> Option<(u64, u64, u64)> {
    let candidate = raw
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find(|s| s.chars().filter(|c| *c == '.').count() >= 1 && !s.is_empty())?;
    let mut it = candidate.split('.');
    let major = it.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let minor = it.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let patch = it.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

fn semver_cmp(a: (u64, u64, u64), b: (u64, u64, u64)) -> i8 {
    if a.0 != b.0 {
        return if a.0 > b.0 { 1 } else { -1 };
    }
    if a.1 != b.1 {
        return if a.1 > b.1 { 1 } else { -1 };
    }
    if a.2 != b.2 {
        return if a.2 > b.2 { 1 } else { -1 };
    }
    0
}

pub fn rtk_version_raw() -> Option<String> {
    let out = Command::new("rtk").arg("--version").output().ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    if s.trim().is_empty() {
        s = String::from_utf8_lossy(&out.stderr).to_string();
    }
    let t = s.trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

pub fn rtk_is_usable() -> bool {
    if Command::new("rtk")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        return false;
    }
    let min_v = env::var("CX_RTK_MIN_VERSION").unwrap_or_else(|_| "0.22.1".to_string());
    let max_v = env::var("CX_RTK_MAX_VERSION").unwrap_or_default();
    let ver_raw = rtk_version_raw().unwrap_or_default();
    let Some(cur) = parse_semver_triplet(&ver_raw) else {
        if !RTK_WARNED_UNSUPPORTED.swap(true, Ordering::SeqCst) {
            eprintln!("cxrs: unable to parse rtk version; falling back to raw command output.");
        }
        return false;
    };
    let min = parse_semver_triplet(&min_v).unwrap_or((0, 0, 0));
    if semver_cmp(cur, min) < 0 {
        if !RTK_WARNED_UNSUPPORTED.swap(true, Ordering::SeqCst) {
            eprintln!(
                "cxrs: rtk version '{}' is below supported minimum '{}'; falling back to raw command output.",
                ver_raw, min_v
            );
        }
        return false;
    }
    if !max_v.is_empty() {
        let max = parse_semver_triplet(&max_v).unwrap_or((u64::MAX, u64::MAX, u64::MAX));
        if semver_cmp(cur, max) > 0 {
            if !RTK_WARNED_UNSUPPORTED.swap(true, Ordering::SeqCst) {
                eprintln!(
                    "cxrs: rtk version '{}' is above supported maximum '{}'; falling back to raw command output.",
                    ver_raw, max_v
                );
            }
            return false;
        }
    }
    true
}

pub fn budget_config_from_env() -> BudgetConfig {
    BudgetConfig {
        budget_chars: env::var("CX_CONTEXT_BUDGET_CHARS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(12000),
        budget_lines: env::var("CX_CONTEXT_BUDGET_LINES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(300),
        clip_mode: env::var("CX_CONTEXT_CLIP_MODE").unwrap_or_else(|_| "smart".to_string()),
        clip_footer: env::var("CX_CONTEXT_CLIP_FOOTER")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(1)
            == 1,
    }
}

fn is_rtk_supported_prefix(cmd0: &str) -> bool {
    matches!(
        cmd0,
        "git" | "diff" | "ls" | "tree" | "grep" | "test" | "log" | "read"
    )
}

fn normalize_generic(input: &str) -> String {
    let mut out = String::new();
    let mut blank_seen = false;
    for mut line in input.lines().map(|l| l.to_string()) {
        if line.trim().is_empty() {
            if !blank_seen {
                out.push('\n');
            }
            blank_seen = true;
            continue;
        }
        blank_seen = false;
        if line.chars().count() > 600 {
            line = format!("{}...", line.chars().take(600).collect::<String>());
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn reduce_git_status(input: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in input.lines() {
        let t = line.trim_start();
        if line.starts_with("On branch ")
            || line.starts_with("HEAD detached")
            || line.starts_with("Your branch ")
            || line.starts_with("Changes to be committed:")
            || line.starts_with("Changes not staged for commit:")
            || line.starts_with("Untracked files:")
            || line.starts_with("nothing to commit")
            || line.starts_with("no changes added to commit")
            || t.starts_with("modified:")
            || t.starts_with("new file:")
            || t.starts_with("deleted:")
            || t.starts_with("renamed:")
            || t.starts_with("both modified:")
            || t.starts_with("both added:")
            || t.starts_with("both deleted:")
        {
            out.push(line.to_string());
        }
    }
    if out.is_empty() {
        input
            .lines()
            .take(120)
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        out.join("\n")
    }
}

fn reduce_diff_like(input: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut changed = 0usize;
    for line in input.lines() {
        if line.starts_with("diff --git ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("@@ ")
            || line.starts_with("Binary files ")
            || line.starts_with("rename from ")
            || line.starts_with("rename to ")
        {
            out.push(line.to_string());
        } else if (line.starts_with('+') || line.starts_with('-')) && changed < 300 {
            out.push(line.to_string());
            changed += 1;
        }
    }
    if out.is_empty() {
        input.to_string()
    } else {
        out.join("\n")
    }
}

fn native_reduce_output(cmd: &[String], input: &str) -> String {
    let cmd0 = cmd.first().map(String::as_str).unwrap_or("");
    let cmd1 = cmd.get(1).map(String::as_str).unwrap_or("");
    let reduced = match (cmd0, cmd1) {
        ("git", "status") => reduce_git_status(input),
        ("git", "diff") | ("diff", _) => reduce_diff_like(input),
        _ => input.to_string(),
    };
    normalize_generic(&reduced)
}

pub(crate) fn choose_clip_mode(input: &str, configured_mode: &str) -> String {
    match configured_mode {
        "head" => "head".to_string(),
        "tail" => "tail".to_string(),
        _ => {
            let lower = input.to_lowercase();
            if lower.contains("error") || lower.contains("fail") || lower.contains("warning") {
                "tail".to_string()
            } else {
                "head".to_string()
            }
        }
    }
}

fn first_n_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn last_n_chars(s: &str, n: usize) -> String {
    let total = s.chars().count();
    if n >= total {
        return s.to_string();
    }
    s.chars().skip(total - n).collect()
}

pub fn clip_text_with_config(input: &str, cfg: &BudgetConfig) -> (String, CaptureStats) {
    let original_chars = input.chars().count();
    let original_lines = input.lines().count();
    let mode_used = choose_clip_mode(input, &cfg.clip_mode);
    let lines: Vec<&str> = input.lines().collect();
    let line_limited = if lines.len() <= cfg.budget_lines {
        input.to_string()
    } else if mode_used == "tail" {
        lines[lines.len().saturating_sub(cfg.budget_lines)..].join("\n")
    } else {
        lines[..cfg.budget_lines].join("\n")
    };
    let char_limited = if line_limited.chars().count() <= cfg.budget_chars {
        line_limited
    } else if mode_used == "tail" {
        last_n_chars(&line_limited, cfg.budget_chars)
    } else {
        first_n_chars(&line_limited, cfg.budget_chars)
    };
    let kept_chars = char_limited.chars().count();
    let kept_lines = char_limited.lines().count();
    let clipped = kept_chars < original_chars || kept_lines < original_lines;
    let final_text = if clipped && cfg.clip_footer {
        format!(
            "{char_limited}\n[cx] output clipped: original={}/{}, kept={}/{}, mode={}",
            original_chars, original_lines, kept_chars, kept_lines, mode_used
        )
    } else {
        char_limited
    };
    (
        final_text,
        CaptureStats {
            system_output_len_raw: Some(original_chars as u64),
            system_output_len_processed: Some(input.chars().count() as u64),
            system_output_len_clipped: Some(kept_chars as u64),
            system_output_lines_raw: Some(original_lines as u64),
            system_output_lines_processed: Some(input.lines().count() as u64),
            system_output_lines_clipped: Some(kept_lines as u64),
            clipped: Some(clipped),
            budget_chars: Some(cfg.budget_chars as u64),
            budget_lines: Some(cfg.budget_lines as u64),
            clip_mode: Some(mode_used),
            clip_footer: Some(cfg.clip_footer),
            rtk_used: None,
            capture_provider: None,
        },
    )
}

pub fn should_use_rtk(
    cmd: &[String],
    provider_mode: &str,
    rtk_enabled: bool,
    rtk_usable: bool,
) -> bool {
    let supported = cmd
        .first()
        .map(|c| is_rtk_supported_prefix(c))
        .unwrap_or(false);
    match provider_mode {
        "rtk" => rtk_enabled && supported && rtk_usable,
        "native" => false,
        _ => rtk_enabled && supported && rtk_usable,
    }
}

fn run_capture(command: &[String]) -> Result<(String, i32), String> {
    if command.is_empty() {
        return Err("missing command".to_string());
    }
    let mut c = Command::new(&command[0]);
    if command.len() > 1 {
        c.args(&command[1..]);
    }
    let output = c
        .output()
        .map_err(|e| format!("failed to execute '{}': {e}", command[0]))?;
    let status = output.status.code().unwrap_or(1);
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !stderr.trim().is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }
    Ok((combined, status))
}

pub fn run_system_command_capture(cmd: &[String]) -> Result<(String, i32, CaptureStats), String> {
    if cmd.is_empty() {
        return Err("missing command".to_string());
    }
    let (raw_out, status) = run_capture(cmd)?;
    let rtk_enabled = env::var("CX_RTK_SYSTEM")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let native_reduce = env::var("CX_NATIVE_REDUCE")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let provider_mode = env::var("CX_CAPTURE_PROVIDER").unwrap_or_else(|_| "auto".to_string());
    let provider_mode = match provider_mode.as_str() {
        "rtk" | "native" | "auto" => provider_mode,
        _ => "auto".to_string(),
    };
    let rtk_usable = rtk_is_usable();
    let use_rtk = should_use_rtk(cmd, &provider_mode, rtk_enabled, rtk_usable);
    let mut provider_used = if use_rtk { "rtk" } else { "native" }.to_string();
    let processed = if use_rtk {
        let mut rtk_cmd = vec!["rtk".to_string()];
        rtk_cmd.extend_from_slice(cmd);
        match run_capture(&rtk_cmd) {
            Ok((rtk_out, rtk_status)) if rtk_status == 0 && !rtk_out.trim().is_empty() => rtk_out,
            _ => {
                provider_used = "native".to_string();
                raw_out.clone()
            }
        }
    } else {
        raw_out.clone()
    };
    let reduced = if native_reduce {
        native_reduce_output(cmd, &processed)
    } else {
        processed
    };
    let (clipped_text, mut stats) = clip_text_with_config(&reduced, &budget_config_from_env());
    stats.rtk_used = Some(provider_used == "rtk");
    stats.capture_provider = Some(provider_used);
    Ok((clipped_text, status, stats))
}

pub fn chunk_text_by_budget(input: &str, chunk_chars: usize) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_chars = 0usize;
    for line in input.lines() {
        let line_chars = line.chars().count() + 1;
        if cur_chars > 0 && cur_chars + line_chars > chunk_chars {
            chunks.push(cur);
            cur = String::new();
            cur_chars = 0;
        }
        cur.push_str(line);
        cur.push('\n');
        cur_chars += line_chars;
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    if chunks.is_empty() {
        vec![String::new()]
    } else {
        chunks
    }
}

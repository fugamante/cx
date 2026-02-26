use std::env;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::process::{run_command_output_with_timeout, run_command_status_with_timeout};

static RTK_WARNED_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

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
    let mut cmd = Command::new("rtk");
    cmd.arg("--version");
    let out = run_command_output_with_timeout(cmd, "rtk --version").ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    if s.trim().is_empty() {
        s = String::from_utf8_lossy(&out.stderr).to_string();
    }
    let t = s.trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

pub fn rtk_is_usable() -> bool {
    let mut cmd = Command::new("rtk");
    cmd.arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if run_command_status_with_timeout(cmd, "rtk --help")
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
            crate::cx_eprintln!(
                "cxrs: unable to parse rtk version; falling back to raw command output."
            );
        }
        return false;
    };
    let min = parse_semver_triplet(&min_v).unwrap_or((0, 0, 0));
    if semver_cmp(cur, min) < 0 {
        if !RTK_WARNED_UNSUPPORTED.swap(true, Ordering::SeqCst) {
            crate::cx_eprintln!(
                "cxrs: rtk version '{}' is below supported minimum '{}'; falling back to raw command output.",
                ver_raw,
                min_v
            );
        }
        return false;
    }
    if !max_v.is_empty() {
        let max = parse_semver_triplet(&max_v).unwrap_or((u64::MAX, u64::MAX, u64::MAX));
        if semver_cmp(cur, max) > 0 {
            if !RTK_WARNED_UNSUPPORTED.swap(true, Ordering::SeqCst) {
                crate::cx_eprintln!(
                    "cxrs: rtk version '{}' is above supported maximum '{}'; falling back to raw command output.",
                    ver_raw,
                    max_v
                );
            }
            return false;
        }
    }
    true
}

fn is_rtk_supported_prefix(cmd0: &str) -> bool {
    matches!(
        cmd0,
        "git" | "diff" | "ls" | "tree" | "grep" | "test" | "log" | "read"
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

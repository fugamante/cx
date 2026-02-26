use std::io::Write;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use wait_timeout::ChildExt;

use crate::config::DEFAULT_CMD_TIMEOUT_SECS;

fn parse_env_secs(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|v| v.max(1))
}

fn timeout_secs_for_label(label: &str) -> u64 {
    let lower = label.to_ascii_lowercase();
    if (lower.contains("codex") || lower.contains("ollama"))
        && let Some(v) = parse_env_secs("CX_TIMEOUT_LLM_SECS")
    {
        return v;
    }
    if lower.contains("git")
        && let Some(v) = parse_env_secs("CX_TIMEOUT_GIT_SECS")
    {
        return v;
    }
    if (lower.contains("bash") || lower.contains("pbcopy") || lower.contains("shell"))
        && let Some(v) = parse_env_secs("CX_TIMEOUT_SHELL_SECS")
    {
        return v;
    }
    parse_env_secs("CX_CMD_TIMEOUT_SECS")
        .unwrap_or(DEFAULT_CMD_TIMEOUT_SECS as u64)
        .max(1)
}

fn timeout_duration(label: &str) -> Duration {
    Duration::from_secs(timeout_secs_for_label(label))
}

fn timeout_error(label: &str) -> String {
    format!("{label} timed out after {}s", timeout_secs_for_label(label))
}

pub fn parse_timeout_secs_from_error(err: &str) -> Option<u64> {
    let marker = " timed out after ";
    let start = err.find(marker)? + marker.len();
    let tail = &err[start..];
    let num: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    if num.is_empty() {
        None
    } else {
        num.parse::<u64>().ok()
    }
}

pub fn is_timeout_error(err: &str) -> bool {
    err.contains(" timed out after ")
}

fn terminate_pid(pid: u32) {
    let pid_s = pid.to_string();
    let _ = Command::new("kill").args(["-TERM", &pid_s]).status();
}

fn kill_pid(pid: u32) {
    let pid_s = pid.to_string();
    let _ = Command::new("kill").args(["-KILL", &pid_s]).status();
}

fn wait_child_status(child: &mut Child, label: &str) -> Result<ExitStatus, String> {
    match child
        .wait_timeout(timeout_duration(label))
        .map_err(|e| format!("{label} wait timeout error: {e}"))?
    {
        Some(status) => Ok(status),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err(timeout_error(label))
        }
    }
}

pub fn run_command_status_with_timeout(
    mut cmd: Command,
    label: &str,
) -> Result<ExitStatus, String> {
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("{label} spawn failed: {e}"))?;
    wait_child_status(&mut child, label)
}

pub fn run_command_output_with_timeout(mut cmd: Command, label: &str) -> Result<Output, String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| format!("{label} spawn failed: {e}"))?;
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout_duration(label)) {
        Ok(res) => res.map_err(|e| format!("{label} read output failed: {e}")),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            terminate_pid(pid);
            if rx.recv_timeout(Duration::from_secs(2)).is_err() {
                kill_pid(pid);
            }
            Err(timeout_error(label))
        }
        Err(_) => Err(format!("{label} output worker channel closed unexpectedly")),
    }
}

pub fn run_command_with_stdin_output_with_timeout(
    mut cmd: Command,
    stdin_text: &str,
    label: &str,
) -> Result<Output, String> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("{label} spawn failed: {e}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(stdin_text.as_bytes())
            .map_err(|e| format!("{label} failed writing stdin: {e}"))?;
    }
    let _ = child.stdin.take();
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout_duration(label)) {
        Ok(res) => res.map_err(|e| format!("{label} read output failed: {e}")),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            terminate_pid(pid);
            if rx.recv_timeout(Duration::from_secs(2)).is_err() {
                kill_pid(pid);
            }
            Err(timeout_error(label))
        }
        Err(_) => Err(format!("{label} output worker channel closed unexpectedly")),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_timeout_error, parse_timeout_secs_from_error};

    #[test]
    fn parse_timeout_secs_handles_expected_format() {
        let msg = "codex exec --json - timed out after 45s";
        assert_eq!(parse_timeout_secs_from_error(msg), Some(45));
    }

    #[test]
    fn parse_timeout_secs_returns_none_for_non_timeout() {
        let msg = "codex exec failed with status 1";
        assert_eq!(parse_timeout_secs_from_error(msg), None);
    }

    #[test]
    fn timeout_error_detector_matches_timeout_phrase() {
        assert!(is_timeout_error("foo timed out after 2s"));
        assert!(!is_timeout_error("foo failed"));
    }
}

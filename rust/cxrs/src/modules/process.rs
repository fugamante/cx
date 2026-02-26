use std::fmt;
use std::io::Write;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use wait_timeout::ChildExt;

use crate::config::DEFAULT_CMD_TIMEOUT_SECS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeoutInfo {
    pub label: String,
    pub timeout_secs: u64,
}

#[derive(Debug)]
pub enum ProcessError {
    Timeout(TimeoutInfo),
    Message(String),
}

impl ProcessError {
    pub fn timeout_info(&self) -> Option<&TimeoutInfo> {
        match self {
            Self::Timeout(info) => Some(info),
            Self::Message(_) => None,
        }
    }
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout(info) => {
                write!(f, "{} timed out after {}s", info.label, info.timeout_secs)
            }
            Self::Message(msg) => write!(f, "{msg}"),
        }
    }
}

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

fn timeout_error(label: &str) -> ProcessError {
    ProcessError::Timeout(TimeoutInfo {
        label: label.to_string(),
        timeout_secs: timeout_secs_for_label(label),
    })
}

fn terminate_pid(pid: u32) {
    let pid_s = pid.to_string();
    let _ = Command::new("kill").args(["-TERM", &pid_s]).status();
}

fn kill_pid(pid: u32) {
    let pid_s = pid.to_string();
    let _ = Command::new("kill").args(["-KILL", &pid_s]).status();
}

fn wait_child_status(child: &mut Child, label: &str) -> Result<ExitStatus, ProcessError> {
    match child
        .wait_timeout(timeout_duration(label))
        .map_err(|e| ProcessError::Message(format!("{label} wait timeout error: {e}")))?
    {
        Some(status) => Ok(status),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err(timeout_error(label))
        }
    }
}

pub fn run_command_status_with_timeout_meta(
    mut cmd: Command,
    label: &str,
) -> Result<ExitStatus, ProcessError> {
    let mut child = cmd
        .spawn()
        .map_err(|e| ProcessError::Message(format!("{label} spawn failed: {e}")))?;
    wait_child_status(&mut child, label)
}

pub fn run_command_status_with_timeout(cmd: Command, label: &str) -> Result<ExitStatus, String> {
    run_command_status_with_timeout_meta(cmd, label).map_err(|e| e.to_string())
}

pub fn run_command_output_with_timeout_meta(
    mut cmd: Command,
    label: &str,
) -> Result<Output, ProcessError> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| ProcessError::Message(format!("{label} spawn failed: {e}")))?;
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout_duration(label)) {
        Ok(res) => {
            res.map_err(|e| ProcessError::Message(format!("{label} read output failed: {e}")))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            terminate_pid(pid);
            if rx.recv_timeout(Duration::from_secs(2)).is_err() {
                kill_pid(pid);
            }
            Err(timeout_error(label))
        }
        Err(_) => Err(ProcessError::Message(format!(
            "{label} output worker channel closed unexpectedly"
        ))),
    }
}

pub fn run_command_output_with_timeout(cmd: Command, label: &str) -> Result<Output, String> {
    run_command_output_with_timeout_meta(cmd, label).map_err(|e| e.to_string())
}

pub fn run_command_with_stdin_output_with_timeout_meta(
    mut cmd: Command,
    stdin_text: &str,
    label: &str,
) -> Result<Output, ProcessError> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| ProcessError::Message(format!("{label} spawn failed: {e}")))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(stdin_text.as_bytes())
            .map_err(|e| ProcessError::Message(format!("{label} failed writing stdin: {e}")))?;
    }
    let _ = child.stdin.take();
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout_duration(label)) {
        Ok(res) => {
            res.map_err(|e| ProcessError::Message(format!("{label} read output failed: {e}")))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            terminate_pid(pid);
            if rx.recv_timeout(Duration::from_secs(2)).is_err() {
                kill_pid(pid);
            }
            Err(timeout_error(label))
        }
        Err(_) => Err(ProcessError::Message(format!(
            "{label} output worker channel closed unexpectedly"
        ))),
    }
}

pub fn run_command_with_stdin_output_with_timeout(
    cmd: Command,
    stdin_text: &str,
    label: &str,
) -> Result<Output, String> {
    run_command_with_stdin_output_with_timeout_meta(cmd, stdin_text, label)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{ProcessError, TimeoutInfo};

    #[test]
    fn timeout_error_returns_structured_metadata() {
        let err = ProcessError::Timeout(TimeoutInfo {
            label: "timeout-test".to_string(),
            timeout_secs: 17,
        });
        match err {
            ProcessError::Timeout(info) => {
                assert_eq!(info.label, "timeout-test");
                assert_eq!(info.timeout_secs, 17);
            }
            ProcessError::Message(msg) => panic!("expected timeout, got message: {msg}"),
        }
    }

    #[test]
    fn process_error_display_for_message() {
        let msg = ProcessError::Message("boom".to_string());
        assert_eq!(msg.to_string(), "boom");
    }
}

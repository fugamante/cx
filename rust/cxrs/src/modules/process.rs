use std::io::Write;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use wait_timeout::ChildExt;

use crate::config::DEFAULT_CMD_TIMEOUT_SECS;

fn timeout_secs() -> u64 {
    std::env::var("CX_CMD_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_CMD_TIMEOUT_SECS as u64)
        .max(1)
}

fn timeout_duration() -> Duration {
    Duration::from_secs(timeout_secs())
}

fn timeout_error(label: &str) -> String {
    format!("{label} timed out after {}s", timeout_secs())
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
        .wait_timeout(timeout_duration())
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
    match rx.recv_timeout(timeout_duration()) {
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
    match rx.recv_timeout(timeout_duration()) {
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

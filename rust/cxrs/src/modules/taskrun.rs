use serde_json::Value;
use std::env;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::capture::{BudgetConfig, clip_text_with_config};
use crate::config::app_config;
use crate::config::cli_app_name;
use crate::local_models::resolve_model_for_backend;
use crate::logs::file_len;
use crate::paths::{repo_root, resolve_log_file};
use crate::runlog::{RunLogInput, log_primary_run};
use crate::runtime::llm_backend;
use crate::state::{read_state_value, value_at_path};
use crate::types::{ExecutionResult, LlmOutputKind, TaskInput, TaskRecord, TaskSpec};

#[derive(Debug, Clone)]
pub enum TaskRunError {
    Critical(String),
}

impl fmt::Display for TaskRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskRunError::Critical(s) => write!(f, "{s}"),
        }
    }
}

pub struct TaskRunner {
    pub read_tasks: fn() -> Result<Vec<TaskRecord>, String>,
    pub write_tasks: fn(&[TaskRecord]) -> Result<(), String>,
    pub current_task_id: fn() -> Option<String>,
    pub current_task_parent_id: fn() -> Option<String>,
    pub set_state_path: fn(&str, Value) -> Result<(), String>,
    pub utc_now_iso: fn() -> String,
    pub cmd_commitjson: fn() -> i32,
    pub cmd_commitmsg: fn() -> i32,
    pub cmd_diffsum: fn(bool) -> i32,
    pub cmd_next: fn(&[String]) -> i32,
    pub cmd_fix_run: fn(&[String]) -> i32,
    pub cmd_fix: fn(&[String]) -> i32,
    pub cmd_cx: fn(&[String]) -> i32,
    pub cmd_cxj: fn(&[String]) -> i32,
    pub cmd_cxo: fn(&[String]) -> i32,
    pub execute_task: fn(TaskSpec) -> Result<ExecutionResult, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSandboxConfig {
    pub enabled: bool,
    pub image: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TaskSandboxReadiness {
    pub enabled: bool,
    pub active: bool,
    pub image: Option<String>,
    pub ready: bool,
    pub docker_available: bool,
    pub image_available: bool,
    pub repo_mount_writable: bool,
    pub entrypoint_available: bool,
    pub issues: Vec<String>,
    pub recommended_action: Option<String>,
}

fn env_bool_override(name: &str) -> Option<bool> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .and_then(|v| match v.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
}

fn state_pref_bool(path: &str) -> Option<bool> {
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, path))
        .and_then(Value::as_bool)
}

fn state_pref_string(path: &str) -> Option<String> {
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, path))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

pub fn task_sandbox_config() -> TaskSandboxConfig {
    let enabled = env_bool_override("CX_TASK_SANDBOX_ENABLED")
        .or_else(|| state_pref_bool("preferences.task_sandbox.enabled"))
        .unwrap_or(false);
    let image = env::var("CX_TASK_SANDBOX_IMAGE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| state_pref_string("preferences.task_sandbox.image"));
    TaskSandboxConfig { enabled, image }
}

fn task_sandbox_active() -> bool {
    env_bool_override("CX_TASK_SANDBOX_ACTIVE").unwrap_or(false)
}

fn passthrough_container_envs(cmd: &mut Command) {
    let mut pairs: Vec<(String, String)> = env::vars()
        .filter(|(k, _)| {
            k.starts_with("CX_")
                || k.starts_with("OPENAI_")
                || k.starts_with("OLLAMA_")
                || matches!(k.as_str(), "HTTP_PROXY" | "HTTPS_PROXY" | "NO_PROXY")
        })
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in pairs {
        if key == "CX_TASK_SANDBOX_ACTIVE" {
            continue;
        }
        cmd.arg("-e");
        cmd.arg(format!("{key}={value}"));
    }
}

fn sandbox_lane_detail(image: &str) -> String {
    format!("docker:{image}")
}

fn current_uid_gid() -> Result<(String, String), String> {
    let uid = crate::process::run_command_output_with_timeout(
        {
            let mut cmd = Command::new("id");
            cmd.arg("-u");
            cmd
        },
        "id -u",
    )
    .map_err(|e| format!("{} task sandbox: {e}", cli_app_name()))?;
    let gid = crate::process::run_command_output_with_timeout(
        {
            let mut cmd = Command::new("id");
            cmd.arg("-g");
            cmd
        },
        "id -g",
    )
    .map_err(|e| format!("{} task sandbox: {e}", cli_app_name()))?;
    let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
    let gid = String::from_utf8_lossy(&gid.stdout).trim().to_string();
    if uid.is_empty() || gid.is_empty() {
        return Err(format!(
            "{} task sandbox: unable to resolve uid/gid for docker run",
            cli_app_name()
        ));
    }
    Ok((uid, gid))
}

fn command_success(cmd: Command, label: &str) -> bool {
    crate::process::run_command_output_with_timeout(cmd, label)
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn sandbox_probe_success(root: &Path, image: &str) -> bool {
    let Ok((uid, gid)) = current_uid_gid() else {
        return false;
    };
    let mut cmd = Command::new("docker");
    cmd.arg("run");
    cmd.arg("--rm");
    cmd.arg("--user");
    cmd.arg(format!("{uid}:{gid}"));
    cmd.arg("--workdir");
    cmd.arg("/work");
    cmd.arg("-v");
    cmd.arg(format!("{}:/work", root.display()));
    cmd.arg(image);
    cmd.arg("bash");
    cmd.arg("-lc");
    cmd.arg(
        "test -d .cx && test -w .cx && \
if [[ -x ./bin/xshelf || -x ./bin/cx ]]; then exit 0; fi && \
if command -v xshelf >/dev/null 2>&1 || command -v cx >/dev/null 2>&1; then exit 0; fi && \
exit 127",
    );
    command_success(cmd, "task sandbox readiness docker run")
}

pub fn task_sandbox_readiness() -> TaskSandboxReadiness {
    let cfg = task_sandbox_config();
    let active = task_sandbox_active();
    let mut issues: Vec<String> = Vec::new();
    if !cfg.enabled {
        issues.push("sandbox_disabled".to_string());
    }
    if cfg.image.is_none() {
        issues.push("image_unset".to_string());
    }
    let docker_available = {
        let mut cmd = Command::new("docker");
        cmd.arg("--version");
        command_success(cmd, "docker --version")
    };
    if !docker_available {
        issues.push("docker_unavailable".to_string());
    }
    let image_available = if docker_available {
        if let Some(image) = cfg.image.as_deref() {
            let mut cmd = Command::new("docker");
            cmd.arg("image");
            cmd.arg("inspect");
            cmd.arg(image);
            command_success(cmd, "docker image inspect")
        } else {
            false
        }
    } else {
        false
    };
    if cfg.image.is_some() && !image_available {
        issues.push("image_unavailable".to_string());
    }
    let root = repo_root();
    let repo_mount_writable = root
        .as_ref()
        .map(|p| {
            p.join(".cx").is_dir()
                && !p
                    .join(".cx")
                    .metadata()
                    .map(|m| m.permissions().readonly())
                    .unwrap_or(true)
        })
        .unwrap_or(false);
    if root.is_none() {
        issues.push("repo_unavailable".to_string());
    } else if !repo_mount_writable {
        issues.push("repo_state_not_writable".to_string());
    }
    let entrypoint_available = if docker_available && image_available && repo_mount_writable {
        if let (Some(root), Some(image)) = (root.as_ref(), cfg.image.as_deref()) {
            sandbox_probe_success(root, image)
        } else {
            false
        }
    } else {
        false
    };
    if cfg.enabled
        && cfg.image.is_some()
        && docker_available
        && image_available
        && !entrypoint_available
    {
        issues.push("entrypoint_unavailable".to_string());
    }
    let ready = cfg.enabled
        && cfg.image.is_some()
        && docker_available
        && image_available
        && repo_mount_writable
        && entrypoint_available
        && issues.is_empty();
    let recommended_action = if ready {
        None
    } else if issues.iter().any(|i| i == "sandbox_disabled") {
        Some("run `xshelf task sandbox enable`".to_string())
    } else if issues.iter().any(|i| i == "image_unset") {
        Some("run `xshelf task sandbox set-image <image>`".to_string())
    } else if issues.iter().any(|i| i == "docker_unavailable") {
        Some("start Docker and make `docker --version` work".to_string())
    } else if issues.iter().any(|i| i == "image_unavailable") {
        Some("build or pull the configured sandbox image".to_string())
    } else if issues.iter().any(|i| i == "repo_state_not_writable") {
        Some("ensure repo-local `.cx/` state is writable from the container user".to_string())
    } else if issues.iter().any(|i| i == "entrypoint_unavailable") {
        Some(
            "install xshelf/cx in the image or expose repo-local ./bin/xshelf or ./bin/cx"
                .to_string(),
        )
    } else {
        Some("inspect sandbox configuration".to_string())
    };
    TaskSandboxReadiness {
        enabled: cfg.enabled,
        active,
        image: cfg.image,
        ready,
        docker_available,
        image_available,
        repo_mount_writable,
        entrypoint_available,
        issues,
        recommended_action,
    }
}

fn task_in_sandbox(
    id: &str,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    emit_output: bool,
) -> Result<(i32, Option<String>), String> {
    let cfg = task_sandbox_config();
    let image = cfg.image.ok_or_else(|| {
        format!(
            "{} task sandbox: image is required when sandbox is enabled",
            cli_app_name()
        )
    })?;
    let root = repo_root().ok_or_else(|| {
        format!(
            "{} task sandbox: not inside a git repository",
            cli_app_name()
        )
    })?;
    let (uid, gid) = current_uid_gid()?;

    let log_cursor = capture_log_cursor();
    let mut docker = Command::new("docker");
    docker.arg("run");
    docker.arg("--rm");
    docker.arg("--user");
    docker.arg(format!("{uid}:{gid}"));
    docker.arg("--workdir");
    docker.arg("/work");
    docker.arg("-e");
    docker.arg("HOME=/tmp/cx-home");
    docker.arg("-e");
    docker.arg("CARGO_TARGET_DIR=.cx/task-sandbox/target");
    docker.arg("-e");
    docker.arg("CX_TASK_SANDBOX_ACTIVE=1");
    docker.arg("-e");
    docker.arg("CX_EXECUTION_LANE=container");
    docker.arg("-e");
    docker.arg(format!(
        "CX_EXECUTION_LANE_DETAIL={}",
        sandbox_lane_detail(&image)
    ));
    passthrough_container_envs(&mut docker);
    docker.arg("-v");
    docker.arg(format!("{}:/work", root.display()));
    docker.arg(&image);
    docker.arg("bash");
    docker.arg("-lc");

    let mut inner_args = vec![
        "task".to_string(),
        "run".to_string(),
        id.to_string(),
        "--managed-by-parent".to_string(),
    ];
    if let Some(mode) = mode_override {
        inner_args.push("--mode".to_string());
        inner_args.push(mode.to_string());
    }
    if let Some(backend) = backend_override {
        inner_args.push("--backend".to_string());
        inner_args.push(backend.to_string());
    }
    let quoted = inner_args
        .iter()
        .map(|arg| shell_words::quote(arg).to_string())
        .collect::<Vec<String>>()
        .join(" ");
    let script = format!(
        "mkdir -p \"$HOME\" \"$CARGO_TARGET_DIR\" && \
if [[ -x ./bin/xshelf ]]; then app=./bin/xshelf; \
elif [[ -x ./bin/cx ]]; then app=./bin/cx; \
elif command -v xshelf >/dev/null 2>&1; then app=xshelf; \
elif command -v cx >/dev/null 2>&1; then app=cx; \
else echo \"{} task sandbox: xshelf/cx is not available inside the container image\" >&2; exit 127; fi && \
\"$app\" {quoted}",
        cli_app_name()
    );
    docker.arg(script);

    let status_code = if emit_output {
        crate::process::run_command_status_with_timeout(docker, "task sandbox docker run")
            .map_err(|e| format!("{} task sandbox: {e}", cli_app_name()))?
            .code()
            .unwrap_or(1)
    } else {
        crate::process::run_command_output_with_timeout(docker, "task sandbox docker run")
            .map_err(|e| format!("{} task sandbox: {e}", cli_app_name()))?
            .status
            .code()
            .unwrap_or(1)
    };
    let recovered = log_cursor
        .as_ref()
        .and_then(|(p, offset)| recover_execution_id_from_log(p, *offset));
    Ok((status_code, recovered))
}

#[derive(Debug, Clone)]
struct ReplicaOutcome {
    index: u32,
    status_code: i32,
    execution_id: Option<String>,
    error: Option<String>,
}

struct ReplicaRunConfig<'a> {
    mode_override: Option<&'a str>,
    backend_override: Option<&'a str>,
    emit_output: bool,
    replica_index: u32,
    replica_count: u32,
    converge_mode: &'a str,
}

fn parse_words(input: &str) -> Vec<String> {
    match shell_words::split(input) {
        Ok(v) => v,
        Err(_) => input
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
    }
}

fn command_status_or_usage(run: fn(&[String]) -> i32, args: &[String]) -> i32 {
    if args.is_empty() { 2 } else { run(args) }
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn clip_for_prompt(input: &str, max_chars: usize, max_lines: usize) -> String {
    let cfg = BudgetConfig {
        budget_chars: max_chars.max(1),
        budget_lines: max_lines.max(1),
        clip_mode: "smart".to_string(),
        clip_footer: false,
    };
    let (clipped, _) = clip_text_with_config(input, &cfg);
    clipped
}

fn compact_prompt_text(input: &str) -> String {
    input
        .lines()
        .map(str::trim_end)
        .collect::<Vec<&str>>()
        .join("\n")
        .trim()
        .to_string()
}

fn task_prompt(task: &TaskRecord) -> String {
    let cfg = app_config();
    let objective_chars = env_usize("CX_TASK_OBJECTIVE_MAX_CHARS", cfg.budget_chars.min(3000));
    let objective_lines = env_usize("CX_TASK_OBJECTIVE_MAX_LINES", cfg.budget_lines.min(120));
    let context_chars = env_usize(
        "CX_TASK_CONTEXT_MAX_CHARS",
        cfg.budget_chars.saturating_mul(2).min(6000),
    );
    let context_lines = env_usize("CX_TASK_CONTEXT_MAX_LINES", cfg.budget_lines.min(180));
    let objective = compact_prompt_text(&clip_for_prompt(
        task.objective.trim(),
        objective_chars,
        objective_lines,
    ));
    let context = compact_prompt_text(&clip_for_prompt(
        task.context_ref.trim(),
        context_chars,
        context_lines,
    ));
    if context.is_empty() {
        return format!(
            "Task Objective:\n{}\n\nRespond with concise execution notes and next actions.",
            objective
        );
    }
    format!(
        "Task Objective:\n{}\n\nContext Ref:\n{}\n\nRespond with concise execution notes and next actions.",
        objective, context
    )
}

fn task_backend_override(task: &TaskRecord) -> Option<String> {
    let backend = task.backend.trim().to_lowercase();
    match backend.as_str() {
        "primary" => Some("primary".to_string()),
        "ollama" | "llamacpp" | "mlx" => Some(backend),
        "llama.cpp" | "llama_cpp" => Some("llamacpp".to_string()),
        _ => None,
    }
}

fn task_mode_override(task: &TaskRecord) -> Option<String> {
    match task.profile.trim().to_lowercase().as_str() {
        "fast" => Some("lean".to_string()),
        "quality" => Some("verbose".to_string()),
        "schema_strict" => Some("deterministic".to_string()),
        _ => None,
    }
}

fn task_model_override(task: &TaskRecord) -> Option<String> {
    task.model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_model_backend(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "ollama" => "ollama".to_string(),
        "llamacpp" | "llama.cpp" | "llama_cpp" => "llamacpp".to_string(),
        "mlx" => "mlx".to_string(),
        _ => "primary".to_string(),
    }
}

fn resolved_task_model_override(
    backend_override: Option<&str>,
    model: Option<&str>,
) -> Result<Option<(String, String)>, String> {
    let raw_model = match model.map(str::trim).filter(|s| !s.is_empty()) {
        Some(v) => v,
        None => return Ok(None),
    };
    let backend_raw = backend_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(llm_backend);
    let backend = normalize_model_backend(&backend_raw);
    if matches!(backend.as_str(), "ollama" | "llamacpp" | "mlx") {
        let resolved = resolve_model_for_backend(&backend, raw_model)?.resolved_model;
        return Ok(Some((backend, resolved)));
    }
    Ok(Some((backend, raw_model.to_string())))
}

fn set_optional_env(name: &str, value: Option<String>) {
    match value {
        Some(v) => unsafe { env::set_var(name, v) },
        None => unsafe { env::remove_var(name) },
    }
}

fn run_task_prompt(
    runner: &TaskRunner,
    task: &TaskRecord,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    model_override: Option<&str>,
    emit_output: bool,
) -> Result<(i32, Option<String>), String> {
    let prev_mode = env::var("CX_MODE").ok();
    let prev_backend = env::var("CX_LLM_BACKEND").ok();
    let prev_ollama_model = env::var("CX_OLLAMA_MODEL").ok();
    let prev_llama_cpp_model = env::var("CX_LLAMA_CPP_MODEL").ok();
    let prev_mlx_model = env::var("CX_MLX_MODEL").ok();
    let prev_primary_model = env::var("CX_MODEL").ok();
    if let Some(mode) = mode_override {
        // scoped overrides for prompt-based task execution.
        unsafe { env::set_var("CX_MODE", mode) };
    }
    if let Some(backend) = backend_override {
        unsafe { env::set_var("CX_LLM_BACKEND", backend) };
    }
    let resolved_model_override =
        match resolved_task_model_override(backend_override, model_override) {
            Ok(v) => v,
            Err(e) => {
                set_optional_env("CX_MODE", prev_mode);
                set_optional_env("CX_LLM_BACKEND", prev_backend);
                set_optional_env("CX_OLLAMA_MODEL", prev_ollama_model);
                set_optional_env("CX_LLAMA_CPP_MODEL", prev_llama_cpp_model);
                set_optional_env("CX_MLX_MODEL", prev_mlx_model);
                set_optional_env("CX_MODEL", prev_primary_model);
                return Err(e);
            }
        };
    if let Some((backend, model)) = resolved_model_override {
        match backend.as_str() {
            "llamacpp" => unsafe { env::set_var("CX_LLAMA_CPP_MODEL", model) },
            "mlx" => unsafe { env::set_var("CX_MLX_MODEL", model) },
            "primary" => unsafe { env::set_var("CX_MODEL", model) },
            _ => unsafe { env::set_var("CX_OLLAMA_MODEL", model) },
        }
    }
    let exec_result = (runner.execute_task)(TaskSpec {
        command_name: "cxtask_run".to_string(),
        input: TaskInput::Prompt(task_prompt(task)),
        output_kind: LlmOutputKind::AgentText,
        schema: None,
        schema_task_input: None,
        logging_enabled: true,
        capture_override: None,
    });
    set_optional_env("CX_MODE", prev_mode);
    set_optional_env("CX_LLM_BACKEND", prev_backend);
    set_optional_env("CX_OLLAMA_MODEL", prev_ollama_model);
    set_optional_env("CX_LLAMA_CPP_MODEL", prev_llama_cpp_model);
    set_optional_env("CX_MLX_MODEL", prev_mlx_model);
    set_optional_env("CX_MODEL", prev_primary_model);
    let res = exec_result?;
    if emit_output {
        println!("{}", res.stdout);
    }
    Ok((0, Some(res.execution_id)))
}

fn run_objective_subprocess(
    objective_words: &[String],
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    model_override: Option<&str>,
    emit_output: bool,
) -> Result<i32, String> {
    if objective_words.is_empty() {
        return Ok(2);
    }
    let exe = env::current_exe()
        .map_err(|e| format!("{} task run: current_exe failed: {e}", cli_app_name()))?;
    let mut cmd = Command::new(exe);
    cmd.args(objective_words);
    if let Some(mode) = mode_override {
        cmd.env("CX_MODE", mode);
    }
    if let Some(backend) = backend_override {
        cmd.env("CX_LLM_BACKEND", backend);
    }
    let resolved_model_override = resolved_task_model_override(backend_override, model_override)
        .map_err(|e| {
            format!(
                "{} task run: model override resolution failed: {e}",
                cli_app_name()
            )
        })?;
    if let Some((backend, model)) = resolved_model_override {
        match backend.as_str() {
            "llamacpp" => {
                cmd.env("CX_LLAMA_CPP_MODEL", model);
            }
            "mlx" => {
                cmd.env("CX_MLX_MODEL", model);
            }
            "primary" => {
                cmd.env("CX_MODEL", model);
            }
            _ => {
                cmd.env("CX_OLLAMA_MODEL", model);
            }
        }
    }
    if emit_output {
        let status = crate::process::run_command_status_with_timeout(cmd, "cxtask_run subprocess")?;
        return Ok(status.code().unwrap_or(1));
    }
    let output = crate::process::run_command_output_with_timeout(cmd, "cxtask_run subprocess")?;
    Ok(output.status.code().unwrap_or(1))
}

fn capture_log_cursor() -> Option<(PathBuf, u64)> {
    let log_file = resolve_log_file()?;
    Some((log_file.clone(), file_len(&log_file)))
}

fn recover_execution_id_from_log(log_file: &Path, offset: u64) -> Option<String> {
    let file = File::open(log_file).ok()?;
    let mut reader = BufReader::new(file);
    if offset > 0 && reader.seek(SeekFrom::Start(offset)).is_err() {
        return None;
    }
    let mut line = String::new();
    let mut latest: Option<String> = None;
    loop {
        line.clear();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if let Some(exec_id) = v
            .get("execution_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            latest = Some(exec_id.to_string());
        }
    }
    latest
}

fn dispatch_task_command(
    runner: &TaskRunner,
    words: &[String],
    task: &TaskRecord,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    emit_output: bool,
) -> Result<(i32, Option<String>), String> {
    let Some(cmd0) = words.first().map(String::as_str) else {
        return run_task_prompt(
            runner,
            task,
            mode_override,
            backend_override,
            None,
            emit_output,
        );
    };
    let args: Vec<String> = words.iter().skip(1).cloned().collect();
    let model_override = task_model_override(task);
    if !emit_output {
        let code = run_objective_subprocess(
            words,
            mode_override,
            backend_override,
            model_override.as_deref(),
            false,
        )?;
        return Ok((code, None));
    }
    if mode_override.is_some() || backend_override.is_some() || model_override.is_some() {
        match cmd0 {
            "cxcommitjson" | "commitjson" | "cxcommitmsg" | "commitmsg" | "cxdiffsum"
            | "diffsum" | "cxdiffsum_staged" | "diffsum-staged" | "cxnext" | "next"
            | "cxfix_run" | "fix-run" | "cxfix" | "fix" | "cx" | "cxj" | "cxo" => {
                let code = run_objective_subprocess(
                    words,
                    mode_override,
                    backend_override,
                    model_override.as_deref(),
                    true,
                )?;
                return Ok((code, None));
            }
            _ => {}
        }
    }
    let status = match cmd0 {
        "cxcommitjson" | "commitjson" => (runner.cmd_commitjson)(),
        "cxcommitmsg" | "commitmsg" => (runner.cmd_commitmsg)(),
        "cxdiffsum" | "diffsum" => (runner.cmd_diffsum)(false),
        "cxdiffsum_staged" | "diffsum-staged" => (runner.cmd_diffsum)(true),
        "cxnext" | "next" => command_status_or_usage(runner.cmd_next, &args),
        "cxfix_run" | "fix-run" => command_status_or_usage(runner.cmd_fix_run, &args),
        "cxfix" | "fix" => command_status_or_usage(runner.cmd_fix, &args),
        "cx" => command_status_or_usage(runner.cmd_cx, &args),
        "cxj" => command_status_or_usage(runner.cmd_cxj, &args),
        "cxo" => command_status_or_usage(runner.cmd_cxo, &args),
        _ => {
            return run_task_prompt(
                runner,
                task,
                mode_override,
                backend_override,
                model_override.as_deref(),
                emit_output,
            );
        }
    };
    Ok((status, None))
}

fn run_task_objective(
    runner: &TaskRunner,
    task: &TaskRecord,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    emit_output: bool,
) -> Result<(i32, Option<String>), String> {
    let log_cursor = capture_log_cursor();
    let words = parse_words(&task.objective);
    let (status, execution_id) = dispatch_task_command(
        runner,
        &words,
        task,
        mode_override,
        backend_override,
        emit_output,
    )?;
    if execution_id.is_some() {
        return Ok((status, execution_id));
    }
    let recovered = log_cursor
        .as_ref()
        .and_then(|(p, offset)| recover_execution_id_from_log(p, *offset));
    Ok((status, recovered))
}

fn normalize_converge_mode(raw: &str) -> String {
    let m = raw.trim().to_lowercase();
    if matches!(
        m.as_str(),
        "none" | "first_valid" | "majority" | "judge" | "score"
    ) {
        m
    } else {
        "none".to_string()
    }
}

fn effective_replica_count(task: &TaskRecord, mode: &str) -> u32 {
    let n = task.replicas.max(1);
    if mode == "none" { 1 } else { n }
}

fn select_winner(mode: &str, outcomes: &[ReplicaOutcome]) -> ReplicaOutcome {
    if outcomes.is_empty() {
        return ReplicaOutcome {
            index: 1,
            status_code: 1,
            execution_id: None,
            error: Some("no replica outcomes".to_string()),
        };
    }
    match mode {
        "first_valid" => outcomes
            .iter()
            .find(|o| o.status_code == 0)
            .cloned()
            .unwrap_or_else(|| outcomes[0].clone()),
        "majority" => {
            let ok = outcomes.iter().filter(|o| o.status_code == 0).count();
            let fail = outcomes.len().saturating_sub(ok);
            if ok >= fail {
                outcomes
                    .iter()
                    .find(|o| o.status_code == 0)
                    .cloned()
                    .unwrap_or_else(|| outcomes[0].clone())
            } else {
                outcomes
                    .iter()
                    .find(|o| o.status_code != 0)
                    .cloned()
                    .unwrap_or_else(|| outcomes[0].clone())
            }
        }
        "judge" | "score" => {
            let mut scored: Vec<(i64, u32, ReplicaOutcome)> = outcomes
                .iter()
                .cloned()
                .map(|o| (score_outcome(&o), o.index, o))
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            scored
                .first()
                .map(|(_, _, o)| o.clone())
                .unwrap_or_else(|| outcomes[0].clone())
        }
        _ => outcomes[0].clone(),
    }
}

fn score_outcome(outcome: &ReplicaOutcome) -> i64 {
    let success_score = if outcome.status_code == 0 { 1000 } else { 0 };
    let execution_id_bonus = if outcome.execution_id.is_some() {
        100
    } else {
        0
    };
    let error_penalty = outcome.error.as_ref().map(|e| e.len() as i64).unwrap_or(0);
    success_score + execution_id_bonus - error_penalty.min(200)
}

fn score_outcome_breakdown(outcome: &ReplicaOutcome) -> (i64, i64, i64, i64) {
    let success_score = if outcome.status_code == 0 { 1000 } else { 0 };
    let execution_id_bonus = if outcome.execution_id.is_some() {
        100
    } else {
        0
    };
    let error_penalty = outcome.error.as_ref().map(|e| e.len() as i64).unwrap_or(0);
    let bounded_penalty = error_penalty.min(200);
    (
        success_score + execution_id_bonus - bounded_penalty,
        success_score,
        execution_id_bonus,
        bounded_penalty,
    )
}

fn judge_winner_with_model(
    runner: &TaskRunner,
    task: &TaskRecord,
    outcomes: &[ReplicaOutcome],
    mode_override: Option<&str>,
    backend_override: Option<&str>,
) -> Option<(u32, String)> {
    if outcomes.is_empty() {
        return None;
    }
    let candidates = outcomes
        .iter()
        .map(|o| {
            let (score, success_score, execution_id_bonus, error_penalty) =
                score_outcome_breakdown(o);
            serde_json::json!({
                "index": o.index,
                "status_code": o.status_code,
                "score": score,
                "success_score": success_score,
                "execution_id_bonus": execution_id_bonus,
                "error_penalty": error_penalty,
                "has_execution_id": o.execution_id.is_some(),
                "error": o.error
            })
        })
        .collect::<Vec<Value>>();
    let prompt = format!(
        "Task objective:\n{}\n\nSelect the best candidate index using reliability-first judgement. Prefer successful runs, lower error penalties, and stable metadata.\n\nCandidates:\n{}\n\nReturn JSON only.",
        task.objective,
        serde_json::to_string_pretty(&candidates).ok()?
    );
    let schema = crate::types::LoadedSchema {
        name: "converge_judge.schema.json".to_string(),
        path: PathBuf::from("<inline>"),
        value: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["winner_index", "reason"],
            "properties": {
              "winner_index": { "type": "integer", "minimum": 1, "maximum": outcomes.len() as i64 },
              "reason": { "type": "string", "minLength": 1 }
            }
        }),
        id: None,
    };
    let prev_mode = env::var("CX_MODE").ok();
    let prev_backend = env::var("CX_LLM_BACKEND").ok();
    if let Some(mode) = mode_override {
        set_optional_env("CX_MODE", Some(mode.to_string()));
    }
    if let Some(backend) = backend_override {
        set_optional_env("CX_LLM_BACKEND", Some(backend.to_string()));
    }
    let res = (runner.execute_task)(TaskSpec {
        command_name: "cxtask_converge_judge".to_string(),
        input: TaskInput::Prompt(prompt.clone()),
        output_kind: LlmOutputKind::SchemaJson,
        schema: Some(schema),
        schema_task_input: Some(prompt),
        logging_enabled: true,
        capture_override: None,
    });
    set_optional_env("CX_MODE", prev_mode);
    set_optional_env("CX_LLM_BACKEND", prev_backend);
    let Ok(result) = res else {
        return None;
    };
    let parsed: Value = serde_json::from_str(&result.stdout).ok()?;
    let winner = parsed.get("winner_index").and_then(Value::as_u64)? as u32;
    if !outcomes.iter().any(|o| o.index == winner) {
        return None;
    }
    let reason = parsed
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("judge_selected")
        .to_string();
    Some((winner, reason))
}

fn run_replica(
    runner: &TaskRunner,
    task: &TaskRecord,
    config: ReplicaRunConfig<'_>,
) -> ReplicaOutcome {
    set_optional_env(
        "CX_TASK_REPLICA_INDEX",
        Some(config.replica_index.to_string()),
    );
    set_optional_env(
        "CX_TASK_REPLICA_COUNT",
        Some(config.replica_count.to_string()),
    );
    set_optional_env(
        "CX_TASK_CONVERGE_MODE",
        Some(config.converge_mode.to_string()),
    );
    set_optional_env("CX_TASK_CONVERGE_WINNER", None);
    let run_result = if !task_sandbox_active() && task_sandbox_config().enabled {
        task_in_sandbox(
            &task.id,
            config.mode_override,
            config.backend_override,
            config.emit_output,
        )
    } else {
        run_task_objective(
            runner,
            task,
            config.mode_override,
            config.backend_override,
            config.emit_output,
        )
    };
    match run_result {
        Ok((code, execution_id)) => ReplicaOutcome {
            index: config.replica_index,
            status_code: code,
            execution_id,
            error: None,
        },
        Err(e) => ReplicaOutcome {
            index: config.replica_index,
            status_code: 1,
            execution_id: None,
            error: Some(e),
        },
    }
}

fn convergence_votes_json(
    converge_mode: &str,
    outcomes: &[ReplicaOutcome],
    winner: &ReplicaOutcome,
    decision_source: &str,
    decision_reason: Option<&str>,
) -> String {
    let ok = outcomes.iter().filter(|o| o.status_code == 0).count() as u64;
    let fail = outcomes.len() as u64 - ok;
    let candidates = outcomes
        .iter()
        .map(|o| {
            serde_json::json!({
                "index": o.index,
                "status_code": o.status_code,
                "score": score_outcome(o),
                "score_components": {
                    "success_score": score_outcome_breakdown(o).1,
                    "execution_id_bonus": score_outcome_breakdown(o).2,
                    "error_penalty": score_outcome_breakdown(o).3
                },
                "execution_id": o.execution_id,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    serde_json::json!({
        "mode": converge_mode,
        "winner": winner.index,
        "decision_source": decision_source,
        "decision_reason": decision_reason,
        "ok": ok,
        "fail": fail,
        "replicas_executed": outcomes.len() as u64,
        "replicas_target": outcomes.iter().map(|o| o.index).max().unwrap_or(0) as u64,
        "candidates": candidates,
    })
    .to_string()
}

fn log_convergence_summary(
    task: &TaskRecord,
    converge_mode: &str,
    outcomes: &[ReplicaOutcome],
    winner: &ReplicaOutcome,
    decision_source: &str,
    decision_reason: Option<&str>,
) {
    let votes_json = convergence_votes_json(
        converge_mode,
        outcomes,
        winner,
        decision_source,
        decision_reason,
    );
    let prev_votes = env::var("CX_TASK_CONVERGE_VOTES").ok();
    set_optional_env("CX_TASK_CONVERGE_WINNER", Some(winner.index.to_string()));
    set_optional_env("CX_TASK_CONVERGE_VOTES", Some(votes_json));
    let usage = crate::types::UsageStats::default();
    let capture = crate::types::CaptureStats::default();
    let _ = log_primary_run(RunLogInput {
        tool: "cxtask_converge",
        prompt: &task.objective,
        prompt_raw: None,
        prompt_filtered: None,
        schema_prompt: None,
        schema_raw: None,
        schema_attempt: None,
        timed_out: None,
        timeout_secs: None,
        command_label: Some("task_converge"),
        duration_ms: 0,
        usage: Some(&usage),
        capture: Some(&capture),
        schema_ok: true,
        schema_reason: None,
        schema_name: None,
        quarantine_id: None,
        policy_blocked: None,
        policy_reason: None,
    });
    set_optional_env("CX_TASK_CONVERGE_VOTES", prev_votes);
}

fn set_runtime_task_state(runner: &TaskRunner, id: &str, parent_id: Option<&String>) {
    let _ = (runner.set_state_path)("runtime.current_task_id", Value::String(id.to_string()));
    let _ = (runner.set_state_path)(
        "runtime.current_task_parent_id",
        match parent_id {
            Some(v) => Value::String(v.clone()),
            None => Value::Null,
        },
    );
}

fn restore_runtime_task_state(
    runner: &TaskRunner,
    prev_task_id: Option<String>,
    prev_parent_id: Option<String>,
) {
    let _ = (runner.set_state_path)(
        "runtime.current_task_id",
        prev_task_id.map_or(Value::Null, Value::String),
    );
    let _ = (runner.set_state_path)(
        "runtime.current_task_parent_id",
        prev_parent_id.map_or(Value::Null, Value::String),
    );
}

fn finalize_task_status(
    runner: &TaskRunner,
    id: &str,
    status_code: i32,
) -> Result<(), TaskRunError> {
    let mut tasks = (runner.read_tasks)().map_err(TaskRunError::Critical)?;
    let idx = tasks.iter().position(|t| t.id == id).ok_or_else(|| {
        TaskRunError::Critical(format!(
            "{} task run: task disappeared: {id}",
            cli_app_name()
        ))
    })?;
    tasks[idx].status = if status_code == 0 {
        "complete".to_string()
    } else {
        "failed".to_string()
    };
    tasks[idx].updated_at = (runner.utc_now_iso)();
    (runner.write_tasks)(&tasks).map_err(TaskRunError::Critical)?;
    if (runner.current_task_id)().as_deref() == Some(id) {
        let _ = (runner.set_state_path)("runtime.current_task_id", Value::Null);
    }
    Ok(())
}

pub fn run_task_by_id(
    runner: &TaskRunner,
    id: &str,
    mode_override: Option<&str>,
    backend_override: Option<&str>,
    managed_by_parent: bool,
    emit_output: bool,
) -> Result<(i32, Option<String>), TaskRunError> {
    let mut tasks = (runner.read_tasks)().map_err(TaskRunError::Critical)?;
    let idx = tasks.iter().position(|t| t.id == id).ok_or_else(|| {
        TaskRunError::Critical(format!("{} task run: task not found: {id}", cli_app_name()))
    })?;
    if tasks[idx].status == "complete" {
        return Ok((0, None));
    }
    if !managed_by_parent {
        tasks[idx].status = "in_progress".to_string();
        tasks[idx].updated_at = (runner.utc_now_iso)();
        (runner.write_tasks)(&tasks).map_err(TaskRunError::Critical)?;
    }
    let prev_task_id = if managed_by_parent {
        None
    } else {
        (runner.current_task_id)()
    };
    let prev_parent_id = if managed_by_parent {
        None
    } else {
        (runner.current_task_parent_id)()
    };
    let prev_replica_index = env::var("CX_TASK_REPLICA_INDEX").ok();
    let prev_replica_count = env::var("CX_TASK_REPLICA_COUNT").ok();
    let prev_converge_mode = env::var("CX_TASK_CONVERGE_MODE").ok();
    let prev_converge_winner = env::var("CX_TASK_CONVERGE_WINNER").ok();
    if !managed_by_parent {
        set_runtime_task_state(runner, id, tasks[idx].parent_id.as_ref());
    }

    let effective_mode = mode_override
        .map(ToOwned::to_owned)
        .or_else(|| task_mode_override(&tasks[idx]));
    let effective_backend = backend_override
        .map(ToOwned::to_owned)
        .or_else(|| task_backend_override(&tasks[idx]));
    let converge_mode = normalize_converge_mode(&tasks[idx].converge);
    let replica_count = effective_replica_count(&tasks[idx], &converge_mode);
    if tasks[idx].converge == "none" && tasks[idx].replicas > 1 {
        crate::cx_eprintln!(
            "{} task run: task {} replicas={} ignored because converge=none",
            cli_app_name(),
            id,
            tasks[idx].replicas
        );
    }
    let mut outcomes: Vec<ReplicaOutcome> = Vec::new();
    for replica_index in 1..=replica_count {
        let outcome = run_replica(
            runner,
            &tasks[idx],
            ReplicaRunConfig {
                mode_override: effective_mode.as_deref(),
                backend_override: effective_backend.as_deref(),
                emit_output,
                replica_index,
                replica_count,
                converge_mode: &converge_mode,
            },
        );
        let should_stop = converge_mode == "first_valid" && outcome.status_code == 0;
        outcomes.push(outcome);
        if should_stop {
            break;
        }
    }
    let judge_pick = if converge_mode == "judge" {
        judge_winner_with_model(
            runner,
            &tasks[idx],
            &outcomes,
            effective_mode.as_deref(),
            effective_backend.as_deref(),
        )
    } else {
        None
    };
    let (winner, decision_source, decision_reason) = if let Some((winner_idx, reason)) = judge_pick
    {
        let selected = outcomes
            .iter()
            .find(|o| o.index == winner_idx)
            .cloned()
            .unwrap_or_else(|| select_winner(&converge_mode, &outcomes));
        (selected, "model_judge", Some(reason))
    } else {
        (
            select_winner(&converge_mode, &outcomes),
            "score_fallback",
            None,
        )
    };
    set_optional_env("CX_TASK_CONVERGE_WINNER", Some(winner.index.to_string()));
    if replica_count > 1 || converge_mode != "none" {
        log_convergence_summary(
            &tasks[idx],
            &converge_mode,
            &outcomes,
            &winner,
            decision_source,
            decision_reason.as_deref(),
        );
    }
    if !managed_by_parent {
        restore_runtime_task_state(runner, prev_task_id, prev_parent_id);
    }
    set_optional_env("CX_TASK_REPLICA_INDEX", prev_replica_index);
    set_optional_env("CX_TASK_REPLICA_COUNT", prev_replica_count);
    set_optional_env("CX_TASK_CONVERGE_MODE", prev_converge_mode);
    set_optional_env("CX_TASK_CONVERGE_WINNER", prev_converge_winner);

    let status_code = winner.status_code;
    let execution_id = winner.execution_id.clone();
    let objective_err = winner.error.clone();

    if !managed_by_parent {
        finalize_task_status(runner, id, status_code)?;
    }
    if let Some(e) = objective_err {
        crate::cx_eprintln!(
            "{} task run: objective failed for {id}: {e}",
            cli_app_name()
        );
    }
    Ok((status_code, execution_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(index: u32, status_code: i32) -> ReplicaOutcome {
        ReplicaOutcome {
            index,
            status_code,
            execution_id: None,
            error: None,
        }
    }

    #[test]
    fn winner_first_valid_picks_first_success() {
        let winner = select_winner("first_valid", &[out(1, 1), out(2, 0), out(3, 0)]);
        assert_eq!(winner.index, 2);
    }

    #[test]
    fn winner_majority_prefers_success_when_tied_or_better() {
        let winner = select_winner("majority", &[out(1, 1), out(2, 0)]);
        assert_eq!(winner.status_code, 0);
        assert_eq!(winner.index, 2);
    }

    #[test]
    fn winner_score_prefers_success() {
        let winner = select_winner("score", &[out(1, 1), out(2, 0)]);
        assert_eq!(winner.index, 2);
    }

    #[test]
    fn winner_judge_breaks_tie_by_lowest_index() {
        let winner = select_winner("judge", &[out(2, 1), out(1, 1)]);
        assert_eq!(winner.index, 1);
    }

    #[test]
    fn prompt_clips_context() {
        let t = TaskRecord {
            id: "task_001".to_string(),
            parent_id: None,
            role: "implementer".to_string(),
            objective: "Do something useful".to_string(),
            context_ref: (0..600)
                .map(|i| format!("line-{i}"))
                .collect::<Vec<String>>()
                .join("\n"),
            status: "pending".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            backend: "auto".to_string(),
            model: None,
            profile: "balanced".to_string(),
            converge: "none".to_string(),
            replicas: 1,
            max_concurrency: None,
            run_mode: "sequential".to_string(),
            depends_on: Vec::new(),
            resource_keys: Vec::new(),
            max_retries: None,
            timeout_secs: None,
        };
        let prompt = task_prompt(&t);
        assert!(prompt.contains("Task Objective:"));
        assert!(prompt.contains("Context Ref:"));
        assert!(prompt.len() < t.context_ref.len() + 200);
    }
}

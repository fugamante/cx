use serde_json::Value;
use std::env;
use std::sync::OnceLock;

use crate::state::{read_state_value, value_at_path};

/// Canonical application identity (used by routing/help/version surfaces).
pub const APP_NAME: &str = "cxrs";
pub const APP_DESC: &str = "Rust runtime for the cx toolchain";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Canonical runtime defaults.
pub const DEFAULT_CONTEXT_BUDGET_CHARS: usize = 12_000;
pub const DEFAULT_CONTEXT_BUDGET_LINES: usize = 300;
pub const DEFAULT_RUN_WINDOW: usize = 50;
pub const DEFAULT_OPTIMIZE_WINDOW: usize = 200;
pub const DEFAULT_QUARANTINE_LIST: usize = 20;
pub const DEFAULT_CMD_TIMEOUT_SECS: usize = 120;

/// Process-level configuration snapshot.
///
/// Loaded once at startup and reused by command modules to avoid scattered,
/// potentially inconsistent env parsing.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub budget_chars: usize,
    pub budget_lines: usize,
    pub clip_mode: String,
    pub clip_footer: bool,
    pub llm_backend: String,
    pub ollama_model: String,
    pub codex_model: String,
    pub cxbench_log: bool,
    pub cxbench_passthru: bool,
    pub cxfix_run: bool,
    pub cxfix_force: bool,
    pub cx_unsafe: bool,
    pub cx_mode: String,
    pub schema_relaxed: bool,
    pub cxlog_enabled: bool,
    pub capture_provider: String,
    pub cmd_timeout_secs: usize,
}

static APP_CONFIG: OnceLock<AppConfig> = OnceLock::new();

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .map(|v| v == 1)
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn state_pref_str(state: &Option<Value>, path: &str) -> Option<String> {
    state
        .as_ref()
        .and_then(|v| value_at_path(v, path))
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn resolve_backend(state: &Option<Value>) -> String {
    let raw = env::var("CX_LLM_BACKEND")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| state_pref_str(state, "preferences.llm_backend"))
        .unwrap_or_else(|| "codex".to_string());
    if raw.eq_ignore_ascii_case("ollama") {
        "ollama".to_string()
    } else {
        "codex".to_string()
    }
}

fn resolve_ollama_model(state: &Option<Value>) -> String {
    env::var("CX_OLLAMA_MODEL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| state_pref_str(state, "preferences.ollama_model"))
        .unwrap_or_default()
}

impl AppConfig {
    pub fn from_env() -> Self {
        let state = read_state_value();
        Self {
            budget_chars: env_usize("CX_CONTEXT_BUDGET_CHARS", DEFAULT_CONTEXT_BUDGET_CHARS),
            budget_lines: env_usize("CX_CONTEXT_BUDGET_LINES", DEFAULT_CONTEXT_BUDGET_LINES),
            clip_mode: env::var("CX_CONTEXT_CLIP_MODE").unwrap_or_else(|_| "smart".to_string()),
            clip_footer: env_bool("CX_CONTEXT_CLIP_FOOTER", true),
            llm_backend: resolve_backend(&state),
            ollama_model: resolve_ollama_model(&state),
            codex_model: env::var("CX_MODEL").unwrap_or_default(),
            cxbench_log: env_bool("CXBENCH_LOG", true),
            cxbench_passthru: env_bool("CXBENCH_PASSTHRU", false),
            cxfix_run: env_bool("CXFIX_RUN", false),
            cxfix_force: env_bool("CXFIX_FORCE", false),
            cx_unsafe: env_bool("CX_UNSAFE", false),
            cx_mode: env::var("CX_MODE").unwrap_or_else(|_| "lean".to_string()),
            schema_relaxed: env_bool("CX_SCHEMA_RELAXED", false),
            cxlog_enabled: env_bool("CXLOG_ENABLED", true),
            capture_provider: env::var("CX_CAPTURE_PROVIDER")
                .unwrap_or_else(|_| "auto".to_string()),
            cmd_timeout_secs: env_usize("CX_CMD_TIMEOUT_SECS", DEFAULT_CMD_TIMEOUT_SECS).max(1),
        }
    }
}

pub fn init_app_config() {
    let _ = app_config();
}

pub fn app_config() -> &'static AppConfig {
    APP_CONFIG.get_or_init(AppConfig::from_env)
}

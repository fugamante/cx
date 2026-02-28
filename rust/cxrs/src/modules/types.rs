use jsonschema::JSONSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

pub static SCHEMA_COMPILED_CACHE: OnceLock<Mutex<HashMap<String, Arc<JSONSchema>>>> =
    OnceLock::new();

#[derive(Debug, Deserialize, Default, Clone)]
#[allow(dead_code)]
pub struct RunEntry {
    #[serde(default)]
    pub ts: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub cached_input_tokens: Option<u64>,
    #[serde(default)]
    pub effective_input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default)]
    pub prompt_sha256: Option<String>,
    #[serde(default)]
    pub schema_prompt_sha256: Option<String>,
    #[serde(default)]
    pub schema_sha256: Option<String>,
    #[serde(default)]
    pub schema_attempt: Option<u64>,
    #[serde(default)]
    pub timed_out: Option<bool>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub command_label: Option<String>,
    #[serde(default)]
    pub prompt_preview: Option<String>,
    #[serde(default)]
    pub system_output_len_raw: Option<u64>,
    #[serde(default)]
    pub system_output_len_processed: Option<u64>,
    #[serde(default)]
    pub system_output_len_clipped: Option<u64>,
    #[serde(default)]
    pub system_output_lines_raw: Option<u64>,
    #[serde(default)]
    pub system_output_lines_processed: Option<u64>,
    #[serde(default)]
    pub system_output_lines_clipped: Option<u64>,
    #[serde(default)]
    pub clipped: Option<bool>,
    #[serde(default)]
    pub budget_chars: Option<u64>,
    #[serde(default)]
    pub budget_lines: Option<u64>,
    #[serde(default)]
    pub clip_mode: Option<String>,
    #[serde(default)]
    pub clip_footer: Option<bool>,
    #[serde(default)]
    pub rtk_used: Option<bool>,
    #[serde(default)]
    pub capture_provider: Option<String>,
    #[serde(default)]
    pub llm_backend: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,
    #[serde(default)]
    pub adapter_type: Option<String>,
    #[serde(default)]
    pub provider_transport: Option<String>,
    #[serde(default)]
    pub provider_status: Option<String>,
    #[serde(default)]
    pub worker_id: Option<String>,
    #[serde(default)]
    pub converge_mode: Option<String>,
    #[serde(default)]
    pub converge_winner: Option<String>,
    #[serde(default)]
    pub converge_votes: Option<Value>,
    #[serde(default)]
    pub queue_ms: Option<u64>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub task_parent_id: Option<String>,
    #[serde(default)]
    pub schema_enforced: Option<bool>,
    #[serde(default)]
    pub schema_valid: Option<bool>,
    #[serde(default)]
    pub policy_blocked: Option<bool>,
    #[serde(default)]
    pub policy_reason: Option<String>,
    #[serde(default)]
    pub retry_attempt: Option<u32>,
    #[serde(default)]
    pub retry_max: Option<u32>,
    #[serde(default)]
    pub retry_reason: Option<String>,
    #[serde(default)]
    pub retry_backoff_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct QuarantineRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub prompt_sha256: String,
    #[serde(default)]
    pub raw_response: String,
    #[serde(default)]
    pub raw_sha256: String,
    #[serde(default)]
    pub attempts: Vec<QuarantineAttempt>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct QuarantineAttempt {
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub prompt_sha256: String,
    #[serde(default)]
    pub raw_response: String,
    #[serde(default)]
    pub raw_sha256: String,
}

#[derive(Debug, Default, Clone)]
pub struct CaptureStats {
    pub system_output_len_raw: Option<u64>,
    pub system_output_len_processed: Option<u64>,
    pub system_output_len_clipped: Option<u64>,
    pub system_output_lines_raw: Option<u64>,
    pub system_output_lines_processed: Option<u64>,
    pub system_output_lines_clipped: Option<u64>,
    pub clipped: Option<bool>,
    pub budget_chars: Option<u64>,
    pub budget_lines: Option<u64>,
    pub clip_mode: Option<String>,
    pub clip_footer: Option<bool>,
    pub rtk_used: Option<bool>,
    pub capture_provider: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct UsageStats {
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmOutputKind {
    Plain,
    Jsonl,
    AgentText,
    SchemaJson,
}

#[derive(Debug, Clone)]
pub enum TaskInput {
    Prompt(String),
    SystemCommand(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct TaskSpec {
    pub command_name: String,
    pub input: TaskInput,
    pub output_kind: LlmOutputKind,
    pub schema: Option<LoadedSchema>,
    pub schema_task_input: Option<String>,
    pub logging_enabled: bool,
    pub capture_override: Option<CaptureStats>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub schema_valid: Option<bool>,
    pub quarantine_id: Option<String>,
    pub capture_stats: CaptureStats,
    pub execution_id: String,
    pub usage: UsageStats,
    pub system_status: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct LoadedSchema {
    pub name: String,
    pub path: PathBuf,
    pub value: Value,
    pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ExecutionLog {
    pub execution_id: String,
    pub timestamp: String,
    pub ts: String,
    pub command: String,
    pub tool: String,
    pub cwd: String,
    pub scope: String,
    pub repo_root: String,
    pub backend_used: String,
    pub llm_backend: String,
    pub llm_model: Option<String>,
    pub adapter_type: Option<String>,
    pub provider_transport: Option<String>,
    pub provider_status: Option<String>,
    pub backend_selected: Option<String>,
    pub model_selected: Option<String>,
    pub route_policy: Option<String>,
    pub route_reason: Option<String>,
    pub worker_id: Option<String>,
    pub replica_index: Option<u32>,
    pub replica_count: Option<u32>,
    pub converge_mode: Option<String>,
    pub converge_winner: Option<String>,
    pub converge_votes: Option<Value>,
    pub queue_ms: Option<u64>,
    pub capture_provider: Option<String>,
    pub execution_mode: String,
    pub duration_ms: Option<u64>,
    pub schema_enforced: bool,
    pub schema_name: Option<String>,
    pub schema_valid: bool,
    pub schema_ok: bool,
    pub schema_reason: Option<String>,
    pub quarantine_id: Option<String>,
    pub task_id: Option<String>,
    pub task_parent_id: Option<String>,
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub effective_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub system_output_len_raw: Option<u64>,
    pub system_output_len_processed: Option<u64>,
    pub system_output_len_clipped: Option<u64>,
    pub system_output_lines_raw: Option<u64>,
    pub system_output_lines_processed: Option<u64>,
    pub system_output_lines_clipped: Option<u64>,
    pub clipped: Option<bool>,
    pub budget_chars: Option<u64>,
    pub budget_lines: Option<u64>,
    pub clip_mode: Option<String>,
    pub clip_footer: Option<bool>,
    pub rtk_used: Option<bool>,
    pub prompt_sha256: Option<String>,
    pub schema_prompt_sha256: Option<String>,
    pub schema_sha256: Option<String>,
    pub schema_attempt: Option<u64>,
    pub timed_out: Option<bool>,
    pub timeout_secs: Option<u64>,
    pub command_label: Option<String>,
    pub prompt_preview: Option<String>,
    pub policy_blocked: Option<bool>,
    pub policy_reason: Option<String>,
    pub retry_attempt: Option<u32>,
    pub retry_max: Option<u32>,
    pub retry_reason: Option<String>,
    pub retry_backoff_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskRecord {
    pub id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub objective: String,
    pub context_ref: String,
    #[serde(default = "default_task_backend")]
    pub backend: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_task_profile")]
    pub profile: String,
    #[serde(default = "default_task_converge")]
    pub converge: String,
    #[serde(default = "default_task_replicas")]
    pub replicas: u32,
    #[serde(default)]
    pub max_concurrency: Option<u32>,
    #[serde(default = "default_task_run_mode")]
    pub run_mode: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub resource_keys: Vec<String>,
    #[serde(default)]
    pub max_retries: Option<u32>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

fn default_task_run_mode() -> String {
    "sequential".to_string()
}

fn default_task_backend() -> String {
    "auto".to_string()
}

fn default_task_profile() -> String {
    "balanced".to_string()
}

fn default_task_converge() -> String {
    "none".to_string()
}

fn default_task_replicas() -> u32 {
    1
}

use crate::types::{ExecutionResult, TaskSpec};

#[allow(dead_code)]
pub struct CmdCtx {
    pub app_name: &'static str,
    pub app_version: &'static str,
    pub execute_task: fn(TaskSpec) -> Result<ExecutionResult, String>,
    pub run_llm_jsonl: fn(&str) -> Result<String, String>,
}

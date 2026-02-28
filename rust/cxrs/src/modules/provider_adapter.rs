use crate::llm::{
    LlmRunError, run_codex_jsonl, run_codex_plain, run_ollama_plain, wrap_agent_text_as_jsonl,
};
use crate::runtime::{llm_backend, resolve_ollama_model_for_run};

pub trait ProviderAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError>;
    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError>;
}

pub struct CodexCliAdapter;

impl ProviderAdapter for CodexCliAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_codex_plain(prompt)
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_codex_jsonl(prompt)
    }
}

pub struct OllamaCliAdapter {
    model: String,
}

impl OllamaCliAdapter {
    fn new() -> Result<Self, LlmRunError> {
        let model = resolve_ollama_model_for_run().map_err(LlmRunError::message)?;
        Ok(Self { model })
    }
}

impl ProviderAdapter for OllamaCliAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_ollama_plain(prompt, &self.model)
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        let text = self.run_plain(prompt)?;
        wrap_agent_text_as_jsonl(&text).map_err(LlmRunError::message)
    }
}

pub fn resolve_provider_adapter() -> Result<Box<dyn ProviderAdapter>, LlmRunError> {
    if llm_backend() == "ollama" {
        return Ok(Box::new(OllamaCliAdapter::new()?));
    }
    Ok(Box::new(CodexCliAdapter))
}

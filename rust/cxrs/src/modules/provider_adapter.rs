use crate::llm::{
    LlmRunError, run_codex_jsonl, run_codex_plain, run_ollama_plain, wrap_agent_text_as_jsonl,
};
use crate::runtime::{llm_backend, resolve_ollama_model_for_run};

fn normalized_backend_name(raw: &str) -> &'static str {
    if raw.eq_ignore_ascii_case("ollama") {
        "ollama"
    } else {
        "codex"
    }
}

pub fn selected_adapter_name() -> &'static str {
    if normalized_backend_name(&llm_backend()) == "ollama" {
        "ollama-cli"
    } else {
        "codex-cli"
    }
}

fn ollama_plain_to_jsonl(text: &str) -> Result<String, LlmRunError> {
    wrap_agent_text_as_jsonl(text).map_err(LlmRunError::message)
}

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
        ollama_plain_to_jsonl(&text)
    }
}

pub fn resolve_provider_adapter() -> Result<Box<dyn ProviderAdapter>, LlmRunError> {
    if normalized_backend_name(&llm_backend()) == "ollama" {
        return Ok(Box::new(OllamaCliAdapter::new()?));
    }
    Ok(Box::new(CodexCliAdapter))
}

pub fn run_jsonl_with_current_adapter(prompt: &str) -> Result<String, LlmRunError> {
    let adapter = resolve_provider_adapter()?;
    adapter.run_jsonl(prompt)
}

#[cfg(test)]
mod tests {
    use super::{normalized_backend_name, ollama_plain_to_jsonl};
    use serde_json::Value;

    #[test]
    fn backend_normalization_defaults_to_codex() {
        assert_eq!(normalized_backend_name("codex"), "codex");
        assert_eq!(normalized_backend_name("CoDeX"), "codex");
        assert_eq!(normalized_backend_name("unknown"), "codex");
    }

    #[test]
    fn backend_normalization_accepts_ollama_case_insensitive() {
        assert_eq!(normalized_backend_name("ollama"), "ollama");
        assert_eq!(normalized_backend_name("OLLAMA"), "ollama");
    }

    #[test]
    fn ollama_plain_output_is_wrapped_as_jsonl_agent_message() {
        let raw = "line1\nline2 with \"quotes\"";
        let jsonl = ollama_plain_to_jsonl(raw).expect("wrap jsonl");
        let parsed: Value = serde_json::from_str(&jsonl).expect("parse wrapped json");
        assert_eq!(
            parsed.get("type").and_then(Value::as_str),
            Some("item.completed")
        );
        let item = parsed.get("item").expect("item");
        assert_eq!(
            item.get("type").and_then(Value::as_str),
            Some("agent_message")
        );
        assert_eq!(item.get("text").and_then(Value::as_str), Some(raw));
    }

    #[test]
    fn selected_adapter_name_follows_backend_normalization() {
        assert_eq!(normalized_backend_name("ollama"), "ollama");
        assert_eq!(normalized_backend_name("codex"), "codex");
    }
}

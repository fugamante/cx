use crate::llm::{
    LlmRunError, run_codex_jsonl, run_codex_plain, run_ollama_plain, wrap_agent_text_as_jsonl,
};
use crate::runtime::{llm_backend, resolve_ollama_model_for_run};
use std::env;

fn normalized_backend_name(raw: &str) -> &'static str {
    if raw.eq_ignore_ascii_case("ollama") {
        "ollama"
    } else {
        "codex"
    }
}

fn adapter_override() -> Option<String> {
    env::var("CX_PROVIDER_ADAPTER")
        .ok()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

pub fn selected_adapter_name() -> &'static str {
    if let Some(v) = adapter_override()
        && v == "mock"
    {
        return "mock";
    }
    if normalized_backend_name(&llm_backend()) == "ollama" {
        "ollama-cli"
    } else {
        "codex-cli"
    }
}

pub fn selected_provider_transport() -> &'static str {
    provider_transport_for_adapter(selected_adapter_name())
}

fn provider_transport_for_adapter(adapter_name: &str) -> &'static str {
    if adapter_name == "mock" {
        "mock"
    } else {
        "process"
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

pub struct MockAdapter {
    plain_response: String,
    jsonl_response: Option<String>,
    error_message: Option<String>,
}

impl MockAdapter {
    fn new_from_env() -> Self {
        let plain_response = env::var("CX_MOCK_PLAIN_RESPONSE")
            .unwrap_or_else(|_| "{\"commands\":[\"echo mock\"]}".to_string());
        let jsonl_response = env::var("CX_MOCK_JSONL_RESPONSE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let error_message = env::var("CX_MOCK_ERROR")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Self {
            plain_response,
            jsonl_response,
            error_message,
        }
    }
}

impl ProviderAdapter for MockAdapter {
    fn run_plain(&self, _prompt: &str) -> Result<String, LlmRunError> {
        if let Some(err) = &self.error_message {
            return Err(LlmRunError::message(err.clone()));
        }
        Ok(self.plain_response.clone())
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        if let Some(err) = &self.error_message {
            return Err(LlmRunError::message(err.clone()));
        }
        if let Some(jsonl) = &self.jsonl_response {
            return Ok(jsonl.clone());
        }
        let plain = self.run_plain(prompt)?;
        ollama_plain_to_jsonl(&plain)
    }
}

pub fn resolve_provider_adapter() -> Result<Box<dyn ProviderAdapter>, LlmRunError> {
    if let Some(v) = adapter_override()
        && v == "mock"
    {
        return Ok(Box::new(MockAdapter::new_from_env()));
    }
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

    #[test]
    fn provider_transport_mapping_covers_mock_and_process() {
        assert_eq!(super::provider_transport_for_adapter("mock"), "mock");
        assert_eq!(
            super::provider_transport_for_adapter("codex-cli"),
            "process"
        );
        assert_eq!(
            super::provider_transport_for_adapter("ollama-cli"),
            "process"
        );
    }
}

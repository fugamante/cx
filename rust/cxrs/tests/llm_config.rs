mod common;

use common::{TempRepo, read_json, stderr_str, stdout_str};
use serde_json::Value;

#[test]
fn llm_use_persists_backend_and_model() {
    let repo = TempRepo::new("cxrs-llm");

    let out = repo.run(&["llm", "use", "ollama", "llama3.1"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let show = repo.run(&["llm", "show"]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let text = stdout_str(&show);
    assert!(text.contains("llm_backend: ollama"), "{text}");
    assert!(text.contains("ollama_model: llama3.1"), "{text}");

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .and_then(Value::as_str),
        Some("ollama")
    );
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("ollama_model"))
            .and_then(Value::as_str),
        Some("llama3.1")
    );
}

#[test]
fn llm_unset_can_clear_model_backend_and_all() {
    let repo = TempRepo::new("cxrs-llm");

    let out = repo.run(&["llm", "use", "ollama", "llama3.1"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let unset_model = repo.run(&["llm", "unset", "model"]);
    assert!(
        unset_model.status.success(),
        "stdout={} stderr={}",
        stdout_str(&unset_model),
        stderr_str(&unset_model)
    );

    let show_after_model = repo.run(&["llm", "show"]);
    assert!(show_after_model.status.success());
    let show_text = stdout_str(&show_after_model);
    assert!(show_text.contains("llm_backend: ollama"), "{show_text}");
    assert!(show_text.contains("ollama_model: <unset>"), "{show_text}");

    let unset_backend = repo.run(&["llm", "unset", "backend"]);
    assert!(unset_backend.status.success());
    let show_after_backend = repo.run(&["llm", "show"]);
    let show_backend_text = stdout_str(&show_after_backend);
    assert!(
        show_backend_text.contains("llm_backend: codex"),
        "{show_backend_text}"
    );

    let out2 = repo.run(&["llm", "use", "ollama", "llama3.1"]);
    assert!(out2.status.success());
    let unset_all = repo.run(&["llm", "unset", "all"]);
    assert!(unset_all.status.success());

    let state = read_json(&repo.state_file());
    assert!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .is_some_and(Value::is_null)
    );
    assert!(
        state
            .get("preferences")
            .and_then(|v| v.get("ollama_model"))
            .is_some_and(Value::is_null)
    );
}

#[test]
fn ollama_without_model_fails_non_interactive_with_clear_error() {
    let repo = TempRepo::new("cxrs-llm");

    assert!(repo.run(&["llm", "unset", "all"]).status.success());
    assert!(repo.run(&["llm", "use", "ollama"]).status.success());

    let out = repo.run(&["cxo", "echo", "hi"]);
    assert!(
        !out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let err = stderr_str(&out);
    assert!(
        err.contains("ollama model is unset"),
        "expected unset-model guidance in stderr; got: {err}"
    );
}

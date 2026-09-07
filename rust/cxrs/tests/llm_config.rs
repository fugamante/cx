mod common;

use common::{TempRepo, read_json, run_fixture_http_server_once, stderr_str, stdout_str};
use serde_json::{Value, json};
use std::fs;

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
    assert!(
        stderr_str(&out).contains("quota_probe: backend=ollama service_kind=local_unmetered"),
        "stderr={}",
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
fn llm_use_codex_triggers_quota_probe_notice() {
    let repo = TempRepo::new("cxrs-llm");
    let out = repo.run(&["llm", "use", "primary"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("quota_probe: backend=primary"),
        "stderr={}",
        stderr_str(&out)
    );
}

#[test]
fn llm_unset_clears_model_backend_all() {
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
        show_backend_text.contains("llm_backend: primary"),
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
fn ollama_without_model_fails_noninteractive_clear() {
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

#[test]
fn llm_use_llamacpp_persists_model_path() {
    let repo = TempRepo::new("cxrs-llm");

    let out = repo.run(&["llm", "use", "llama.cpp", "/models/tiny.gguf"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("quota_probe: backend=llamacpp service_kind=local_unmetered"),
        "stderr={}",
        stderr_str(&out)
    );

    let show = repo.run(&["llm", "show"]);
    assert!(show.status.success());
    let text = stdout_str(&show);
    assert!(text.contains("llm_backend: llamacpp"), "{text}");
    assert!(
        text.contains("llama_cpp_model: /models/tiny.gguf"),
        "{text}"
    );

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .and_then(Value::as_str),
        Some("llamacpp")
    );
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("llama_cpp_model"))
            .and_then(Value::as_str),
        Some("/models/tiny.gguf")
    );
}

#[test]
fn llamacpp_backend_runs_llama_cli_mock() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "llama-cli",
        r#"#!/usr/bin/env bash
printf 'llama.cpp ok'
"#,
    );

    let out = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "llamacpp"),
            ("CX_LLAMA_CPP_MODEL", "/models/tiny.gguf"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(stdout_str(&out).contains("llama.cpp ok"));
}

#[test]
fn llamacpp_backend_uses_hf_repo_quant() {
    let repo = TempRepo::new("cxrs-llm");
    let args_file = repo.root.join("llama_args.txt");
    repo.write_mock(
        "llama-cli",
        r#"#!/usr/bin/env bash
printf '%s\n' "$@" > "$LLAMA_ARGS_FILE"
printf 'llama.cpp hf ok'
"#,
    );
    let args_path = args_file.display().to_string();

    let out = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "llamacpp"),
            ("CX_LLAMA_CPP_MODEL", "ggml-org/Qwen3-0.6B-GGUF:Q4_0"),
            ("CX_LLAMA_CPP_ARGS", "-n 8 --temp 0"),
            ("LLAMA_ARGS_FILE", &args_path),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(stdout_str(&out).contains("llama.cpp hf ok"));
    let args = fs::read_to_string(&args_file).expect("read llama-cli args");
    assert!(args.contains("-hf\n"), "{args}");
    assert!(args.contains("ggml-org/Qwen3-0.6B-GGUF:Q4_0\n"), "{args}");
    assert!(!args.contains("-m\n"), "{args}");
    assert!(args.contains("-n\n8\n"), "{args}");
    assert!(args.contains("--temp\n0\n"), "{args}");
}

#[test]
fn llm_use_mlx_persists_model_name() {
    let repo = TempRepo::new("cxrs-llm");

    let out = repo.run(&["llm", "use", "mlx", "mlx-community/Tiny"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("quota_probe: backend=mlx service_kind=local_unmetered"),
        "stderr={}",
        stderr_str(&out)
    );

    let show = repo.run(&["llm", "show"]);
    assert!(show.status.success());
    let text = stdout_str(&show);
    assert!(text.contains("llm_backend: mlx"), "{text}");
    assert!(text.contains("mlx_model: mlx-community/Tiny"), "{text}");

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .and_then(Value::as_str),
        Some("mlx")
    );
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("mlx_model"))
            .and_then(Value::as_str),
        Some("mlx-community/Tiny")
    );
}

#[test]
fn llm_use_mlx_alias_persists_resolved_model() {
    let repo = TempRepo::new("cxrs-llm");
    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny",
    ]);
    assert!(
        add.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add),
        stderr_str(&add)
    );

    let out = repo.run(&["llm", "use", "mlx", "tiny"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let text = stdout_str(&out);
    assert!(text.contains("mlx_model: mlx-community/Tiny"), "{text}");

    let show = repo.run(&["llm", "show"]);
    assert!(show.status.success(), "stderr={}", stderr_str(&show));
    let show_text = stdout_str(&show);
    assert!(
        show_text.contains("mlx_model: mlx-community/Tiny"),
        "{show_text}"
    );
    assert!(show_text.contains("mlx_model_alias: tiny"), "{show_text}");
    assert!(
        show_text.contains("mlx_model_resolved_model: mlx-community/Tiny"),
        "{show_text}"
    );

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("mlx_model"))
            .and_then(Value::as_str),
        Some("mlx-community/Tiny")
    );
}

#[test]
fn models_crud() {
    let repo = TempRepo::new("cxrs-llm");

    let empty = repo.run(&["llm", "models", "list"]);
    assert!(
        empty.status.success(),
        "stdout={} stderr={}",
        stdout_str(&empty),
        stderr_str(&empty)
    );
    assert!(stdout_str(&empty).contains("local_models: <empty>"));

    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "qwen-coder",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Qwen-Coder-4bit",
        "--provider",
        "huggingface",
        "--repo-id",
        "mlx-community/Qwen-Coder-4bit",
        "--quantization",
        "4bit",
        "--format",
        "mlx",
        "--size-bytes",
        "1234",
        "--preferred-args",
        "--temp 0 --top-p 0.9",
        "--trust-remote-code",
        "false",
    ]);
    assert!(
        add.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add),
        stderr_str(&add)
    );
    let add_text = stdout_str(&add);
    assert!(add_text.contains("model_alias: qwen-coder"), "{add_text}");
    assert!(add_text.contains("model_backend: mlx"), "{add_text}");
    assert!(add_text.contains("model_id: mlx:qwen-coder"), "{add_text}");

    let list = repo.run(&["llm", "models", "list"]);
    assert!(list.status.success());
    let list_text = stdout_str(&list);
    assert!(
        list_text.contains(
            "- alias=qwen-coder backend=mlx id=mlx:qwen-coder model=mlx-community/Qwen-Coder-4bit"
        ),
        "{list_text}"
    );

    let inspect = repo.run(&["llm", "models", "inspect", "qwen-coder", "--json"]);
    assert!(
        inspect.status.success(),
        "stdout={} stderr={}",
        stdout_str(&inspect),
        stderr_str(&inspect)
    );
    let inspected = serde_json::from_str::<Value>(&stdout_str(&inspect)).expect("inspect JSON");
    assert_eq!(
        inspected.get("alias").and_then(Value::as_str),
        Some("qwen-coder")
    );
    assert_eq!(
        inspected.get("backend").and_then(Value::as_str),
        Some("mlx")
    );
    assert_eq!(
        inspected.get("resolved_model").and_then(Value::as_str),
        Some("mlx-community/Qwen-Coder-4bit")
    );
    assert_eq!(
        inspected.get("size_bytes").and_then(Value::as_u64),
        Some(1234)
    );
    assert_eq!(
        inspected.get("preferred_args").and_then(Value::as_str),
        Some("--temp 0 --top-p 0.9")
    );
    assert_eq!(
        inspected.get("trust_remote_code").and_then(Value::as_str),
        Some("false")
    );

    let inspect_text = repo.run(&["llm", "models", "inspect", "qwen-coder"]);
    assert!(
        inspect_text.status.success(),
        "stdout={} stderr={}",
        stdout_str(&inspect_text),
        stderr_str(&inspect_text)
    );
    assert!(
        stdout_str(&inspect_text).contains("preferred_args: --temp 0 --top-p 0.9"),
        "{}",
        stdout_str(&inspect_text)
    );

    let stored = read_json(&repo.local_models_file());
    assert_eq!(
        stored.get("contract_version").and_then(Value::as_str),
        Some("local_models.v1")
    );

    let remove = repo.run(&["llm", "models", "remove", "mlx:qwen-coder"]);
    assert!(
        remove.status.success(),
        "stdout={} stderr={}",
        stdout_str(&remove),
        stderr_str(&remove)
    );
    assert!(stdout_str(&remove).contains("removed_model_alias: qwen-coder"));

    let after = repo.run(&["llm", "models", "list"]);
    assert!(stdout_str(&after).contains("local_models: <empty>"));
}

#[test]
fn models_replace() {
    let repo = TempRepo::new("cxrs-llm");

    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny",
    ]);
    assert!(add.status.success());

    let duplicate = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny-v2",
    ]);
    assert!(!duplicate.status.success());
    assert!(
        stderr_str(&duplicate).contains("already exists"),
        "{}",
        stderr_str(&duplicate)
    );

    let replace = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny-v2",
        "--replace",
    ]);
    assert!(
        replace.status.success(),
        "stdout={} stderr={}",
        stdout_str(&replace),
        stderr_str(&replace)
    );

    let inspect = repo.run(&["llm", "models", "inspect", "tiny", "--json"]);
    let inspected = serde_json::from_str::<Value>(&stdout_str(&inspect)).expect("inspect JSON");
    assert_eq!(
        inspected.get("resolved_model").and_then(Value::as_str),
        Some("mlx-community/Tiny-v2")
    );
}

#[test]
fn registry_id_collision() {
    let repo = TempRepo::new("cxrs-llm");

    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny",
        "--id",
        "shared-id",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));

    let collision = repo.run(&[
        "llm",
        "models",
        "add",
        "other",
        "--backend",
        "llamacpp",
        "--model",
        "/models/other.gguf",
        "--id",
        "shared-id",
        "--replace",
    ]);
    assert!(
        !collision.status.success(),
        "stdout={}",
        stdout_str(&collision)
    );
    assert!(
        stderr_str(&collision)
            .contains("local model id 'shared-id' already belongs to another backend or alias"),
        "{}",
        stderr_str(&collision)
    );

    let inspect = repo.run(&["llm", "models", "inspect", "shared-id", "--json"]);
    assert!(
        inspect.status.success(),
        "stdout={} stderr={}",
        stdout_str(&inspect),
        stderr_str(&inspect)
    );
    let inspected = serde_json::from_str::<Value>(&stdout_str(&inspect)).expect("inspect JSON");
    assert_eq!(
        inspected.get("backend").and_then(Value::as_str),
        Some("mlx")
    );
    assert_eq!(inspected.get("alias").and_then(Value::as_str), Some("tiny"));
}

#[test]
fn registry_duplicate_ids() {
    let repo = TempRepo::new("cxrs-llm");
    let model = |id: &str, alias: &str, backend: &str| {
        json!({
            "id": id,
            "alias": alias,
            "backend": backend,
            "resolved_model": format!("{backend}/{alias}")
        })
    };

    fs::write(
        repo.local_models_file(),
        serde_json::to_vec_pretty(&json!({
            "contract_version": "local_models.v1",
            "models": [
                model("shared-id", "tiny", "mlx"),
                model("shared-id", "other", "llamacpp")
            ]
        }))
        .expect("serialize duplicate id registry"),
    )
    .expect("write duplicate id registry");
    let duplicate_id = repo.run(&["llm", "models", "list"]);
    assert!(!duplicate_id.status.success());
    assert!(
        stderr_str(&duplicate_id)
            .contains("local model registry contains duplicate id 'shared-id'"),
        "{}",
        stderr_str(&duplicate_id)
    );

    fs::write(
        repo.local_models_file(),
        serde_json::to_vec_pretty(&json!({
            "contract_version": "local_models.v1",
            "models": [
                model("mlx:tiny-a", "tiny", "mlx"),
                model("mlx:tiny-b", "tiny", "mlx")
            ]
        }))
        .expect("serialize duplicate alias registry"),
    )
    .expect("write duplicate alias registry");
    let duplicate_alias = repo.run(&["llm", "models", "inspect", "tiny"]);
    assert!(!duplicate_alias.status.success());
    assert!(
        stderr_str(&duplicate_alias)
            .contains("local model registry contains duplicate backend alias 'mlx:tiny'"),
        "{}",
        stderr_str(&duplicate_alias)
    );
}

#[test]
fn registry_contract_validation() {
    let repo = TempRepo::new("cxrs-llm");
    let valid_model = || {
        json!({
            "id": "mlx:tiny",
            "alias": "tiny",
            "backend": "mlx",
            "resolved_model": "mlx-community/Tiny"
        })
    };
    let cases = [
        (json!([]), "local model registry root must be an object"),
        (
            json!({
                "contract_version": "local_models.v2",
                "models": []
            }),
            "local model registry contract_version must be 'local_models.v1'",
        ),
        (
            json!({
                "contract_version": "local_models.v1",
                "models": {}
            }),
            "local model registry 'models' must be an array",
        ),
        (
            json!({
                "contract_version": "local_models.v1",
                "models": [{
                    "id": "bad:tiny",
                    "alias": "tiny",
                    "backend": "bad",
                    "resolved_model": "bad/Tiny"
                }]
            }),
            "local model record has invalid backend 'bad'",
        ),
        (
            json!({
                "contract_version": "local_models.v1",
                "models": [{
                    "id": "mlx:tiny",
                    "alias": "tiny",
                    "backend": "MLX",
                    "resolved_model": "mlx-community/Tiny"
                }]
            }),
            "local model record backend 'MLX' must use canonical value 'mlx'",
        ),
        (
            json!({
                "contract_version": "local_models.v1",
                "models": [{
                    "trust_remote_code": true,
                    "id": "mlx:tiny",
                    "alias": "tiny",
                    "backend": "mlx",
                    "resolved_model": "mlx-community/Tiny"
                }]
            }),
            "local model record has invalid trust_remote_code value",
        ),
        (
            json!({
                "contract_version": "local_models.v1",
                "models": [{
                    "trust_remote_code": "sometimes",
                    "id": "mlx:tiny",
                    "alias": "tiny",
                    "backend": "mlx",
                    "resolved_model": "mlx-community/Tiny"
                }]
            }),
            "local model record has invalid trust_remote_code 'sometimes'",
        ),
        (
            json!({
                "contract_version": "local_models.v1",
                "models": [{
                    "size_bytes": "large",
                    "id": "mlx:tiny",
                    "alias": "tiny",
                    "backend": "mlx",
                    "resolved_model": "mlx-community/Tiny"
                }]
            }),
            "local model record has invalid 'size_bytes'",
        ),
    ];

    for (registry, expected) in cases {
        fs::write(
            repo.local_models_file(),
            serde_json::to_vec_pretty(&registry).expect("serialize invalid registry"),
        )
        .expect("write invalid registry");
        let out = repo.run(&["llm", "models", "list"]);
        assert!(!out.status.success(), "stdout={}", stdout_str(&out));
        assert!(
            stderr_str(&out).contains(expected),
            "expected={expected} stderr={}",
            stderr_str(&out)
        );
    }

    fs::write(
        repo.local_models_file(),
        serde_json::to_vec_pretty(&json!({
            "models": [valid_model()]
        }))
        .expect("serialize legacy registry"),
    )
    .expect("write legacy registry");
    let legacy = repo.run(&["llm", "models", "list", "--json"]);
    assert!(
        legacy.status.success(),
        "stdout={} stderr={}",
        stdout_str(&legacy),
        stderr_str(&legacy)
    );
    let payload =
        serde_json::from_str::<Value>(&stdout_str(&legacy)).expect("legacy registry JSON");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("local_models.v1")
    );
}

#[test]
fn models_inspect_reports_accounting_paths() {
    let repo = TempRepo::new("cxrs-llm");
    let missing_path = repo.root.join("does-not-exist");
    let model_dir = repo.root.join("model-dir");
    let cache_dir = repo.root.join("cache-dir");
    fs::create_dir_all(&model_dir).expect("create model dir");
    fs::create_dir_all(&cache_dir).expect("create cache dir");
    fs::write(model_dir.join("model.bin"), b"abc").expect("write model file");
    fs::write(cache_dir.join("cache.bin"), b"12345").expect("write cache file");

    let missing_path_s = missing_path.display().to_string();
    let model_dir_s = model_dir.display().to_string();
    let cache_dir_s = cache_dir.display().to_string();

    let add_missing = repo.run(&[
        "llm",
        "models",
        "add",
        "missing-local",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/MissingLocal",
        "--local-path",
        &missing_path_s,
    ]);
    assert!(
        add_missing.status.success(),
        "stderr={}",
        stderr_str(&add_missing)
    );

    let inspect_missing = repo.run(&["llm", "models", "inspect", "missing-local", "--json"]);
    assert!(
        inspect_missing.status.success(),
        "stdout={} stderr={}",
        stdout_str(&inspect_missing),
        stderr_str(&inspect_missing)
    );
    let missing_json =
        serde_json::from_str::<Value>(&stdout_str(&inspect_missing)).expect("inspect missing");
    assert_eq!(
        missing_json
            .get("inspect")
            .and_then(|v| v.get("resolved_model_status"))
            .and_then(Value::as_str),
        Some("missing_local_path")
    );
    assert_eq!(
        missing_json
            .get("inspect")
            .and_then(|v| v.get("local_path"))
            .and_then(|v| v.get("status"))
            .and_then(Value::as_str),
        Some("missing")
    );

    let add_resolved = repo.run(&[
        "llm",
        "models",
        "add",
        "with-local-dir",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/LocalDir",
        "--local-path",
        &model_dir_s,
        "--cache-path",
        &cache_dir_s,
    ]);
    assert!(
        add_resolved.status.success(),
        "stderr={}",
        stderr_str(&add_resolved)
    );

    let inspect_cheap = repo.run(&["llm", "models", "inspect", "with-local-dir", "--json"]);
    assert!(
        inspect_cheap.status.success(),
        "stderr={}",
        stderr_str(&inspect_cheap)
    );
    let cheap_json =
        serde_json::from_str::<Value>(&stdout_str(&inspect_cheap)).expect("inspect cheap");
    assert_eq!(
        cheap_json
            .get("inspect")
            .and_then(|v| v.get("accounting_mode"))
            .and_then(Value::as_str),
        Some("cheap")
    );
    assert_eq!(
        cheap_json
            .get("inspect")
            .and_then(|v| v.get("local_path"))
            .and_then(|v| v.get("path_kind"))
            .and_then(Value::as_str),
        Some("dir")
    );
    assert!(
        cheap_json
            .get("inspect")
            .and_then(|v| v.get("local_path"))
            .and_then(|v| v.get("size_bytes"))
            .is_some_and(Value::is_null)
    );

    let inspect_disk = repo.run(&[
        "llm",
        "models",
        "inspect",
        "with-local-dir",
        "--disk-usage",
        "--json",
    ]);
    assert!(
        inspect_disk.status.success(),
        "stderr={}",
        stderr_str(&inspect_disk)
    );
    let disk_json =
        serde_json::from_str::<Value>(&stdout_str(&inspect_disk)).expect("inspect disk");
    assert_eq!(
        disk_json
            .get("inspect")
            .and_then(|v| v.get("accounting_mode"))
            .and_then(Value::as_str),
        Some("disk_usage")
    );
    assert_eq!(
        disk_json
            .get("inspect")
            .and_then(|v| v.get("local_path"))
            .and_then(|v| v.get("size_bytes"))
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        disk_json
            .get("inspect")
            .and_then(|v| v.get("cache_path"))
            .and_then(|v| v.get("size_bytes"))
            .and_then(Value::as_u64),
        Some(5)
    );
}

#[test]
fn llm_verify_mlx_smoke_resolves_registry_alias() {
    let repo = TempRepo::new("cxrs-llm");
    let args_file = repo.root.join("mlx_verify_args.txt");
    let args_path = args_file.display().to_string();
    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny-Resolved",
        "--preferred-args",
        "--temp 0 --top-p 0.8",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));

    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
printf '%s\n' "$@" > "$MLX_VERIFY_ARGS_FILE"
printf 'OK'
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();
    let out = repo.run_with_env(
        &["llm", "verify", "mlx", "--json"],
        &[
            ("CX_MLX_MODEL", "tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
            ("MLX_VERIFY_ARGS_FILE", &args_path),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v = serde_json::from_str::<Value>(&stdout_str(&out)).expect("verify json");
    assert_eq!(
        v.get("contract_version").and_then(Value::as_str),
        Some("llm-verify.v1")
    );
    assert_eq!(v.get("backend").and_then(Value::as_str), Some("mlx"));
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("model"))
            .and_then(|v| v.get("input"))
            .and_then(Value::as_str),
        Some("tiny")
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("model"))
            .and_then(|v| v.get("resolved"))
            .and_then(Value::as_str),
        Some("mlx-community/Tiny-Resolved")
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("model"))
            .and_then(|v| v.get("preferred_args"))
            .and_then(Value::as_str),
        Some("--temp 0 --top-p 0.8")
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("correctness"))
            .and_then(|v| v.get("exact"))
            .and_then(Value::as_bool),
        Some(true)
    );
    let verify_args = fs::read_to_string(&args_file).expect("read verify args");
    assert!(
        verify_args.contains("--model\nmlx-community/Tiny-Resolved\n"),
        "{verify_args}"
    );
    assert!(verify_args.contains("--temp\n0\n"), "{verify_args}");
    assert!(verify_args.contains("--top-p\n0.8\n"), "{verify_args}");

    let inspect = repo.run(&["llm", "models", "inspect", "tiny", "--json"]);
    assert!(
        inspect.status.success(),
        "stdout={} stderr={}",
        stdout_str(&inspect),
        stderr_str(&inspect)
    );
    let inspected = serde_json::from_str::<Value>(&stdout_str(&inspect)).expect("inspect json");
    assert_eq!(
        inspected.get("last_smoke_status").and_then(Value::as_str),
        Some("pass")
    );
    assert!(
        inspected
            .get("last_used_at")
            .and_then(Value::as_str)
            .is_some(),
        "{inspected}"
    );
}

#[test]
fn llm_verify_mlx_benchmark_emits_typed_metrics() {
    let repo = TempRepo::new("cxrs-llm");
    let args_file = repo.root.join("mlx_benchmark_args.txt");
    let args_path = args_file.display().to_string();
    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "bench",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/BenchResolved",
        "--preferred-args",
        "--temp 0.4 --top-p 0.85",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));

    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
set -euo pipefail
OUT=""
PREV=""
printf '%s\n' "$@" > "$MLX_BENCH_ARGS_FILE"
for ARG in "$@"; do
  if [ "$PREV" = "--out" ]; then
    OUT="$ARG"
    break
  fi
  PREV="$ARG"
done
cat > "$OUT" <<'JSON'
{
  "contract_version": "turboquant-mlx.v2",
  "backend": "mlx",
  "model": "mlx-community/BenchResolved",
  "context_target": 8192,
  "runtime_config": {
    "temperature": 0.1,
    "top_p": 0.85,
    "min_p": 0.0,
    "top_k": 0,
    "seed": 7,
    "ignored_args": []
  },
  "runs": [
    {
      "prompt_name": "smoke",
      "passed": true,
      "prompt_tokens_per_sec": 100.0,
      "decode_tokens_per_sec": 50.0,
      "peak_memory_gb": 1.2,
      "cache_nbytes": 1000,
      "wall_ms": 200
    },
    {
      "prompt_name": "retrieval",
      "passed": true,
      "prompt_tokens_per_sec": 120.0,
      "decode_tokens_per_sec": 70.0,
      "peak_memory_gb": 1.5,
      "cache_nbytes": 1200,
      "wall_ms": 300
    }
  ],
  "passes": 2,
  "total": 2
}
JSON
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();
    let out = repo.run_with_env(
        &["llm", "verify", "mlx", "--profile", "benchmark", "--json"],
        &[
            ("CX_MLX_MODEL", "bench"),
            ("CX_MLX_PYTHON", &mlx_python),
            ("CX_MLX_ARGS", "--temp 0.1 --seed 7"),
            ("MLX_BENCH_ARGS_FILE", &args_path),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v = serde_json::from_str::<Value>(&stdout_str(&out)).expect("verify json");
    assert_eq!(v.get("profile").and_then(Value::as_str), Some("benchmark"));
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("memory"))
            .and_then(|v| v.get("cache_metric_kind"))
            .and_then(Value::as_str),
        Some("cache_nbytes")
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("memory"))
            .and_then(|v| v.get("cache_metric_unit"))
            .and_then(Value::as_str),
        Some("bytes")
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("correctness"))
            .and_then(|v| v.get("exact"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("runtime"))
            .and_then(|v| v.get("prompt_tps_mean"))
            .and_then(Value::as_f64),
        Some(110.0)
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("memory"))
            .and_then(|v| v.get("peak_memory_gb_max"))
            .and_then(Value::as_f64),
        Some(1.5)
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("raw_probe"))
            .and_then(|v| v.get("runtime_config"))
            .and_then(|v| v.get("temperature"))
            .and_then(Value::as_f64),
        Some(0.1)
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("raw_probe"))
            .and_then(|v| v.get("runtime_config"))
            .and_then(|v| v.get("top_p"))
            .and_then(Value::as_f64),
        Some(0.85)
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("raw_probe"))
            .and_then(|v| v.get("runtime_config"))
            .and_then(|v| v.get("seed"))
            .and_then(Value::as_i64),
        Some(7)
    );
    let bench_args = fs::read_to_string(&args_file).expect("read benchmark args");
    assert!(
        bench_args.contains("--preferred-args\n--temp 0.4 --top-p 0.85\n"),
        "{bench_args}"
    );
    assert!(
        bench_args.contains("--mlx-args\n--temp 0.1 --seed 7\n"),
        "{bench_args}"
    );
}

#[test]
fn llm_use_mlx_alias_shows_resolved_model() {
    let repo = TempRepo::new("cxrs-llm");
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "tiny",
            "--backend",
            "mlx",
            "--model",
            "mlx-community/Tiny-Resolved",
            "--preferred-args",
            "--temp 0.7 --top-p 0.9",
        ])
        .status
        .success()
    );

    let use_out = repo.run(&["llm", "use", "mlx", "tiny"]);
    assert!(
        use_out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&use_out),
        stderr_str(&use_out)
    );

    let show = repo.run(&["llm", "show"]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let text = stdout_str(&show);
    assert!(text.contains("llm_backend: mlx"), "{text}");
    assert!(
        text.contains("mlx_model: mlx-community/Tiny-Resolved"),
        "{text}"
    );
    assert!(text.contains("mlx_model_alias: tiny"), "{text}");
    assert!(
        text.contains("mlx_model_resolved_model: mlx-community/Tiny-Resolved"),
        "{text}"
    );
    assert!(
        text.contains("active_model_resolved_model: mlx-community/Tiny-Resolved"),
        "{text}"
    );

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("mlx_model"))
            .and_then(Value::as_str),
        Some("mlx-community/Tiny-Resolved")
    );
}

#[test]
fn backend_alias_collision() {
    let repo = TempRepo::new("cxrs-llm");
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "shared",
            "--backend",
            "llamacpp",
            "--model",
            "/models/shared.gguf",
        ])
        .status
        .success()
    );
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "shared",
            "--backend",
            "mlx",
            "--model",
            "mlx-community/Shared-Resolved",
        ])
        .status
        .success()
    );

    let use_out = repo.run(&["llm", "use", "mlx", "shared"]);
    assert!(
        use_out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&use_out),
        stderr_str(&use_out)
    );
    let text = stdout_str(&use_out);
    assert!(
        text.contains("mlx_model: mlx-community/Shared-Resolved"),
        "{text}"
    );
    let show = repo.run(&["llm", "show"]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    assert!(stdout_str(&show).contains("mlx_model_alias: shared"));
}

#[test]
fn inspect_ambiguous_alias() {
    let repo = TempRepo::new("cxrs-llm");
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "shared",
            "--backend",
            "llamacpp",
            "--model",
            "/models/shared.gguf",
        ])
        .status
        .success()
    );
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "shared",
            "--backend",
            "mlx",
            "--model",
            "mlx-community/Shared-Resolved",
        ])
        .status
        .success()
    );

    let inspect = repo.run(&["llm", "models", "inspect", "shared"]);
    assert!(!inspect.status.success(), "stdout={}", stdout_str(&inspect));
    assert!(
        stderr_str(&inspect).contains("local model selector 'shared' is ambiguous"),
        "{}",
        stderr_str(&inspect)
    );
    assert!(
        stderr_str(&inspect).contains("llamacpp:shared"),
        "{}",
        stderr_str(&inspect)
    );
    assert!(
        stderr_str(&inspect).contains("mlx:shared"),
        "{}",
        stderr_str(&inspect)
    );
}

#[test]
fn remove_ambiguous_alias() {
    let repo = TempRepo::new("cxrs-llm");
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "shared",
            "--backend",
            "llamacpp",
            "--model",
            "/models/shared.gguf",
        ])
        .status
        .success()
    );
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "shared",
            "--backend",
            "mlx",
            "--model",
            "mlx-community/Shared-Resolved",
        ])
        .status
        .success()
    );

    let remove = repo.run(&["llm", "models", "remove", "shared"]);
    assert!(!remove.status.success(), "stdout={}", stdout_str(&remove));
    assert!(
        stderr_str(&remove).contains("local model selector 'shared' is ambiguous"),
        "{}",
        stderr_str(&remove)
    );
}

#[test]
fn mlx_alias_exec_resolution_env_override() {
    let repo = TempRepo::new("cxrs-llm");
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "tiny",
            "--backend",
            "mlx",
            "--model",
            "mlx-community/Tiny-Resolved",
            "--preferred-args",
            "--temp 0.7 --top-p 0.9",
        ])
        .status
        .success()
    );
    assert!(repo.run(&["llm", "use", "mlx", "tiny"]).status.success());

    let alias_args_file = repo.root.join("mlx_alias_args.txt");
    let alias_args_path = alias_args_file.display().to_string();
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
printf '%s\n' "$@" > "$MLX_ARGS_FILE"
printf 'mlx ok'
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();

    let alias_run = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_MODEL", "tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
            ("CX_MLX_ARGS", "--temp 0.1 --seed 9"),
            ("MLX_ARGS_FILE", &alias_args_path),
        ],
    );
    assert!(
        alias_run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&alias_run),
        stderr_str(&alias_run)
    );
    let alias_args = fs::read_to_string(&alias_args_file).expect("read mlx alias args");
    assert!(
        alias_args.contains("--model\nmlx-community/Tiny-Resolved\n"),
        "{alias_args}"
    );
    assert!(alias_args.contains("--top-p\n0.9\n"), "{alias_args}");
    assert!(alias_args.contains("--seed\n9\n"), "{alias_args}");
    let pref_pos = alias_args.find("--temp\n0.7\n").expect("preferred temp");
    let env_pos = alias_args.find("--temp\n0.1\n").expect("env temp");
    assert!(pref_pos < env_pos, "{alias_args}");

    let direct_args_file = repo.root.join("mlx_direct_args.txt");
    let direct_args_path = direct_args_file.display().to_string();
    let direct_run = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_MODEL", "mlx-community/Tiny-Resolved"),
            ("CX_MLX_PYTHON", &mlx_python),
            ("CX_MLX_ARGS", "--temp 0.1 --seed 7"),
            ("MLX_ARGS_FILE", &direct_args_path),
        ],
    );
    assert!(
        direct_run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&direct_run),
        stderr_str(&direct_run)
    );
    let direct_args = fs::read_to_string(&direct_args_file).expect("read mlx direct args");
    assert!(
        direct_args.contains("--model\nmlx-community/Tiny-Resolved\n"),
        "{direct_args}"
    );
    assert!(!direct_args.contains("--top-p\n0.9\n"), "{direct_args}");
    assert!(!direct_args.contains("--temp\n0.7\n"), "{direct_args}");
    assert!(direct_args.contains("--temp\n0.1\n"), "{direct_args}");
    assert!(direct_args.contains("--seed\n7\n"), "{direct_args}");
}

#[test]
fn mlx_backend_runs_mlx_lm_mock() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
printf 'mlx ok'
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();

    let out = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_MODEL", "mlx-community/Tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(stdout_str(&out).contains("mlx ok"));
}

#[test]
fn mlx_backend_normalizes_generate_banner_output() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
cat <<'OUT'
==========
Prompt: say ok
Generation: OK
==========
Prompt tokens: 3
Generation tokens: 1
OUT
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();

    let out = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_MODEL", "mlx-community/Tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "OK");
}

#[test]
fn llm_check_reports_mlx_runtime_status() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
exit 0
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();

    let out = repo.run_with_env(
        &["llm", "check", "mlx"],
        &[
            ("CX_MLX_MODEL", "mlx-community/Tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let text = stdout_str(&out);
    assert!(text.contains("backend: mlx"), "{text}");
    assert!(text.contains("runtime_ok: yes"), "{text}");
    assert!(text.contains("model_ok: yes"), "{text}");
}

#[test]
fn llm_smoke_runs_current_backend() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "llama-cli",
        r#"#!/usr/bin/env bash
printf 'smoke ok'
"#,
    );

    let out = repo.run_with_env(
        &["llm", "smoke", "say", "ok"],
        &[
            ("CX_LLM_BACKEND", "llamacpp"),
            ("CX_LLAMA_CPP_MODEL", "/models/tiny.gguf"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let text = stdout_str(&out);
    assert!(text.contains("smoke_backend: llamacpp"), "{text}");
    assert!(text.contains("smoke_output:\nsmoke ok"), "{text}");
}

#[test]
fn smoke_updates_usage() {
    let repo = TempRepo::new("cxrs-llm");
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "tiny",
            "--backend",
            "mlx",
            "--model",
            "mlx-community/Tiny-Resolved",
        ])
        .status
        .success()
    );
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
printf 'smoke ok'
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();
    let out = repo.run_with_env(
        &["llm", "smoke", "say", "ok"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_MODEL", "tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let inspect = repo.run(&["llm", "models", "inspect", "tiny", "--json"]);
    assert!(
        inspect.status.success(),
        "stdout={} stderr={}",
        stdout_str(&inspect),
        stderr_str(&inspect)
    );
    let inspected = serde_json::from_str::<Value>(&stdout_str(&inspect)).expect("inspect json");
    assert!(
        inspected
            .get("last_used_at")
            .and_then(Value::as_str)
            .is_some(),
        "{inspected}"
    );
    assert!(
        inspected
            .get("last_smoke_status")
            .is_some_and(Value::is_null),
        "{inspected}"
    );
}

#[test]
fn llm_resident_show_json_contract() {
    let repo = TempRepo::new("cxrs-llm");
    let out = repo.run(&["llm", "resident", "show", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload = serde_json::from_str::<Value>(&stdout_str(&out)).expect("resident show json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("llm-resident.v1")
    );
    assert_eq!(
        payload.get("selected_transport").and_then(Value::as_str),
        Some("process")
    );
    assert_eq!(
        payload
            .get("boundary")
            .and_then(|v| v.get("eligible"))
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload
            .get("boundary")
            .and_then(|v| v.get("reason"))
            .and_then(Value::as_str),
        Some("transport_not_http")
    );
    assert_eq!(
        payload
            .get("runtime_capability")
            .and_then(|v| v.get("resident_server"))
            .and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn llm_resident_probe_uses_openai_profile() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat <<'JSON'
{"data":[{"id":"mlx-local"},{"id":"qwen-local"}]}
JSON
"#,
    );
    let out = repo.run_with_env(
        &["llm", "resident", "probe-models", "--json"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_REQUEST_PROFILE", "openai_json"),
            (
                "CX_HTTP_PROVIDER_URL",
                "http://127.0.0.1:11434/v1/chat/completions",
            ),
            ("CX_HTTP_REQUIRE_HTTPS", "0"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload = serde_json::from_str::<Value>(&stdout_str(&out)).expect("resident probe json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("llm-resident.v1")
    );
    assert_eq!(
        payload.get("selected_transport").and_then(Value::as_str),
        Some("http")
    );
    assert_eq!(
        payload
            .get("boundary")
            .and_then(|v| v.get("eligible"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload
            .get("boundary")
            .and_then(|v| v.get("reason"))
            .and_then(Value::as_str),
        Some("eligible")
    );
    assert_eq!(
        payload
            .get("runtime_capability")
            .and_then(|v| v.get("resident_server"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload
            .get("probe")
            .and_then(|v| v.get("probe_url"))
            .and_then(Value::as_str),
        Some("http://127.0.0.1:11434/v1/models")
    );
    assert_eq!(
        payload
            .get("probe")
            .and_then(|v| v.get("model_count"))
            .and_then(Value::as_u64),
        Some(2)
    );
}

#[test]
fn sidecar_probe_boundary() {
    let repo = TempRepo::new("cxrs-llm");
    let (url, captured, handle) =
        run_fixture_http_server_once(r#"{"data":[{"id":"sidecar-mock"}]}"#);
    let out = repo.run_with_env(
        &["llm", "resident", "probe-models", "--json"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_REQUEST_PROFILE", "openai_json"),
            ("CX_HTTP_PROVIDER_URL", url.as_str()),
            ("CX_HTTP_REQUIRE_HTTPS", "0"),
        ],
    );
    handle.join().expect("fixture http server joined");
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let request = captured.lock().expect("captured request").clone();
    let request = request.expect("captured fixture request");
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/v1/models");

    let payload = serde_json::from_str::<Value>(&stdout_str(&out)).expect("resident probe json");
    assert_eq!(
        payload.get("selected_adapter").and_then(Value::as_str),
        Some("http-curl")
    );
    assert_eq!(
        payload.get("selected_transport").and_then(Value::as_str),
        Some("http")
    );
    assert_eq!(
        payload.get("http_request_profile").and_then(Value::as_str),
        Some("openai_json")
    );
    assert_eq!(
        payload
            .get("runtime_capability")
            .and_then(|v| v.get("resident_server"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload
            .get("probe")
            .and_then(|v| v.get("model_count"))
            .and_then(Value::as_u64),
        Some(1)
    );
}

#[test]
fn resident_probe_boundary() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat <<'JSON'
{"data":[{"id":"mlx-local"}]}
JSON
"#,
    );
    let out = repo.run_with_env(
        &["llm", "resident", "probe-models", "--json"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_REQUEST_PROFILE", "openai_json"),
            (
                "CX_HTTP_PROVIDER_URL",
                "http://127.0.0.1:11434/v1/chat/completions",
            ),
            ("CX_HTTP_REQUIRE_HTTPS", "0"),
        ],
    );
    assert!(!out.status.success(), "stdout={}", stdout_str(&out));
    assert!(
        stderr_str(&out).contains("reason: backend_not_mlx"),
        "stderr={}",
        stderr_str(&out)
    );
}

#[test]
fn resident_show_remote() {
    let repo = TempRepo::new("cxrs-llm");
    let out = repo.run_with_env(
        &["llm", "resident", "show", "--json"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_REQUEST_PROFILE", "openai_json"),
            (
                "CX_HTTP_PROVIDER_URL",
                "https://api.example.com/v1/chat/completions",
            ),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload = serde_json::from_str::<Value>(&stdout_str(&out)).expect("resident show json");
    assert_eq!(
        payload
            .get("boundary")
            .and_then(|v| v.get("provider_url_local"))
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload
            .get("boundary")
            .and_then(|v| v.get("reason"))
            .and_then(Value::as_str),
        Some("provider_url_not_local")
    );
    assert_eq!(
        payload
            .get("runtime_capability")
            .and_then(|v| v.get("resident_server"))
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload
            .get("runtime_capability")
            .and_then(|v| v.get("openai_compatible"))
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn task_model_alias_resolves_active_backend() {
    let repo = TempRepo::new("cxrs-llm");
    let args_file = repo.root.join("mlx_task_args.txt");
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
printf '%s\n' "$@" > "$MLX_TASK_ARGS_FILE"
printf 'mlx task ok'
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();
    let args_path = args_file.display().to_string();

    let add_model = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny-task",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny-Task",
    ]);
    assert!(
        add_model.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add_model),
        stderr_str(&add_model)
    );

    let add_task = repo.run(&[
        "task",
        "add",
        "cxo echo model-override",
        "--role",
        "implementer",
        "--model",
        "tiny-task",
    ]);
    assert!(
        add_task.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add_task),
        stderr_str(&add_task)
    );
    let task_id = stdout_str(&add_task).trim().to_string();

    let run = repo.run_with_env(
        &["task", "run", &task_id],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_PYTHON", &mlx_python),
            ("MLX_TASK_ARGS_FILE", &args_path),
        ],
    );
    assert!(
        run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&run),
        stderr_str(&run)
    );
    assert!(stdout_str(&run).contains("mlx task ok"));

    let args = fs::read_to_string(&args_file).expect("read mlx task args");
    assert!(
        args.contains("--model\nmlx-community/Tiny-Task\n"),
        "{args}"
    );
}

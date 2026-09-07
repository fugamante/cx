use super::{native_reduce_output_with_metadata, normalize_generic};

#[test]
fn passthrough_metadata() {
    let input = "plain output\nkept as-is\n";
    let metadata = super::reduction_metadata(
        super::ReducerKind::GenericPassthrough,
        super::ReduceProfile::Balanced,
        input,
        input,
    );
    assert_eq!(metadata.reducer_kind, "generic_passthrough");
    assert_eq!(metadata.profile, "balanced");
    assert_eq!(metadata.lossiness_level, "lossless");
    assert_eq!(metadata.omitted_lines, 0);
    assert_eq!(metadata.omitted_chars, 0);
    assert!(metadata.critical_sections_kept.is_empty());
}

#[test]
fn failure_markers() {
    let input = "line 1\nFAIL test_x\nwarning: foo\nline 2\n";
    let out = super::native_reduce_output(&["test".into()], input);
    assert!(out.contains("FAIL test_x"));
    assert!(out.contains("warning: foo"));
}

#[test]
fn test_fallback() {
    let input = include_str!("../../tests/fixtures/phase_x/cargo_unknown_fallback.txt");
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/phase_x/output_fallback_manifest.json"
    ))
    .expect("parse fallback fixture manifest");
    let command = manifest["command"]
        .as_array()
        .expect("command")
        .iter()
        .map(|value| value.as_str().expect("command string").to_string())
        .collect::<Vec<_>>();
    let result = native_reduce_output_with_metadata(&command, input);

    assert_eq!(result.metadata.reducer_kind, "test_output");
    assert_eq!(result.text, normalize_generic(input));
    assert!(!result.text.is_empty());
    for span in manifest["required_spans"]
        .as_array()
        .expect("required spans")
    {
        let span = span.as_str().expect("span string");
        assert!(result.text.contains(span), "missing fallback span: {span}");
    }
}

#[test]
fn test_tail() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/phase_x/output_tail_manifest.json"
    ))
    .expect("parse tail fixture manifest");
    let prefix_lines = manifest["prefix_lines"].as_u64().expect("prefix lines") as usize;
    let mut input = (0..prefix_lines)
        .map(|index| format!("running noisy harness step {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    input.push_str(include_str!(
        "../../tests/fixtures/phase_x/cargo_late_failure.txt"
    ));
    let result = native_reduce_output_with_metadata(&["cargo".into(), "test".into()], &input);

    assert!(result.text.lines().count() <= 400);
    for span in manifest["required_spans"]
        .as_array()
        .expect("required spans")
    {
        let span = span.as_str().expect("span string");
        assert!(result.text.contains(span), "missing late span: {span}");
    }
    assert_eq!(
        result
            .text
            .matches("test result: FAILED. 0 passed; 1 failed")
            .count(),
        1
    );
}

#[test]
fn cargo_omissions() {
    let input = "opaque output\n";
    for command in [
        vec!["cargo", "check"],
        vec!["cargo", "build"],
        vec!["cargo", "clippy"],
        vec!["cargo"],
        Vec::new(),
        vec!["cargo-test"],
    ] {
        let command = command.into_iter().map(String::from).collect::<Vec<_>>();
        let result = native_reduce_output_with_metadata(&command, input);
        assert_eq!(result.metadata.reducer_kind, "generic_passthrough");
        assert_eq!(result.text, input);
    }
}

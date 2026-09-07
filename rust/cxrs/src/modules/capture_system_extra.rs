use super::{BudgetConfig, CapturePromptProfile, process_capture};

fn budget() -> BudgetConfig {
    BudgetConfig {
        budget_chars: 1000,
        budget_lines: 80,
        clip_mode: "head".to_string(),
        clip_footer: false,
    }
}

#[test]
fn shadow_unchanged() {
    let cmd = vec!["test".to_string()];
    let raw = "running 1 test\ntest api_error ... FAILED\nthread 'api_error' panicked\nerror: root cause\n";
    let legacy = process_capture(
        &cmd,
        101,
        raw.to_string(),
        true,
        false,
        CapturePromptProfile::Legacy,
        &budget(),
    );
    let shadow = process_capture(
        &cmd,
        101,
        raw.to_string(),
        true,
        true,
        CapturePromptProfile::Legacy,
        &budget(),
    );
    assert_eq!(legacy.0, shadow.0);
    assert_eq!(format!("{:?}", legacy.1), format!("{:?}", shadow.1));
}

#[test]
fn shadow_fallback() {
    let cmd = vec!["cargo".to_string(), "test".to_string()];
    let raw = include_str!("../../tests/fixtures/phase_x/cargo_unknown_fallback.txt");
    let (captured, stats) = process_capture(
        &cmd,
        2,
        raw.to_string(),
        true,
        false,
        CapturePromptProfile::ShadowNarrow,
        &budget(),
    );

    assert!(captured.contains("[high] output.reduced"));
    assert!(captured.contains("custom harness protocol v7"));
    assert!(captured.contains("opaque trailer 00ff"));
    assert_eq!(stats.capture_prompt_profile_applied, Some(true));
    assert_eq!(
        stats.capture_prompt_reducer_kind.as_deref(),
        Some("test_output")
    );
    assert_eq!(stats.capture_prompt_fallback_reason, None);
}

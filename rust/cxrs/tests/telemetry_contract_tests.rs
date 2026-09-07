mod common;

use common::*;
use serde_json::Value;
use std::fs;

fn assert_actions_for_command(repo: &TempRepo, cmd: &[&str], rows: &[Value], ctx: &str) {
    write_runs_log_rows(repo, rows);
    let out = repo.run(cmd);
    assert!(out.status.success(), "{ctx}: stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("actions json");
    assert_actions_contract(&payload);
}

#[test]
fn logs_stats_alias_reports_population_drift() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row1 = serde_json::json!({
        "execution_id":"e1","timestamp":"2026-01-01T00:00:00Z","command":"cx",
        "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
        "duration_ms":10,"schema_enforced":false,"schema_valid":true
    });
    let row2 = serde_json::json!({
        "execution_id":"e2","timestamp":"2026-01-01T00:00:01Z","command":"next",
        "backend_used":"primary","capture_provider":"native","execution_mode":"deterministic",
        "duration_ms":20,"schema_enforced":true,"schema_valid":true,"task_id":"task_001",
        "retry_attempt":2,"timed_out":false
    });
    let mut text = serde_json::to_string(&row1).expect("row1");
    text.push('\n');
    text.push_str(&serde_json::to_string(&row2).expect("row2"));
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["logs", "stats", "2"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let stdout = stdout_str(&out);
    assert!(stdout.contains("logs stats"), "{stdout}");
    assert!(stdout.contains("retry_telemetry"), "{stdout}");
    assert!(stdout.contains("retry_attempt_histogram"), "{stdout}");
    assert!(stdout.contains("field_population"), "{stdout}");
    assert!(stdout.contains("contract_drift"), "{stdout}");
    assert!(stdout.contains("new_keys_second_half"), "{stdout}");

    let out_json = repo.run(&["telemetry", "2", "--json"]);
    assert!(
        out_json.status.success(),
        "stderr={}",
        stderr_str(&out_json)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out_json)).expect("json output");
    assert_eq!(v.get("window_runs").and_then(Value::as_u64), Some(2));
    assert_eq!(v["normalization"]["modern_rows"], 0);
    assert_eq!(v["normalization"]["legacy_rows"], 2);
    assert_eq!(v["normalization"]["migrated_legacy_rows"], 0);
    assert_eq!(
        v["normalization"]["recommendation"],
        "run `cx logs migrate`"
    );
    let fields = v
        .get("fields")
        .and_then(Value::as_array)
        .expect("fields array");
    assert!(
        fields
            .iter()
            .any(|f| f.get("field").and_then(Value::as_str) == Some("execution_id")),
        "missing execution_id coverage field: {v}"
    );
    assert!(
        fields.iter().all(|f| {
            f.get("field").is_some()
                && f.get("present").and_then(Value::as_u64).is_some()
                && f.get("non_null").and_then(Value::as_u64).is_some()
                && f.get("total").and_then(Value::as_u64).is_some()
        }),
        "invalid field shape in telemetry payload: {v}"
    );
    let drift = v.get("contract_drift").expect("contract_drift");
    assert!(drift.get("new_keys_second_half").is_some());
    assert!(drift.get("missing_keys_second_half").is_some());
    let retry = v.get("retry_telemetry").expect("retry_telemetry");
    assert!(retry.get("rows_with_retry_metadata").is_some());
    assert!(retry.get("rows_after_retry_success_rate").is_some());
    assert!(retry.get("attempt_histogram").is_some());
    let capture_prompt = v
        .get("capture_prompt_telemetry")
        .expect("capture_prompt_telemetry");
    assert!(capture_prompt.get("rows_with_explicit_profile").is_some());
    assert!(
        capture_prompt
            .get("shadow_narrow_configured_runs")
            .is_some()
    );
    assert!(capture_prompt.get("shadow_narrow_applied_runs").is_some());
    assert!(capture_prompt.get("shadow_narrow_fallback_runs").is_some());
    assert!(capture_prompt.get("applied_reducer_kinds").is_some());
    assert!(capture_prompt.get("fallback_reasons").is_some());
    let critical = v.get("critical_telemetry").expect("critical_telemetry");
    assert!(critical.get("summary_rows").is_some());
    assert!(critical.get("halted_rows").is_some());
    assert!(critical.get("critical_errors_total").is_some());
    let timing = v.get("timing_telemetry").expect("timing_telemetry");
    assert!(timing.get("task_rows").is_some());
    assert!(timing.get("rows_with_worker_id").is_some());
    assert!(timing.get("rows_with_queue_ms").is_some());
    assert!(timing.get("rows_with_wave_index").is_some());
    assert!(timing.get("rows_with_wave_mode").is_some());
    assert!(timing.get("rows_with_wave_size").is_some());
    let http_modes = v
        .get("http_mode_stats")
        .and_then(Value::as_array)
        .expect("http_mode_stats");
    assert!(
        http_modes.is_empty()
            || http_modes.iter().all(|m| {
                m.get("format").is_some()
                    && m.get("parser_mode").is_some()
                    && m.get("runs").and_then(Value::as_u64).is_some()
                    && m.get("success_rate").is_some()
            }),
        "invalid http_mode_stats shape: {v}"
    );
}

#[test]
fn telemetry_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"tf1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,
            "provider_transport":"http","http_provider_format":"text","http_parser_mode":"envelope"
        }),
        serde_json::json!({
            "execution_id":"tf2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":12,"schema_enforced":false,"schema_valid":true
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["telemetry", "10", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("telemetry json");
    let fixture = load_fixture_json("telemetry_json_contract.json");

    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "telemetry");
    let backend_caps_keys = fixture_keys(&fixture, "backend_capabilities_keys");
    assert_has_keys(
        payload
            .get("backend_capabilities")
            .expect("backend_capabilities"),
        &backend_caps_keys,
        "telemetry.backend_capabilities",
    );
    let turboquant_keys = fixture_keys(&fixture, "backend_capabilities_turboquant_keys");
    assert_has_keys(
        payload
            .get("backend_capabilities")
            .and_then(|v| v.get("turboquant"))
            .expect("backend_capabilities.turboquant"),
        &turboquant_keys,
        "telemetry.backend_capabilities.turboquant",
    );
    let adapter_policy_keys = fixture_keys(&fixture, "adapter_rollout_policy_keys");
    assert_has_keys(
        payload
            .get("adapter_rollout_policy")
            .expect("adapter_rollout_policy"),
        &adapter_policy_keys,
        "telemetry.adapter_rollout_policy",
    );
    let phase7_metrics_keys = fixture_keys(&fixture, "phase7_metrics_keys");
    assert_has_keys(
        payload.get("phase7_metrics").expect("phase7_metrics"),
        &phase7_metrics_keys,
        "telemetry.phase7_metrics",
    );
    let task_execution_keys = fixture_keys(&fixture, "task_execution_keys");
    assert_has_keys(
        payload.get("task_execution").expect("task_execution"),
        &task_execution_keys,
        "telemetry.task_execution",
    );
    let next_action_keys = fixture_keys(&fixture, "task_execution_next_action_keys");
    assert_has_keys(
        payload
            .get("task_execution")
            .and_then(|v| v.get("next_action"))
            .expect("task_execution.next_action"),
        &next_action_keys,
        "telemetry.task_execution.next_action",
    );
    let recent_context_keys = fixture_keys(&fixture, "task_execution_recent_context_keys");
    assert_has_keys(
        payload
            .get("task_execution")
            .and_then(|v| v.get("recent_context"))
            .expect("task_execution.recent_context"),
        &recent_context_keys,
        "telemetry.task_execution.recent_context",
    );
    let phase7_bias_keys = fixture_keys(&fixture, "task_execution_phase7_bias_keys");
    assert_has_keys(
        payload
            .get("task_execution")
            .and_then(|v| v.get("phase7_bias"))
            .expect("task_execution.phase7_bias"),
        &phase7_bias_keys,
        "telemetry.task_execution.phase7_bias",
    );
    let reasoning_gate_keys = fixture_keys(&fixture, "task_execution_reasoning_gate_keys");
    assert_has_keys(
        payload
            .get("task_execution")
            .and_then(|v| v.get("reasoning_gate"))
            .expect("task_execution.reasoning_gate"),
        &reasoning_gate_keys,
        "telemetry.task_execution.reasoning_gate",
    );
    let wave_pressure_keys = fixture_keys(&fixture, "task_execution_wave_pressure_keys");
    assert_has_keys(
        payload
            .get("task_execution")
            .and_then(|v| v.get("wave_pressure"))
            .expect("task_execution.wave_pressure"),
        &wave_pressure_keys,
        "telemetry.task_execution.wave_pressure",
    );
    let drift_keys = fixture_keys(&fixture, "contract_drift_keys");
    assert_has_keys(
        payload.get("contract_drift").expect("contract_drift"),
        &drift_keys,
        "telemetry.contract_drift",
    );
    let capture_prompt_keys = fixture_keys(&fixture, "capture_prompt_keys");
    assert_has_keys(
        payload
            .get("capture_prompt_telemetry")
            .expect("capture_prompt_telemetry"),
        &capture_prompt_keys,
        "telemetry.capture_prompt_telemetry",
    );
    let reducer_item_keys = fixture_keys(&fixture, "capture_prompt_reducer_item_keys");
    let reducer_items = payload
        .get("capture_prompt_telemetry")
        .and_then(|v| v.get("applied_reducer_kinds"))
        .and_then(Value::as_array)
        .expect("capture_prompt_telemetry.applied_reducer_kinds");
    for item in reducer_items {
        assert_has_keys(
            item,
            &reducer_item_keys,
            "telemetry.capture_prompt_telemetry.applied_reducer_kinds[*]",
        );
    }
    let fallback_item_keys = fixture_keys(&fixture, "capture_prompt_fallback_item_keys");
    let fallback_items = payload
        .get("capture_prompt_telemetry")
        .and_then(|v| v.get("fallback_reasons"))
        .and_then(Value::as_array)
        .expect("capture_prompt_telemetry.fallback_reasons");
    for item in fallback_items {
        assert_has_keys(
            item,
            &fallback_item_keys,
            "telemetry.capture_prompt_telemetry.fallback_reasons[*]",
        );
    }
    let retry_keys = fixture_keys(&fixture, "retry_keys");
    assert_has_keys(
        payload.get("retry_telemetry").expect("retry_telemetry"),
        &retry_keys,
        "telemetry.retry_telemetry",
    );
    let critical_keys = fixture_keys(&fixture, "critical_keys");
    assert_has_keys(
        payload
            .get("critical_telemetry")
            .expect("critical_telemetry"),
        &critical_keys,
        "telemetry.critical_telemetry",
    );
    let timing_keys = fixture_keys(&fixture, "timing_keys");
    assert_has_keys(
        payload.get("timing_telemetry").expect("timing_telemetry"),
        &timing_keys,
        "telemetry.timing_telemetry",
    );
    let item_keys = fixture_keys(&fixture, "http_mode_item_keys");
    let modes = payload
        .get("http_mode_stats")
        .and_then(Value::as_array)
        .expect("http_mode_stats array");
    for item in modes {
        assert_has_keys(item, &item_keys, "telemetry.http_mode_stats[*]");
    }
    assert_eq!(
        payload
            .get("backend_capabilities")
            .and_then(|v| v.get("turboquant"))
            .and_then(|v| v.get("cx_runtime_support"))
            .and_then(Value::as_str),
        Some("none")
    );
}

#[test]
fn telemetry_capture_prompt() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"cp1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,
            "capture_prompt_profile":"shadow_narrow","capture_prompt_profile_applied":true,
            "capture_prompt_reducer_kind":"test_output"
        }),
        serde_json::json!({
            "execution_id":"cp2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":11,"schema_enforced":false,"schema_valid":true,
            "capture_prompt_profile":"shadow_narrow","capture_prompt_profile_applied":true,
            "capture_prompt_reducer_kind":"git_diff"
        }),
        serde_json::json!({
            "execution_id":"cp3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":12,"schema_enforced":false,"schema_valid":true,
            "capture_prompt_profile":"shadow_narrow","capture_prompt_profile_applied":false,
            "capture_prompt_reducer_kind":"git_status",
            "capture_prompt_fallback_reason":"unsupported_reducer"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["telemetry", "10", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("telemetry json");
    let telemetry = payload
        .get("capture_prompt_telemetry")
        .expect("capture_prompt_telemetry");
    assert_eq!(
        telemetry
            .get("rows_with_explicit_profile")
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        telemetry
            .get("shadow_narrow_configured_runs")
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        telemetry
            .get("shadow_narrow_applied_runs")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        telemetry
            .get("shadow_narrow_fallback_runs")
            .and_then(Value::as_u64),
        Some(1)
    );
    let reducer_items = telemetry
        .get("applied_reducer_kinds")
        .and_then(Value::as_array)
        .expect("applied_reducer_kinds");
    assert!(reducer_items.iter().any(|item| {
        item.get("reducer_kind").and_then(Value::as_str) == Some("git_diff")
            && item.get("runs").and_then(Value::as_u64) == Some(1)
    }));
    assert!(reducer_items.iter().any(|item| {
        item.get("reducer_kind").and_then(Value::as_str) == Some("test_output")
            && item.get("runs").and_then(Value::as_u64) == Some(1)
    }));
    let fallback_items = telemetry
        .get("fallback_reasons")
        .and_then(Value::as_array)
        .expect("fallback_reasons");
    assert_eq!(fallback_items.len(), 1);
    assert_eq!(
        fallback_items[0].get("reason").and_then(Value::as_str),
        Some("unsupported_reducer")
    );
    assert_eq!(
        fallback_items[0].get("runs").and_then(Value::as_u64),
        Some(1)
    );
}

#[test]
fn logs_stats_strict_severity_behave_expected() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let weak_row = serde_json::json!({
        "execution_id":"e1","timestamp":"2026-01-01T00:00:00Z","command":"cx",
        "backend_used":"primary","capture_provider":"native","execution_mode":"lean"
    });
    let mut text = serde_json::to_string(&weak_row).expect("row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let strict = repo.run(&["logs", "stats", "1", "--strict"]);
    assert_eq!(
        strict.status.code(),
        Some(1),
        "stdout={}",
        stdout_str(&strict)
    );
    let strict_out = stdout_str(&strict);
    assert!(strict_out.contains("severity: critical"), "{strict_out}");
    assert!(strict_out.contains("strict_violations"), "{strict_out}");

    let severity_only = repo.run(&["telemetry", "1", "--severity"]);
    assert!(
        severity_only.status.success(),
        "stderr={}",
        stderr_str(&severity_only)
    );
    let sev_out = stdout_str(&severity_only);
    assert!(sev_out.contains("severity:"), "{sev_out}");
    assert!(!sev_out.contains("field_population"), "{sev_out}");

    let validate_default = repo.run(&["logs", "validate"]);
    assert!(
        validate_default.status.success(),
        "stdout={} stderr={}",
        stdout_str(&validate_default),
        stderr_str(&validate_default)
    );
    let validate_default_out = stdout_str(&validate_default);
    assert!(
        validate_default_out.contains("status: ok_with_warnings"),
        "{validate_default_out}"
    );

    let validate = repo.run(&["logs", "validate", "--strict"]);
    assert_eq!(
        validate.status.code(),
        Some(1),
        "stdout={} stderr={}",
        stdout_str(&validate),
        stderr_str(&validate)
    );
    let validate_out = stdout_str(&validate);
    let issue_count = parse_labeled_u64(&validate_out, "issue_count:")
        .expect("issue_count in logs validate output");

    let telemetry_json = repo.run(&["telemetry", "1", "--json"]);
    assert!(
        telemetry_json.status.success(),
        "stderr={}",
        stderr_str(&telemetry_json)
    );
    let v: Value = serde_json::from_str(&stdout_str(&telemetry_json)).expect("telemetry json");
    let required = v
        .get("required_fields")
        .and_then(Value::as_u64)
        .expect("required_fields");
    let strict_violations = v
        .get("strict_violations")
        .and_then(Value::as_u64)
        .expect("strict_violations");

    assert_eq!(
        issue_count, strict_violations,
        "logs validate and telemetry strict violation counts diverged"
    );
    assert_eq!(required, 36, "unexpected strict contract field count");
}

#[test]
fn telemetry_json_groups_http_mode_stats() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let rows = vec![
        serde_json::json!({
            "execution_id":"h1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,
            "provider_transport":"http","http_provider_format":"text","http_parser_mode":"envelope"
        }),
        serde_json::json!({
            "execution_id":"h2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":12,"schema_enforced":false,"schema_valid":false,
            "provider_transport":"http","http_provider_format":"text","http_parser_mode":"envelope"
        }),
        serde_json::json!({
            "execution_id":"h3","timestamp":"2026-01-01T00:00:02Z","command":"cxj","tool":"cxj",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":14,"schema_enforced":false,"schema_valid":true,
            "provider_transport":"http","http_provider_format":"jsonl","http_parser_mode":"jsonl_passthrough"
        }),
    ];
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["telemetry", "10", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("telemetry json");
    let modes = v
        .get("http_mode_stats")
        .and_then(Value::as_array)
        .expect("http_mode_stats array");
    assert!(!modes.is_empty(), "expected grouped http_mode_stats: {v}");

    let text_mode = modes
        .iter()
        .find(|m| {
            m.get("format").and_then(Value::as_str) == Some("text")
                && m.get("parser_mode").and_then(Value::as_str) == Some("envelope")
        })
        .expect("text/envelope mode");
    assert_eq!(text_mode.get("runs").and_then(Value::as_u64), Some(2));
    assert_eq!(
        text_mode.get("schema_invalid").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        text_mode.get("healthy_runs").and_then(Value::as_u64),
        Some(1)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn telemetry_json_output_is_stable_on_macos() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row = serde_json::json!({
        "execution_id":"m1","timestamp":"2026-01-01T00:00:00Z","command":"cx",
        "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
        "duration_ms":12,"schema_enforced":false,"schema_valid":true
    });
    let mut text = serde_json::to_string(&row).expect("row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out1 = repo.run(&["telemetry", "1", "--json"]);
    let out2 = repo.run(&["telemetry", "1", "--json"]);
    assert!(out1.status.success(), "stderr={}", stderr_str(&out1));
    assert!(out2.status.success(), "stderr={}", stderr_str(&out2));

    let v1: Value = serde_json::from_str(&stdout_str(&out1)).expect("json1");
    let v2: Value = serde_json::from_str(&stdout_str(&out2)).expect("json2");
    assert_eq!(
        v1.get("window_runs").and_then(Value::as_u64),
        Some(1),
        "unexpected telemetry window: {v1}"
    );
    assert_eq!(v1, v2, "telemetry output drifted on repeated invocation");
}

#[test]
fn diag_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row = serde_json::json!({
        "execution_id":"diagfx1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
        "duration_ms":10,"schema_enforced":false,"schema_valid":true,"queue_ms":250,"worker_id":"w1"
    });
    let mut text = serde_json::to_string(&row).expect("serialize row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag", "--json", "--window", "1"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    let fixture = load_fixture_json("diag_json_contract.json");

    assert_fixture_contract(
        &payload,
        &fixture,
        "top_level_keys",
        &[
            (
                "backend_capabilities",
                "backend_capabilities_keys",
                "diag.backend_capabilities",
            ),
            (
                "adapter_rollout_policy",
                "adapter_rollout_policy_keys",
                "diag.adapter_rollout_policy",
            ),
            (
                "operator_context",
                "operator_context_keys",
                "diag.operator_context",
            ),
            ("routing_trace", "routing_trace_keys", "diag.routing_trace"),
            ("scheduler", "scheduler_keys", "diag.scheduler"),
            (
                "phase7_metrics",
                "phase7_metrics_keys",
                "diag.phase7_metrics",
            ),
            (
                "task_readiness",
                "task_readiness_keys",
                "diag.task_readiness",
            ),
            (
                "task_execution",
                "task_execution_keys",
                "diag.task_execution",
            ),
            ("retry", "retry_keys", "diag.retry"),
            ("critical", "critical_keys", "diag.critical"),
            ("concurrency", "concurrency_keys", "diag.concurrency"),
            (
                "capture_prompt_profile_rollout",
                "capture_prompt_profile_rollout_keys",
                "diag.capture_prompt_profile_rollout",
            ),
        ],
    );

    let concurrency = payload.get("concurrency").expect("diag.concurrency");
    let task_execution = payload.get("task_execution").expect("diag.task_execution");
    let defaults = concurrency
        .get("defaults")
        .expect("diag.concurrency.defaults");
    let observed = concurrency
        .get("observed")
        .expect("diag.concurrency.observed");
    for key in [
        "run_all_mode",
        "backend_pool",
        "backend_caps",
        "max_workers",
        "fairness",
        "halt_on_critical",
    ] {
        assert!(
            defaults.get(key).is_some(),
            "diag.concurrency.defaults missing key '{key}' in {defaults}"
        );
    }
    for key in [
        "window_runs",
        "run_all_rows",
        "latest_run_all_mode",
        "run_all_mode_counts",
        "halt_on_critical_rows",
        "halted_remaining_total",
        "latest_halted_remaining",
        "backend_fallback_rows",
        "latest_backend_fallbacks",
        "latest_worker_count",
        "latest_workers",
        "latest_max_retry_attempt",
        "latest_first_queue_started_at",
        "latest_first_task_started_at",
        "latest_last_task_finished_at",
        "wave_task_rows",
        "latest_wave_index",
        "latest_wave_mode",
        "latest_wave_size",
        "largest_wave_index",
        "largest_wave_size",
        "max_queue_wave_index",
        "max_queue_wave_ms",
    ] {
        assert!(
            observed.get(key).is_some(),
            "diag.concurrency.observed missing key '{key}' in {observed}"
        );
    }
    assert!(
        task_execution
            .get("recommendations")
            .and_then(Value::as_array)
            .is_some(),
        "diag.task_execution.recommendations missing in {task_execution}"
    );
    assert!(
        task_execution
            .get("next_action")
            .and_then(Value::as_object)
            .is_some(),
        "diag.task_execution.next_action missing in {task_execution}"
    );
    let next_action_keys = fixture_keys(&fixture, "task_execution_next_action_keys");
    assert_has_keys(
        task_execution
            .get("next_action")
            .expect("diag.task_execution.next_action"),
        &next_action_keys,
        "diag.task_execution.next_action",
    );
    let recent_context_keys = fixture_keys(&fixture, "task_execution_recent_context_keys");
    assert_has_keys(
        task_execution
            .get("recent_context")
            .expect("diag.task_execution.recent_context"),
        &recent_context_keys,
        "diag.task_execution.recent_context",
    );
    let phase7_bias_keys = fixture_keys(&fixture, "task_execution_phase7_bias_keys");
    assert_has_keys(
        task_execution
            .get("phase7_bias")
            .expect("diag.task_execution.phase7_bias"),
        &phase7_bias_keys,
        "diag.task_execution.phase7_bias",
    );
    let concurrency_keys = fixture_keys(&fixture, "task_execution_concurrency_keys");
    assert_has_keys(
        task_execution
            .get("concurrency")
            .expect("diag.task_execution.concurrency"),
        &concurrency_keys,
        "diag.task_execution.concurrency",
    );
    let invariants_keys = fixture_keys(&fixture, "task_execution_invariants_keys");
    assert_has_keys(
        task_execution
            .get("invariants")
            .expect("diag.task_execution.invariants"),
        &invariants_keys,
        "diag.task_execution.invariants",
    );
    let reasoning_gate_keys = fixture_keys(&fixture, "task_execution_reasoning_gate_keys");
    assert_has_keys(
        task_execution
            .get("reasoning_gate")
            .expect("diag.task_execution.reasoning_gate"),
        &reasoning_gate_keys,
        "diag.task_execution.reasoning_gate",
    );
    let wave_pressure_keys = fixture_keys(&fixture, "task_execution_wave_pressure_keys");
    assert_has_keys(
        task_execution
            .get("wave_pressure")
            .expect("diag.task_execution.wave_pressure"),
        &wave_pressure_keys,
        "diag.task_execution.wave_pressure",
    );
    assert_eq!(
        payload
            .get("backend_capabilities")
            .and_then(|v| v.get("turboquant"))
            .and_then(|v| v.get("cx_runtime_support"))
            .and_then(Value::as_str),
        Some("none")
    );
    let turboquant = payload
        .get("backend_capabilities")
        .and_then(|v| v.get("turboquant"))
        .expect("diag.backend_capabilities.turboquant");
    let turboquant_keys = fixture_keys(&fixture, "backend_capabilities_turboquant_keys");
    assert_has_keys(
        turboquant,
        &turboquant_keys,
        "diag.backend_capabilities.turboquant",
    );
    let runtime = payload
        .get("backend_capabilities")
        .and_then(|v| v.get("runtime"))
        .expect("diag.backend_capabilities.runtime");
    let runtime_keys = fixture_keys(&fixture, "backend_capabilities_runtime_keys");
    assert_has_keys(runtime, &runtime_keys, "diag.backend_capabilities.runtime");
}

#[test]
fn diag_capture_prompt() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"dcp1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,
            "capture_prompt_profile":"shadow_narrow","capture_prompt_profile_applied":true,
            "capture_prompt_reducer_kind":"test_output"
        }),
        serde_json::json!({
            "execution_id":"dcp2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":11,"schema_enforced":false,"schema_valid":true,
            "capture_prompt_profile":"shadow_narrow","capture_prompt_profile_applied":false,
            "capture_prompt_reducer_kind":"git_status",
            "capture_prompt_fallback_reason":"unsupported_reducer"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["diag", "--json", "--actions", "--window", "4"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    let rollout = payload
        .get("capture_prompt_profile_rollout")
        .expect("capture prompt rollout");
    assert_eq!(
        rollout
            .get("rows_with_explicit_profile")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        rollout
            .get("shadow_narrow_configured_runs")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        rollout
            .get("shadow_narrow_applied_runs")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        rollout
            .get("shadow_narrow_fallback_runs")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        rollout.get("latest_execution_id").and_then(Value::as_str),
        Some("dcp2")
    );
    assert_eq!(
        rollout
            .get("latest_fallback_reason")
            .and_then(Value::as_str),
        Some("unsupported_reducer")
    );
    assert!(
        rollout
            .get("recommendation")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("fell back to legacy reduction")),
        "{rollout}"
    );
    let actions = payload
        .get("actions")
        .and_then(Value::as_array)
        .expect("actions array");
    assert!(
        actions.iter().any(|action| {
            action.get("id").and_then(Value::as_str) == Some("capture_prompt_rollout_followup")
                && action.get("command").and_then(Value::as_str)
                    == Some("xshelf telemetry 200 --json")
        }),
        "missing capture prompt rollout action in {actions:?}"
    );
}

#[test]
fn diag_json_strict_fails_on_queue_severity() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let mut text = String::new();
    for i in 1..=6u64 {
        let row = serde_json::json!({
            "execution_id":format!("ds{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":3000 + i * 10,"worker_id":"w1"
        });
        text.push_str(&serde_json::to_string(&row).expect("serialize row"));
        text.push('\n');
    }
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag", "--json", "--strict"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    assert_ne!(v.get("severity").and_then(Value::as_str), Some("ok"));
    let reasons = v
        .get("severity_reasons")
        .and_then(Value::as_array)
        .expect("severity reasons array");
    assert!(
        !reasons.is_empty(),
        "expected severity reasons in strict mode"
    );
}

#[test]
fn diag_json_strict_passes_on_ok_severity() {
    let repo = TempRepo::new("cxrs-it");
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().expect("log parent")).expect("mkdir logs");
    let row = serde_json::json!({
        "execution_id":"dsp1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
        "duration_ms":11,"schema_enforced":false,"schema_valid":true,"queue_ms":50,"worker_id":"w1"
    });
    let mut text = serde_json::to_string(&row).expect("serialize row");
    text.push('\n');
    fs::write(&log, text).expect("write runs");

    let out = repo.run(&["diag", "--json", "--strict"]);
    assert!(
        out.status.success(),
        "expected strict pass; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    assert_eq!(v.get("severity").and_then(Value::as_str), Some("ok"));
}

#[test]
fn diag_json_actions_match_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=4u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("diact{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":2500 + i,"worker_id":"w1"
        }));
    }
    assert_actions_for_command(
        &repo,
        &["diag", "--json", "--actions", "--window", "4"],
        &rows,
        "diag actions",
    );
}

#[test]
fn scheduler_json_actions_match_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=4u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("schact{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":2400 + i,"worker_id":"w1"
        }));
    }
    assert_actions_for_command(
        &repo,
        &["scheduler", "--json", "--actions", "--window", "4"],
        &rows,
        "scheduler actions",
    );
}

#[test]
fn diag_actions_next() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"dexec1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":120,"schema_enforced":false,"schema_valid":true,
            "run_all_mode":"mixed","halt_on_critical":true,
            "run_all_scheduled":3,"run_all_complete":1,"run_all_failed":1,"run_all_critical_errors":1,
            "run_all_halted_remaining":1,
            "run_all_backend_fallback_rows":0,
            "run_all_wave_pressure_kind":"none",
            "run_all_latest_wave_index":2,
            "run_all_max_queue_wave_index":2,
            "run_all_max_queue_wave_ms":1000
        }),
        serde_json::json!({
            "execution_id":"dexec2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":20,"schema_enforced":false,"schema_valid":true,"queue_ms":2500,"worker_id":"w1"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["diag", "--json", "--actions", "--window", "4"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    let actions = payload
        .get("actions")
        .and_then(Value::as_array)
        .expect("actions array");
    let first = actions.first().expect("first action");
    assert_eq!(
        first.get("command").and_then(Value::as_str),
        Some("xshelf scheduler --json --window 20")
    );
    assert_eq!(
        first.get("id").and_then(Value::as_str),
        Some("task_execution_inspect_scheduler")
    );
}

#[test]
fn diag_actions_bias() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"dbias1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":120,"schema_enforced":false,"schema_valid":true,
            "run_all_mode":"mixed","halt_on_critical":false,
            "run_all_scheduled":4,"run_all_complete":2,"run_all_failed":1,"run_all_critical_errors":0,
            "run_all_halted_remaining":0,
            "run_all_backend_fallback_rows":0,
            "run_all_failure_pattern":"retryable_failure",
            "run_all_recommended_resume_point":"xshelf task run-all --status pending"
        }),
        serde_json::json!({
            "execution_id":"dbias2","timestamp":"2026-01-01T00:00:01Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":120,"schema_enforced":false,"schema_valid":true,
            "run_all_mode":"mixed","halt_on_critical":false,
            "run_all_scheduled":4,"run_all_complete":2,"run_all_failed":1,"run_all_critical_errors":0,
            "run_all_halted_remaining":0,
            "run_all_backend_fallback_rows":0,
            "run_all_failure_pattern":"retryable_failure",
            "run_all_recommended_resume_point":"xshelf task run-all --status pending"
        }),
        serde_json::json!({
            "execution_id":"dbias3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":20,"schema_enforced":false,"schema_valid":true,"queue_ms":2500,"worker_id":"w1"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["diag", "--json", "--actions", "--window", "4"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    let actions = payload
        .get("actions")
        .and_then(Value::as_array)
        .expect("actions array");
    let first = actions.first().expect("first action");
    assert_eq!(
        first.get("command").and_then(Value::as_str),
        Some("xshelf task run-all --status pending")
    );
    assert!(
        first
            .get("rationale")
            .and_then(Value::as_str)
            .is_some_and(|v| v.contains("Phase VII bias:")),
        "{first}"
    );
}

#[test]
fn scheduler_actions_next() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"sexec1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":100,"schema_enforced":false,"schema_valid":true,
            "run_all_mode":"parallel","halt_on_critical":false,
            "run_all_scheduled":4,"run_all_complete":4,"run_all_failed":0,"run_all_critical_errors":0,
            "run_all_halted_remaining":0,
            "run_all_backend_fallback_rows":0,
            "run_all_wave_pressure_kind":"later_wave_queue",
            "run_all_wave_pressure_suggested_mode":"mixed",
            "run_all_latest_wave_index":4,
            "run_all_max_queue_wave_index":4,
            "run_all_max_queue_wave_ms":4500
        }),
        serde_json::json!({
            "execution_id":"sexec2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":24,"schema_enforced":false,"schema_valid":true,"queue_ms":2600,"worker_id":"w1"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["scheduler", "--json", "--actions", "--window", "4"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("scheduler json");
    let actions = payload
        .get("actions")
        .and_then(Value::as_array)
        .expect("actions array");
    let first = actions.first().expect("first action");
    assert_eq!(
        first.get("command").and_then(Value::as_str),
        Some("xshelf task run-all --mode mixed --status pending")
    );
    assert_eq!(
        first.get("id").and_then(Value::as_str),
        Some("task_execution_rerun")
    );
}

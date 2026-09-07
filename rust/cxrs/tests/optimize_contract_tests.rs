mod common;

use common::*;
use serde_json::Value;

#[test]
fn optimize_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let row = serde_json::json!({
        "execution_id":"ofx1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
        "duration_ms":1000,"schema_enforced":false,"schema_valid":true,
        "retry_attempt":2,"timed_out":false
    });
    write_runs_log_row(&repo, &row);

    let out = repo.run(&["optimize", "10", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("optimize json");
    let fixture = load_fixture_json("optimize_json_contract.json");

    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "optimize");
    let backend_caps_keys = fixture_keys(&fixture, "backend_capabilities_keys");
    assert_has_keys(
        payload
            .get("backend_capabilities")
            .expect("backend_capabilities"),
        &backend_caps_keys,
        "optimize.backend_capabilities",
    );
    let turboquant_keys = fixture_keys(&fixture, "backend_capabilities_turboquant_keys");
    assert_has_keys(
        payload
            .get("backend_capabilities")
            .and_then(|v| v.get("turboquant"))
            .expect("backend_capabilities.turboquant"),
        &turboquant_keys,
        "optimize.backend_capabilities.turboquant",
    );
    let sb_keys = fixture_keys(&fixture, "scoreboard_keys");
    assert_has_keys(
        payload.get("scoreboard").expect("scoreboard"),
        &sb_keys,
        "optimize.scoreboard",
    );
    let retry_keys = fixture_keys(&fixture, "retry_health_keys");
    assert_has_keys(
        payload
            .get("scoreboard")
            .and_then(|v| v.get("retry_health"))
            .expect("retry_health"),
        &retry_keys,
        "optimize.scoreboard.retry_health",
    );
    let timing_keys = fixture_keys(&fixture, "timing_coverage_keys");
    assert_has_keys(
        payload
            .get("scoreboard")
            .and_then(|v| v.get("timing_attribution_coverage"))
            .expect("timing_attribution_coverage"),
        &timing_keys,
        "optimize.scoreboard.timing_attribution_coverage",
    );
    let capture_prompt_keys = fixture_keys(&fixture, "capture_prompt_rollout_keys");
    assert_has_keys(
        payload
            .get("scoreboard")
            .and_then(|v| v.get("capture_prompt_profile_rollout"))
            .expect("capture_prompt_profile_rollout"),
        &capture_prompt_keys,
        "optimize.scoreboard.capture_prompt_profile_rollout",
    );
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
fn optimize_capture_prompt() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"ocp1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":900,"schema_enforced":false,"schema_valid":true,
            "capture_prompt_profile":"shadow_narrow","capture_prompt_profile_applied":true,
            "capture_prompt_reducer_kind":"test_output"
        }),
        serde_json::json!({
            "execution_id":"ocp2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":950,"schema_enforced":false,"schema_valid":true,
            "capture_prompt_profile":"shadow_narrow","capture_prompt_profile_applied":false,
            "capture_prompt_reducer_kind":"git_status",
            "capture_prompt_fallback_reason":"unsupported_reducer"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["optimize", "10", "--json", "--actions"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("optimize json");
    let rollout = payload
        .get("scoreboard")
        .and_then(|v| v.get("capture_prompt_profile_rollout"))
        .expect("capture_prompt_profile_rollout");
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
        rollout
            .get("shadow_narrow_fallback_rate")
            .and_then(Value::as_f64),
        Some(0.5)
    );
    let recommendations = payload
        .get("recommendations")
        .and_then(Value::as_array)
        .expect("recommendations");
    assert!(recommendations.iter().any(|item| {
        item.as_str()
            .is_some_and(|v| v.contains("shadow_narrow fallback observed"))
    }));
    let actions = payload
        .get("actions")
        .and_then(Value::as_array)
        .expect("actions");
    assert!(actions.iter().any(|action| {
        action.get("id").and_then(Value::as_str) == Some("capture_prompt_rollout_followup")
            && action.get("command").and_then(Value::as_str) == Some("xshelf telemetry 200 --json")
    }));
}

#[test]
fn optimize_json_actions_match_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"oact1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":5000,"schema_enforced":true,"schema_valid":false,
            "input_tokens":1000,"cached_input_tokens":10
        }),
        serde_json::json!({
            "execution_id":"oact2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":4000,"schema_enforced":false,"schema_valid":true,
            "input_tokens":1000,"cached_input_tokens":5
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["optimize", "10", "--json", "--actions"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("optimize json");
    assert_actions_contract(&payload);
}

#[test]
fn optimize_actions_next() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"oexec1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":120,"schema_enforced":false,"schema_valid":true,
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
            "execution_id":"oexec2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":5000,"schema_enforced":true,"schema_valid":false,
            "input_tokens":1000,"cached_input_tokens":10
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["optimize", "10", "--json", "--actions"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("optimize json");
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

#[test]
fn optimize_actions_bias() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"obias1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
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
            "execution_id":"obias2","timestamp":"2026-01-01T00:00:01Z","command":"cxtask_runall","tool":"cxtask_runall",
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
            "execution_id":"obias3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":5000,"schema_enforced":true,"schema_valid":false,
            "input_tokens":1000,"cached_input_tokens":10
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["optimize", "10", "--json", "--actions"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("optimize json");
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
fn optimize_actions_strict_gate_is_deterministic() {
    let repo = TempRepo::new("cxrs-it");
    let row = serde_json::json!({
        "execution_id":"ogate1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
        "duration_ms":5000,"schema_enforced":false,"schema_valid":true,
        "input_tokens":1000,"cached_input_tokens":0
    });
    write_runs_log_row(&repo, &row);

    let warn = repo.run(&[
        "optimize",
        "10",
        "--json",
        "--actions",
        "--strict",
        "--severity",
        "warning",
    ]);
    assert!(
        !warn.status.success(),
        "expected warning gate failure, stdout={} stderr={}",
        stdout_str(&warn),
        stderr_str(&warn)
    );

    let crit = repo.run(&[
        "optimize",
        "10",
        "--json",
        "--actions",
        "--strict",
        "--severity",
        "critical",
    ]);
    assert!(
        crit.status.success(),
        "critical gate should pass on warning-only actions, stdout={} stderr={}",
        stdout_str(&crit),
        stderr_str(&crit)
    );
}

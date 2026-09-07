mod common;

use common::*;
use serde_json::Value;

#[test]
fn diag_reports_scheduler_distribution_fields() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"d1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,"queue_ms":0,"worker_id":"w1"
        }),
        serde_json::json!({
            "execution_id":"d2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"ollama","backend_selected":"ollama","capture_provider":"native","execution_mode":"lean",
            "duration_ms":12,"schema_enforced":false,"schema_valid":true,"queue_ms":900,"worker_id":"w2"
        }),
        serde_json::json!({
            "execution_id":"d3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":14,"schema_enforced":false,"schema_valid":true,"queue_ms":1800,"worker_id":"w1"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["diag"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let stdout = stdout_str(&out);
    assert!(stdout.contains("scheduler_window_runs: 3"), "{stdout}");
    assert!(stdout.contains("scheduler_queue_rows: 3"), "{stdout}");
    assert!(stdout.contains("scheduler_workers_seen: w1,w2"), "{stdout}");
    assert!(
        stdout.contains("scheduler_worker_distribution: w1=2,w2=1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("scheduler_backend_distribution: ollama=1,primary=2"),
        "{stdout}"
    );
    assert!(
        stdout.contains("scheduler_rows_with_retry_attempt: 0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("scheduler_rows_with_queue_started_at: 0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("scheduler_rows_with_task_started_at: 0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("scheduler_rows_with_task_finished_at: 0"),
        "{stdout}"
    );
    assert!(stdout.contains("retry_rows_with_metadata:"), "{stdout}");
    assert!(stdout.contains("retry_attempt_histogram:"), "{stdout}");
    assert!(stdout.contains("critical_summary_rows:"), "{stdout}");
    assert!(stdout.contains("critical_errors_total:"), "{stdout}");
}

#[test]
fn diag_p7_metrics() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"pm1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean","duration_ms":120,
            "schema_enforced":false,"schema_valid":true,"run_all_mode":"mixed","run_all_failed":0,
            "run_all_failure_pattern":"clean",
            "run_all_recommended_resume_point":"xshelf task run-all --status pending"
        }),
        serde_json::json!({
            "execution_id":"pm2","timestamp":"2026-01-01T00:00:01Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean","duration_ms":140,
            "schema_enforced":false,"schema_valid":true,"run_all_mode":"mixed","run_all_failed":1,
            "run_all_failure_pattern":"retryable_failure",
            "run_all_recommended_resume_point":"xshelf task check --json"
        }),
        serde_json::json!({
            "execution_id":"pm3","timestamp":"2026-01-01T00:00:02Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean","duration_ms":150,
            "schema_enforced":false,"schema_valid":true,"run_all_mode":"mixed","run_all_failed":1,
            "run_all_failure_pattern":"retryable_failure",
            "run_all_recommended_resume_point":"xshelf task check --json"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["diag"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let stdout = stdout_str(&out);
    assert!(stdout.contains("phase7_window_runs: 3"), "{stdout}");
    assert!(
        stdout.contains("phase7_actions_until_resolution: 2"),
        "{stdout}"
    );
    assert!(
        stdout.contains("phase7_repeat_diagnosis_rate: 0.500"),
        "{stdout}"
    );
    assert!(
        stdout.contains("phase7_resume_reuse_rate: 0.500"),
        "{stdout}"
    );
}

#[test]
fn diag_json_reports_scheduler_object() {
    let repo = TempRepo::new("cxrs-it");
    let row = serde_json::json!({
        "execution_id":"dj1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
        "duration_ms":10,"schema_enforced":false,"schema_valid":true,
        "queue_ms":500,"worker_id":"w1","retry_attempt":2,
        "queue_started_at":"2026-01-01T00:00:00Z","task_started_at":"2026-01-01T00:00:01Z","task_finished_at":"2026-01-01T00:00:02Z"
    });
    write_runs_log_row(&repo, &row);

    let out = repo.run(&["diag", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    assert_eq!(v.get("backend").and_then(Value::as_str), Some("primary"));
    assert_eq!(
        v.get("scheduler_window_requested").and_then(Value::as_u64),
        Some(200)
    );
    let scheduler = v.get("scheduler").expect("scheduler");
    assert_eq!(scheduler.get("queue_rows").and_then(Value::as_u64), Some(1));
    assert_eq!(
        scheduler
            .get("rows_with_retry_attempt")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        scheduler
            .get("rows_with_queue_started_at")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        scheduler
            .get("rows_with_task_started_at")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        scheduler
            .get("rows_with_task_finished_at")
            .and_then(Value::as_u64),
        Some(1)
    );
    let worker_dist = scheduler
        .get("worker_distribution")
        .and_then(Value::as_object)
        .expect("worker distribution");
    assert_eq!(
        worker_dist.get("w1").and_then(Value::as_u64),
        Some(1),
        "unexpected scheduler object: {scheduler}"
    );
    let retry = v.get("retry").expect("retry");
    assert_eq!(retry.get("window_runs").and_then(Value::as_u64), Some(1));
    assert!(
        retry.get("attempt_histogram").is_some(),
        "unexpected retry object: {retry}"
    );
    let critical = v.get("critical").expect("critical");
    assert_eq!(
        critical.get("summary_rows").and_then(Value::as_u64),
        Some(0),
        "unexpected critical object: {critical}"
    );
    let task_readiness = v.get("task_readiness").expect("task_readiness");
    assert_eq!(
        task_readiness.get("status_filter").and_then(Value::as_str),
        Some("pending")
    );
}

#[test]
fn diag_json_window_scopes_scheduler_rows() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=3u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("dw{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":i * 100,"worker_id":"w1"
        }));
    }
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["diag", "--json", "--window", "1"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    assert_eq!(
        v.get("scheduler_window_requested").and_then(Value::as_u64),
        Some(1)
    );
    let scheduler = v.get("scheduler").expect("scheduler");
    assert_eq!(
        scheduler.get("window_runs").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(scheduler.get("queue_rows").and_then(Value::as_u64), Some(1));
}

#[test]
fn diag_json_reports_run_all_critical_telemetry() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"dc1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":120,"schema_enforced":false,"schema_valid":true,
            "run_all_mode":"mixed","halt_on_critical":true,
            "run_all_scheduled":3,"run_all_complete":1,"run_all_failed":1,"run_all_critical_errors":1,
            "run_all_halted_remaining":1,
            "run_all_backend_fallback_rows":2,
            "run_all_backend_fallbacks":"primary->ollama=2"
        }),
        serde_json::json!({
            "execution_id":"dc2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["diag", "--json", "--strict", "--window", "5"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict warning on critical halt telemetry; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    let critical = v.get("critical").expect("critical");
    assert_eq!(
        critical.get("summary_rows").and_then(Value::as_u64),
        Some(1),
        "unexpected critical object: {critical}"
    );
    assert_eq!(
        critical.get("halted_rows").and_then(Value::as_u64),
        Some(1),
        "unexpected critical object: {critical}"
    );
    let reasons = v
        .get("severity_reasons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| x.as_str().map(ToOwned::to_owned))
        .collect::<Vec<String>>()
        .join(",");
    assert!(
        reasons.contains("critical_halts_detected"),
        "expected critical_halts_detected reason, got: {reasons}"
    );
    let observed = v
        .get("concurrency")
        .and_then(|x| x.get("observed"))
        .expect("concurrency.observed");
    assert_eq!(
        observed
            .get("halted_remaining_total")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        observed
            .get("backend_fallback_rows")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        observed
            .get("latest_backend_fallbacks")
            .and_then(Value::as_str),
        Some("primary->ollama=2")
    );
}

#[test]
fn scheduler_json_strict_reports_severity() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=4u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("sch{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":2500 + i * 10,"worker_id":"w1"
        }));
    }
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["scheduler", "--json", "--strict", "--window", "4"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict scheduler failure; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("scheduler json");
    assert_eq!(
        v.get("scheduler_window_requested").and_then(Value::as_u64),
        Some(4)
    );
    assert_ne!(v.get("severity").and_then(Value::as_str), Some("ok"));
}

#[test]
fn scheduler_json_strict_flags_critical_halts() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"schc1","timestamp":"2026-01-01T00:00:00Z","command":"cxtask_runall","tool":"cxtask_runall",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":120,"schema_enforced":false,"schema_valid":true,
            "run_all_mode":"mixed","halt_on_critical":true,
            "run_all_scheduled":3,"run_all_complete":1,"run_all_failed":1,"run_all_critical_errors":1,
            "run_all_halted_remaining":1,
            "run_all_backend_fallback_rows":2,
            "run_all_backend_fallbacks":"primary->ollama=2"
        }),
        serde_json::json!({
            "execution_id":"schc2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,"queue_ms":100,"worker_id":"w1"
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["scheduler", "--json", "--strict", "--window", "5"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected strict scheduler failure on critical halt; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("scheduler json");
    let reasons = v
        .get("severity_reasons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| x.as_str().map(ToOwned::to_owned))
        .collect::<Vec<String>>()
        .join(",");
    assert!(
        reasons.contains("critical_halts_detected"),
        "expected critical_halts_detected reason, got: {reasons}"
    );
    let observed = v
        .get("concurrency")
        .and_then(|x| x.get("observed"))
        .expect("concurrency.observed");
    assert_eq!(
        observed
            .get("latest_halted_remaining")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        observed
            .get("latest_backend_fallbacks")
            .and_then(Value::as_str),
        Some("primary->ollama=2")
    );
}

#[test]
fn scheduler_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let mut rows = Vec::new();
    for i in 1..=2u64 {
        rows.push(serde_json::json!({
            "execution_id":format!("schfx{i}"),"timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","backend_selected":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10 + i,"schema_enforced":false,"schema_valid":true,"queue_ms":i * 100,"worker_id":"w1"
        }));
    }
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["scheduler", "--json", "--window", "2"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("scheduler json");
    let fixture = load_fixture_json("scheduler_json_contract.json");

    assert_fixture_contract(
        &payload,
        &fixture,
        "top_level_keys",
        &[
            ("scheduler", "scheduler_keys", "scheduler.scheduler"),
            (
                "phase7_metrics",
                "phase7_metrics_keys",
                "scheduler.phase7_metrics",
            ),
            (
                "task_readiness",
                "task_readiness_keys",
                "scheduler.task_readiness",
            ),
            (
                "task_execution",
                "task_execution_keys",
                "scheduler.task_execution",
            ),
            ("retry", "retry_keys", "scheduler.retry"),
            ("critical", "critical_keys", "scheduler.critical"),
            ("concurrency", "concurrency_keys", "scheduler.concurrency"),
        ],
    );
    let backend_caps_keys = fixture_keys(&fixture, "backend_capabilities_keys");
    assert_has_keys(
        payload
            .get("backend_capabilities")
            .expect("backend_capabilities"),
        &backend_caps_keys,
        "scheduler.backend_capabilities",
    );
    let turboquant_keys = fixture_keys(&fixture, "backend_capabilities_turboquant_keys");
    assert_has_keys(
        payload
            .get("backend_capabilities")
            .and_then(|v| v.get("turboquant"))
            .expect("backend_capabilities.turboquant"),
        &turboquant_keys,
        "scheduler.backend_capabilities.turboquant",
    );
    let runtime_keys = fixture_keys(&fixture, "backend_capabilities_runtime_keys");
    assert_has_keys(
        payload
            .get("backend_capabilities")
            .and_then(|v| v.get("runtime"))
            .expect("backend_capabilities.runtime"),
        &runtime_keys,
        "scheduler.backend_capabilities.runtime",
    );
    let adapter_policy_keys = fixture_keys(&fixture, "adapter_rollout_policy_keys");
    assert_has_keys(
        payload
            .get("adapter_rollout_policy")
            .expect("adapter_rollout_policy"),
        &adapter_policy_keys,
        "scheduler.adapter_rollout_policy",
    );
    let task_execution = payload.get("task_execution").expect("task_execution");
    let task_execution_wave_keys = fixture_keys(&fixture, "task_execution_wave_pressure_keys");
    assert_has_keys(
        task_execution
            .get("wave_pressure")
            .expect("task_execution.wave_pressure"),
        &task_execution_wave_keys,
        "scheduler.task_execution.wave_pressure",
    );
    let task_execution_next_keys = fixture_keys(&fixture, "task_execution_next_action_keys");
    assert_has_keys(
        task_execution
            .get("next_action")
            .expect("task_execution.next_action"),
        &task_execution_next_keys,
        "scheduler.task_execution.next_action",
    );
    let task_execution_context_keys = fixture_keys(&fixture, "task_execution_recent_context_keys");
    assert_has_keys(
        task_execution
            .get("recent_context")
            .expect("task_execution.recent_context"),
        &task_execution_context_keys,
        "scheduler.task_execution.recent_context",
    );
    let task_execution_bias_keys = fixture_keys(&fixture, "task_execution_phase7_bias_keys");
    assert_has_keys(
        task_execution
            .get("phase7_bias")
            .expect("task_execution.phase7_bias"),
        &task_execution_bias_keys,
        "scheduler.task_execution.phase7_bias",
    );
    let task_execution_concurrency_keys = fixture_keys(&fixture, "task_execution_concurrency_keys");
    assert_has_keys(
        task_execution
            .get("concurrency")
            .expect("task_execution.concurrency"),
        &task_execution_concurrency_keys,
        "scheduler.task_execution.concurrency",
    );
    let task_execution_invariants_keys = fixture_keys(&fixture, "task_execution_invariants_keys");
    assert_has_keys(
        task_execution
            .get("invariants")
            .expect("task_execution.invariants"),
        &task_execution_invariants_keys,
        "scheduler.task_execution.invariants",
    );
    let task_execution_gate_keys = fixture_keys(&fixture, "task_execution_reasoning_gate_keys");
    assert_has_keys(
        task_execution
            .get("reasoning_gate")
            .expect("task_execution.reasoning_gate"),
        &task_execution_gate_keys,
        "scheduler.task_execution.reasoning_gate",
    );

    let concurrency = payload.get("concurrency").expect("scheduler.concurrency");
    let defaults = concurrency
        .get("defaults")
        .expect("scheduler.concurrency.defaults");
    let observed = concurrency
        .get("observed")
        .expect("scheduler.concurrency.observed");
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
            "scheduler.concurrency.defaults missing key '{key}' in {defaults}"
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
            "scheduler.concurrency.observed missing key '{key}' in {observed}"
        );
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
fn diag_wave_observed() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"wv1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":10,"schema_enforced":false,"schema_valid":true,
            "task_id":"t1","wave_index":1,"wave_mode":"parallel","wave_size":2,"queue_ms":120
        }),
        serde_json::json!({
            "execution_id":"wv2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":12,"schema_enforced":false,"schema_valid":true,
            "task_id":"t2","wave_index":1,"wave_mode":"parallel","wave_size":2,"queue_ms":180
        }),
        serde_json::json!({
            "execution_id":"wv3","timestamp":"2026-01-01T00:00:02Z","command":"cxo","tool":"cxo",
            "backend_used":"primary","capture_provider":"native","execution_mode":"lean",
            "duration_ms":14,"schema_enforced":false,"schema_valid":true,
            "task_id":"t3","wave_index":2,"wave_mode":"sequential","wave_size":1,"queue_ms":260
        }),
    ];
    write_runs_log_rows(&repo, &rows);

    let out = repo.run(&["diag", "--json", "--window", "5"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let v: Value = serde_json::from_str(&stdout_str(&out)).expect("diag json");
    let observed = v
        .get("concurrency")
        .and_then(|x| x.get("observed"))
        .expect("concurrency.observed");
    assert_eq!(
        observed.get("wave_task_rows").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        observed.get("latest_wave_index").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        observed.get("latest_wave_mode").and_then(Value::as_str),
        Some("sequential")
    );
    assert_eq!(
        observed.get("latest_wave_size").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        observed.get("largest_wave_index").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        observed.get("largest_wave_size").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        observed.get("max_queue_wave_index").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        observed.get("max_queue_wave_ms").and_then(Value::as_u64),
        Some(260)
    );
}

mod common;

use common::*;
use serde_json::Value;

#[test]
fn optimize_json_matches_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let row = serde_json::json!({
        "execution_id":"ofx1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
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
}

#[test]
fn optimize_json_actions_match_contract_fixture() {
    let repo = TempRepo::new("cxrs-it");
    let rows = vec![
        serde_json::json!({
            "execution_id":"oact1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
            "duration_ms":5000,"schema_enforced":true,"schema_valid":false,
            "input_tokens":1000,"cached_input_tokens":10
        }),
        serde_json::json!({
            "execution_id":"oact2","timestamp":"2026-01-01T00:00:01Z","command":"cxo","tool":"cxo",
            "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
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
fn optimize_actions_strict_severity_gate_is_deterministic() {
    let repo = TempRepo::new("cxrs-it");
    let row = serde_json::json!({
        "execution_id":"ogate1","timestamp":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo",
        "backend_used":"codex","capture_provider":"native","execution_mode":"lean",
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

mod common;

use common::*;
use serde_json::Value;

#[test]
fn broker_show_contract() {
    let repo = TempRepo::new("cxrs-it");
    let out = repo.run(&["broker", "show", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let payload: Value = serde_json::from_str(&stdout_str(&out)).expect("broker show json");
    let fixture = load_fixture_json("broker_show_contract.json");

    let top_keys = fixture_keys(&fixture, "top_level_keys");
    assert_has_keys(&payload, &top_keys, "broker.show");

    let availability_keys = fixture_keys(&fixture, "availability_keys");
    assert_has_keys(
        payload.get("availability").expect("availability"),
        &availability_keys,
        "broker.show.availability",
    );

    let policy_keys = fixture_keys(&fixture, "adapter_rollout_policy_keys");
    assert_has_keys(
        payload
            .get("adapter_rollout_policy")
            .expect("adapter_rollout_policy"),
        &policy_keys,
        "broker.show.adapter_rollout_policy",
    );
}

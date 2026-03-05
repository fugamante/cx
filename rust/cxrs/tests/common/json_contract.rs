use serde_json::Value;
use std::fs;
use std::path::PathBuf;

pub fn load_fixture_json(name: &str) -> Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push(name);
    let content = fs::read_to_string(path).expect("read fixture file");
    serde_json::from_str(&content).expect("parse fixture json")
}

pub fn fixture_keys(fixture: &Value, key: &str) -> Vec<String> {
    fixture
        .get(key)
        .and_then(Value::as_array)
        .expect("fixture key array")
        .iter()
        .map(|v| v.as_str().expect("fixture key string").to_string())
        .collect()
}

pub fn assert_has_keys(obj: &Value, keys: &[String], context: &str) {
    for key in keys {
        assert!(
            obj.get(key).is_some(),
            "{context} missing key '{key}' in payload: {obj}"
        );
    }
}

pub fn assert_fixture_contract(
    payload: &Value,
    fixture: &Value,
    top_level_key_field: &str,
    sections: &[(&str, &str, &str)],
) {
    let top_keys = fixture_keys(fixture, top_level_key_field);
    assert_has_keys(payload, &top_keys, "contract.top");
    for (payload_section, fixture_keys_field, context) in sections {
        let keys = fixture_keys(fixture, fixture_keys_field);
        assert_has_keys(
            payload.get(payload_section).expect("fixture section"),
            &keys,
            context,
        );
    }
}

pub fn assert_actions_contract(payload: &Value) {
    let fixture = load_fixture_json("actions_json_contract.json");
    let keys = fixture_keys(&fixture, "action_keys");
    let actions = payload
        .get("actions")
        .and_then(Value::as_array)
        .expect("actions array");
    assert!(!actions.is_empty(), "expected non-empty actions payload");
    for action in actions {
        assert_has_keys(action, &keys, "actions.item");
        let sev = action
            .get("severity")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        assert!(
            matches!(sev, "warning" | "critical"),
            "unexpected action severity: {sev}"
        );
    }
}

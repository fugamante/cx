use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use crate::contract_versions::{
    ACTIONS_JSON_CONTRACT_VERSION, BROKER_BENCHMARK_JSON_CONTRACT_VERSION,
    BROKER_SHOW_JSON_CONTRACT_VERSION, DIAG_JSON_CONTRACT_VERSION,
    LLM_RESIDENT_JSON_CONTRACT_VERSION, LLM_VERIFY_JSON_CONTRACT_VERSION,
    OPTIMIZE_JSON_CONTRACT_VERSION, POLICY_SHOW_JSON_CONTRACT_VERSION,
    SCHEDULER_JSON_CONTRACT_VERSION, TASK_CHECK_JSON_CONTRACT_VERSION,
    TASK_LIST_JSON_CONTRACT_VERSION, TASK_RUN_ALL_JSON_CONTRACT_VERSION,
    TASK_RUN_JSON_CONTRACT_VERSION, TASK_RUN_PLAN_JSON_CONTRACT_VERSION,
    TASK_SHOW_JSON_CONTRACT_VERSION, TELEMETRY_JSON_CONTRACT_VERSION,
};
use crate::execmeta::utc_now_iso;

#[derive(Clone, Copy)]
struct ContractSpec {
    action: &'static str,
    contract_version: &'static str,
    required_keys: &'static [&'static str],
}

const DIAG_REQUIRED_KEYS: &[&str] = &["contract_version", "retry", "concurrency"];
const SCHED_REQUIRED_KEYS: &[&str] = &["contract_version", "retry", "concurrency"];
const OPT_REQUIRED_KEYS: &[&str] = &["contract_version", "scoreboard"];
const TELE_REQUIRED_KEYS: &[&str] = &["contract_version", "fields", "contract_drift"];
const BROKER_SHOW_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "broker_policy",
    "active_backend",
    "active_model",
    "availability",
    "adapter_rollout_policy",
];
const BROKER_BENCH_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "window",
    "log_file",
    "summary",
    "strict",
    "min_runs",
    "severity",
    "violations",
    "violation_counts",
];
const POLICY_SHOW_REQUIRED_KEYS: &[&str] = &["contract_version", "rules", "overrides"];
const TASK_CHECK_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "status_filter",
    "selected",
    "waves",
    "blocked_total",
    "can_run",
    "recommended_mode",
];
const TASK_RUN_ALL_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "status_filter",
    "mode",
    "task_readiness",
    "preflight",
    "scheduled",
    "tasks",
];
const TASK_LIST_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "status_filter",
    "count",
    "list_readiness",
    "tasks",
];
const TASK_SHOW_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "id",
    "status",
    "latest_run",
    "run_readiness",
];
const TASK_RUN_REQUIRED_KEYS: &[&str] = &["contract_version", "task_id", "status", "execution_id"];
const TASK_RUN_PLAN_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "status_filter",
    "requested_mode",
    "strict_plan",
    "strict_plan_ok",
    "can_execute",
    "wave_count",
    "selected",
    "waves",
    "blocked",
];
const LLM_VERIFY_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "timestamp",
    "backend",
    "profile",
    "context_target",
    "result",
];
const LLM_RESIDENT_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "timestamp",
    "selected_adapter",
    "selected_transport",
    "http_request_profile",
    "boundary",
    "runtime_capability",
];

const FULL_SPECS: &[ContractSpec] = &[
    ContractSpec {
        action: "cx.diag",
        contract_version: DIAG_JSON_CONTRACT_VERSION,
        required_keys: DIAG_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.scheduler",
        contract_version: SCHEDULER_JSON_CONTRACT_VERSION,
        required_keys: SCHED_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.optimize",
        contract_version: OPTIMIZE_JSON_CONTRACT_VERSION,
        required_keys: OPT_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.logs.stats",
        contract_version: TELEMETRY_JSON_CONTRACT_VERSION,
        required_keys: TELE_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.broker.show",
        contract_version: BROKER_SHOW_JSON_CONTRACT_VERSION,
        required_keys: BROKER_SHOW_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.broker.benchmark",
        contract_version: BROKER_BENCHMARK_JSON_CONTRACT_VERSION,
        required_keys: BROKER_BENCH_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.policy.show",
        contract_version: POLICY_SHOW_JSON_CONTRACT_VERSION,
        required_keys: POLICY_SHOW_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.task.check",
        contract_version: TASK_CHECK_JSON_CONTRACT_VERSION,
        required_keys: TASK_CHECK_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.task.run_all",
        contract_version: TASK_RUN_ALL_JSON_CONTRACT_VERSION,
        required_keys: TASK_RUN_ALL_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.task.list",
        contract_version: TASK_LIST_JSON_CONTRACT_VERSION,
        required_keys: TASK_LIST_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.task.show",
        contract_version: TASK_SHOW_JSON_CONTRACT_VERSION,
        required_keys: TASK_SHOW_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.actions",
        contract_version: ACTIONS_JSON_CONTRACT_VERSION,
        required_keys: &["actions_contract_version", "actions"],
    },
    ContractSpec {
        action: "cx.task.run",
        contract_version: TASK_RUN_JSON_CONTRACT_VERSION,
        required_keys: TASK_RUN_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.task.run_plan",
        contract_version: TASK_RUN_PLAN_JSON_CONTRACT_VERSION,
        required_keys: TASK_RUN_PLAN_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.llm.verify",
        contract_version: LLM_VERIFY_JSON_CONTRACT_VERSION,
        required_keys: LLM_VERIFY_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.llm.resident",
        contract_version: LLM_RESIDENT_JSON_CONTRACT_VERSION,
        required_keys: LLM_RESIDENT_REQUIRED_KEYS,
    },
];

const EVAL_LAB_ACTIONS: &[&str] = &[
    "cx.diag",
    "cx.scheduler",
    "cx.optimize",
    "cx.logs.stats",
    "cx.task.check",
    "cx.task.run_all",
    "cx.task.list",
    "cx.task.show",
    "cx.task.run",
];

fn profile_specs(profile: &str) -> Option<Vec<ContractSpec>> {
    match profile {
        "full" => Some(FULL_SPECS.to_vec()),
        "eval-lab" => Some(
            FULL_SPECS
                .iter()
                .copied()
                .filter(|spec| EVAL_LAB_ACTIONS.contains(&spec.action))
                .collect(),
        ),
        _ => None,
    }
}

fn contracts_object(specs: &[ContractSpec]) -> Value {
    let mut contracts = Map::new();
    for spec in specs {
        contracts.insert(
            spec.action.to_string(),
            json!({
                "contract_version": spec.contract_version,
                "required_keys": spec.required_keys,
            }),
        );
    }
    Value::Object(contracts)
}

const BUNDLE_VERSION: &str = "cx-contract-bundle.v1";

fn manifest_value(profile: &str, specs: &[ContractSpec]) -> Value {
    json!({
        "bundle_version": BUNDLE_VERSION,
        "profile": profile,
        "contracts": contracts_object(specs)
    })
}

fn bundle_value(app_version: &str, profile: &str, specs: &[ContractSpec]) -> Value {
    let mut bundle = manifest_value(profile, specs);
    if let Some(obj) = bundle.as_object_mut() {
        obj.insert(
            "source_version".to_string(),
            Value::String(app_version.to_string()),
        );
        obj.insert("generated_at".to_string(), Value::String(utc_now_iso()));
    }
    bundle
}

fn fixture_path(profile: &str) -> Option<PathBuf> {
    match profile {
        "eval-lab" => {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.push("tests");
            path.push("fixtures");
            path.push("eval_lab_bundle.json");
            Some(path)
        }
        _ => None,
    }
}

fn load_fixture_manifest(profile: &str) -> Result<Value, String> {
    let Some(path) = fixture_path(profile) else {
        return Err(format!(
            "no fixture-backed contract bundle for profile '{profile}'"
        ));
    };
    let content =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    serde_json::from_str(&content).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

fn diff_string_arrays(left: &[String], right: &[String]) -> Vec<String> {
    let left_set: BTreeSet<&str> = left.iter().map(String::as_str).collect();
    let right_set: BTreeSet<&str> = right.iter().map(String::as_str).collect();
    left_set
        .difference(&right_set)
        .map(|item| item.to_string())
        .collect()
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn validate_manifest(profile: &str, specs: &[ContractSpec], fixture: &Value) -> Value {
    let manifest = manifest_value(profile, specs);
    let current_contracts = manifest
        .get("contracts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let fixture_contracts = fixture
        .get("contracts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let current_actions: Vec<String> = current_contracts.keys().cloned().collect();
    let fixture_actions: Vec<String> = fixture_contracts.keys().cloned().collect();

    let missing_actions = diff_string_arrays(&fixture_actions, &current_actions);
    let new_actions = diff_string_arrays(&current_actions, &fixture_actions);
    let mut changed_contract_versions = Vec::new();
    let mut changed_required_keys = Vec::new();

    for action in current_actions
        .iter()
        .filter(|action| fixture_contracts.contains_key(*action))
    {
        let current = current_contracts
            .get(action)
            .cloned()
            .unwrap_or(Value::Null);
        let prior = fixture_contracts
            .get(action)
            .cloned()
            .unwrap_or(Value::Null);
        if current.get("contract_version").and_then(Value::as_str)
            != prior.get("contract_version").and_then(Value::as_str)
        {
            changed_contract_versions.push(action.to_string());
        }
        let current_required = string_array(current.get("required_keys"));
        let prior_required = string_array(prior.get("required_keys"));
        if current_required != prior_required {
            changed_required_keys.push(action.to_string());
        }
    }

    let ok = missing_actions.is_empty()
        && new_actions.is_empty()
        && changed_contract_versions.is_empty()
        && changed_required_keys.is_empty();

    json!({
        "ok": ok,
        "bundle_version": BUNDLE_VERSION,
        "profile": profile,
        "drift": {
            "missing_actions": missing_actions,
            "new_actions": new_actions,
            "changed_contract_versions": changed_contract_versions,
            "changed_required_keys": changed_required_keys
        }
    })
}

pub fn cmd_contracts(app_name: &str, app_version: &str, args: &[String]) -> i32 {
    let usage =
        format!("Usage: {app_name} contracts <export|validate> [--profile eval-lab|full] [--json]");
    let mut sub = "export";
    let mut profile = "full";
    let mut json_out = false;
    let mut i = 0usize;
    if let Some(v) = args.first().map(String::as_str)
        && !v.starts_with("--")
    {
        sub = v;
        i = 1;
    }
    if !matches!(sub, "export" | "validate") {
        crate::cx_eprintln!("{usage}");
        return 2;
    }
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_out = true;
                i += 1;
            }
            "--profile" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return 2;
                };
                profile = v;
                i += 2;
            }
            other => {
                crate::cx_eprintln!("cxrs contracts: unknown flag '{other}'");
                crate::cx_eprintln!("{usage}");
                return 2;
            }
        }
    }

    let Some(specs) = profile_specs(profile) else {
        crate::cx_eprintln!("cxrs contracts: invalid profile '{profile}'");
        crate::cx_eprintln!("{usage}");
        return 2;
    };

    if sub == "validate" {
        let fixture = match load_fixture_manifest(profile) {
            Ok(value) => value,
            Err(err) => {
                crate::cx_eprintln!("cxrs contracts: {err}");
                return 2;
            }
        };
        let result = validate_manifest(profile, &specs, &fixture);
        if json_out {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            );
        } else {
            let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
            println!("bundle_version: {BUNDLE_VERSION}");
            println!("profile: {profile}");
            println!("ok: {ok}");
            let drift = result.get("drift").cloned().unwrap_or_else(|| json!({}));
            for key in [
                "missing_actions",
                "new_actions",
                "changed_contract_versions",
                "changed_required_keys",
            ] {
                let items = string_array(drift.get(key));
                println!("{key}: {}", items.len());
                for item in items {
                    println!("- {item}");
                }
            }
        }
        return if result.get("ok").and_then(Value::as_bool) == Some(true) {
            0
        } else {
            1
        };
    }

    let bundle = bundle_value(app_version, profile, &specs);
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&bundle).unwrap_or_else(|_| bundle.to_string())
        );
        return 0;
    }

    let contracts = bundle
        .get("contracts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    println!("bundle_version: {BUNDLE_VERSION}");
    println!("source_version: {app_version}");
    println!("profile: {profile}");
    println!("contract_count: {}", contracts.len());
    for (action, spec) in contracts {
        let version = spec
            .get("contract_version")
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        println!("- {action} [{version}]");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::{bundle_value, load_fixture_manifest, manifest_value, validate_manifest};
    use crate::contract_versions::{
        LLM_RESIDENT_JSON_CONTRACT_VERSION, POLICY_SHOW_JSON_CONTRACT_VERSION,
        TASK_RUN_JSON_CONTRACT_VERSION, TASK_RUN_PLAN_JSON_CONTRACT_VERSION,
    };

    #[test]
    fn eval_bundle_ok() {
        let bundle = bundle_value(
            "0.0.0-test",
            "eval-lab",
            &super::profile_specs("eval-lab").unwrap(),
        );
        let contracts = bundle
            .get("contracts")
            .and_then(serde_json::Value::as_object)
            .unwrap();
        assert!(contracts.contains_key("cx.diag"));
        assert!(contracts.contains_key("cx.scheduler"));
        assert!(contracts.contains_key("cx.task.run_all"));
        assert!(contracts.contains_key("cx.task.run"));
        assert_eq!(
            contracts["cx.task.run"]["contract_version"].as_str(),
            Some(TASK_RUN_JSON_CONTRACT_VERSION)
        );
    }

    #[test]
    fn bundle_includes_contracts() {
        let bundle = bundle_value("0.0.0-test", "full", &super::profile_specs("full").unwrap());
        let contracts = bundle
            .get("contracts")
            .and_then(serde_json::Value::as_object)
            .unwrap();
        assert_eq!(
            contracts["cx.policy.show"]["contract_version"].as_str(),
            Some(POLICY_SHOW_JSON_CONTRACT_VERSION)
        );
        assert_eq!(
            contracts["cx.task.run_plan"]["contract_version"].as_str(),
            Some(TASK_RUN_PLAN_JSON_CONTRACT_VERSION)
        );
        assert_eq!(
            contracts["cx.llm.resident"]["contract_version"].as_str(),
            Some(LLM_RESIDENT_JSON_CONTRACT_VERSION)
        );
        assert!(contracts.contains_key("cx.broker.benchmark"));
        assert!(contracts.contains_key("cx.llm.verify"));
    }

    #[test]
    fn eval_fixture_ok() {
        let fixture = load_fixture_manifest("eval-lab").expect("eval-lab fixture");
        let manifest = manifest_value("eval-lab", &super::profile_specs("eval-lab").unwrap());
        assert_eq!(manifest, fixture);
    }

    #[test]
    fn eval_validate_ok() {
        let fixture = load_fixture_manifest("eval-lab").expect("eval-lab fixture");
        let result = validate_manifest(
            "eval-lab",
            &super::profile_specs("eval-lab").unwrap(),
            &fixture,
        );
        assert_eq!(
            result.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            result
                .get("drift")
                .and_then(|v| v.get("missing_actions"))
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(0)
        );
    }
}

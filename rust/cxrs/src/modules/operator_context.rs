use serde_json::json;

pub fn operator_value() -> serde_json::Value {
    json!({
        "project_name": "XSHELF",
        "formerly": "CX",
        "canonical_command": "xshelf",
        "aliases": ["xs", "cx"],
        "repo_identity_first": true,
        "first_checks": [
            "./bin/xshelf version",
            "./bin/xshelf core --json",
            "./bin/xshelf task check --json",
            "./bin/xshelf doctor"
        ],
        "policy_docs": [
            "README.md",
            "CONTRIBUTING.md",
            "docs/project/README.md"
        ],
        "guidance": "Start from local XSHELF runtime state before external lookup."
    })
}

pub fn operator_lines() -> Vec<String> {
    let checks = [
        "./bin/xshelf version",
        "./bin/xshelf core --json",
        "./bin/xshelf task check --json",
        "./bin/xshelf doctor",
    ];
    vec![
        "operator_context.project: XSHELF (formerly CX)".to_string(),
        "operator_context.canonical_command: xshelf".to_string(),
        "operator_context.aliases: xs,cx".to_string(),
        format!("operator_context.first_checks: {}", checks.join(" | ")),
        "operator_context.guidance: Start from local XSHELF runtime state before external lookup."
            .to_string(),
    ]
}

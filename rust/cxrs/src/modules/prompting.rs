use std::collections::HashMap;

use crate::logs::load_runs;
use crate::paths::resolve_log_file;

fn print_roles() -> i32 {
    println!("== cxrs roles ==");
    println!("architect   Define approach, boundaries, and tradeoffs.");
    println!("implementer Apply focused code changes with minimal blast radius.");
    println!("reviewer    Validate regressions, risks, and missing tests.");
    println!("tester      Design and run deterministic checks.");
    println!("doc         Produce concise operator-facing documentation.");
    0
}

fn role_header(role: &str) -> Option<&'static str> {
    match role {
        "architect" => Some(
            "Role: architect\nFocus: design and decomposition.\nDeliver: implementation plan, constraints, and acceptance checks.",
        ),
        "implementer" => Some(
            "Role: implementer\nFocus: minimal cohesive code change.\nDeliver: patch summary and verification commands.",
        ),
        "reviewer" => Some(
            "Role: reviewer\nFocus: bugs, regressions, and safety.\nDeliver: findings ordered by severity with file references.",
        ),
        "tester" => Some(
            "Role: tester\nFocus: deterministic validation.\nDeliver: test matrix, observed results, and failure triage.",
        ),
        "doc" => Some(
            "Role: doc\nFocus: user/operator clarity.\nDeliver: concise docs with examples and expected outputs.",
        ),
        _ => None,
    }
}

pub fn cmd_roles(role: Option<&str>) -> i32 {
    if let Some(r) = role {
        let Some(header) = role_header(r) else {
            eprintln!("cxrs roles: unknown role '{r}'");
            return 2;
        };
        println!("{header}");
        return 0;
    }
    print_roles()
}

pub fn cmd_prompt(mode: &str, request: &str) -> i32 {
    let valid = ["implement", "fix", "test", "doc", "ops"];
    if !valid.contains(&mode) {
        eprintln!("cxrs prompt: invalid mode '{mode}' (use implement|fix|test|doc|ops)");
        return 2;
    }
    let mode_goal = match mode {
        "implement" => "Implement the requested behavior with minimal risk and clear verification.",
        "fix" => "Diagnose and fix the issue with root-cause focus and regression prevention.",
        "test" => "Design and execute deterministic tests that validate behavior and edge cases.",
        "doc" => "Produce concise, accurate documentation aligned with current implementation.",
        "ops" => "Perform operational changes safely, with rollback and observability guidance.",
        _ => "",
    };
    println!("You are working on the \"cx\" toolchain.");
    println!();
    println!("Context:");
    println!("- Repo canonical implementation is the source of truth.");
    println!("- Keep behavior deterministic and non-interactive.");
    println!("- Do not contaminate stdout pipelines; diagnostics to stderr.");
    println!();
    println!("Goal:");
    println!("- {}", mode_goal);
    println!("- User request: {request}");
    println!();
    println!("Requirements:");
    println!("- Preserve backward compatibility where feasible.");
    println!("- Make minimal cohesive changes.");
    println!("- Validate structured outputs when JSON is required.");
    println!();
    println!("Constraints:");
    println!("- No automatic commands on shell startup.");
    println!("- No global shell option leakage.");
    println!("- Keep repo-aware logging behavior intact.");
    println!();
    println!("Deliverables:");
    println!("- Code changes with file paths.");
    println!("- Short explanation of what changed and why.");
    println!("- Verification command list.");
    println!();
    println!("Test Checklist:");
    println!("1. Build/check passes.");
    println!("2. Target command behavior matches requirements.");
    println!("3. No pipeline-breaking stdout noise.");
    println!("4. Repo-aware log/state paths still resolve correctly.");
    0
}

pub fn cmd_fanout(objective: &str) -> i32 {
    let tasks = [
        (
            "architect",
            "Define minimal design and split objective into independent slices.",
        ),
        (
            "implementer",
            "Implement slice A with deterministic behavior and tests.",
        ),
        (
            "implementer",
            "Implement slice B with minimal shared-state coupling.",
        ),
        (
            "reviewer",
            "Audit for regressions, safety issues, and schema/pipeline risks.",
        ),
        (
            "tester",
            "Create execution checklist and validate outputs against expectations.",
        ),
        ("doc", "Update operator docs and examples for new behavior."),
    ];
    println!("== cxrs fanout ==");
    println!("objective: {objective}");
    println!();
    for (idx, (role, task)) in tasks.iter().enumerate() {
        println!("### Subtask {}/{} [{}]", idx + 1, tasks.len(), role);
        println!("Goal: {task}");
        println!("Scope: Keep this task independently executable.");
        println!("Deliverables: patch summary + verification commands.");
        println!("Tests: include deterministic checks for this slice.");
        println!();
    }
    0
}

pub fn cmd_promptlint(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("== cxrs promptlint (last {n} runs) ==");
        println!("No runs found.");
        println!("log_file: {}", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs promptlint: {e}");
            return 1;
        }
    };
    if runs.is_empty() {
        println!("== cxrs promptlint (last {n} runs) ==");
        println!("No runs found.");
        println!("log_file: {}", log_file.display());
        return 0;
    }

    let mut tool_eff: HashMap<String, (u64, u64)> = HashMap::new();
    let mut tool_cache: HashMap<String, (u64, u64)> = HashMap::new();
    for r in &runs {
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let eff = r.effective_input_tokens.unwrap_or(0);
        let e = tool_eff.entry(tool.clone()).or_insert((0, 0));
        e.0 += eff;
        e.1 += 1;
        let in_t = r.input_tokens.unwrap_or(0);
        let c_t = r.cached_input_tokens.unwrap_or(0);
        let c = tool_cache.entry(tool).or_insert((0, 0));
        c.0 += c_t;
        c.1 += in_t;
    }

    let mut top_eff: Vec<(String, u64)> = tool_eff
        .iter()
        .map(|(tool, (sum, count))| (tool.clone(), if *count == 0 { 0 } else { sum / count }))
        .collect();
    top_eff.sort_by(|a, b| b.1.cmp(&a.1));
    top_eff.truncate(5);

    let mid = runs.len() / 2;
    let (first, second) = runs.split_at(mid.max(1).min(runs.len()));
    let mut drift_rows: Vec<(String, i64, u64, u64)> = Vec::new();
    let mut tools: Vec<String> = tool_eff.keys().cloned().collect();
    tools.sort();
    tools.dedup();
    for tool in tools {
        let mut f_sum = 0u64;
        let mut f_count = 0u64;
        for r in first {
            if r.tool.as_deref().unwrap_or("unknown") == tool {
                f_sum += r.effective_input_tokens.unwrap_or(0);
                f_count += 1;
            }
        }
        let mut s_sum = 0u64;
        let mut s_count = 0u64;
        for r in second {
            if r.tool.as_deref().unwrap_or("unknown") == tool {
                s_sum += r.effective_input_tokens.unwrap_or(0);
                s_count += 1;
            }
        }
        if f_count > 0 && s_count > 0 {
            let f_avg = f_sum / f_count;
            let s_avg = s_sum / s_count;
            drift_rows.push((tool, s_avg as i64 - f_avg as i64, f_avg, s_avg));
        }
    }
    drift_rows.sort_by(|a, b| b.1.cmp(&a.1));
    drift_rows.truncate(5);

    let mut poor_cache: Vec<(String, u64)> = tool_cache
        .iter()
        .filter_map(|(tool, (cached, input))| {
            if *input == 0 {
                None
            } else {
                Some((
                    tool.clone(),
                    ((*cached as f64 / *input as f64) * 100.0).round() as u64,
                ))
            }
        })
        .collect();
    poor_cache.sort_by(|a, b| a.1.cmp(&b.1));
    poor_cache.truncate(5);

    println!("== cxrs promptlint (last {n} runs) ==");
    println!("Top token-heavy tools (avg effective_input_tokens):");
    if top_eff.is_empty() {
        println!("- n/a");
    } else {
        for (tool, avg) in &top_eff {
            println!("- {tool}: {avg}");
        }
    }

    println!("Prompt drift (same tool, avg eff tokens second-half minus first-half):");
    if drift_rows.is_empty() {
        println!("- n/a");
    } else {
        for (tool, delta, first_avg, second_avg) in &drift_rows {
            println!("- {tool}: delta={delta}, first={first_avg}, second={second_avg}");
        }
    }

    println!("Poor cache-hit tools:");
    if poor_cache.is_empty() {
        println!("- n/a");
    } else {
        for (tool, pct) in &poor_cache {
            println!("- {tool}: {pct}%");
        }
    }

    println!("Recommendations:");
    let mut rec_count = 0usize;
    if let Some((tool, avg)) = top_eff.first()
        && *avg > 3000
    {
        println!(
            "- {tool} prompts are heavy ({avg}); reduce embedded context and enforce schema-only outputs."
        );
        rec_count += 1;
    }
    if let Some((tool, delta, _, _)) = drift_rows.first()
        && *delta > 300
    {
        println!(
            "- {tool} shows token drift (+{delta}); stabilize prompt templates and prompt_preview content."
        );
        rec_count += 1;
    }
    if let Some((tool, pct)) = poor_cache.first()
        && *pct < 40
    {
        println!(
            "- {tool} cache hit is low ({pct}%); reduce prompt variability and reuse stable instruction blocks."
        );
        rec_count += 1;
    }
    if rec_count == 0 {
        println!("- No major prompt issues detected in this window.");
    }
    println!("log_file: {}", log_file.display());
    0
}

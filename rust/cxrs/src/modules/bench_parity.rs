use chrono::Utc;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::bench_parity_mocks::{setup_parity_mocks, with_parity_env};
use crate::bench_parity_support::{
    BenchStats, ParityRow, maybe_collect_tokens, print_bench_summary, print_parity_table,
    run_parity_path, setup_temp_repo,
};
use crate::config::app_config;
use crate::logs::file_len;
use crate::paths::{repo_root_hint, resolve_log_file};
use crate::process::run_command_output_with_timeout;
use crate::routing::{bash_function_names, route_handler_for};

type SchemaKeys = Option<Vec<&'static str>>;
type ParityCase = (&'static str, Vec<&'static str>, SchemaKeys);

fn run_command_for_bench(
    command: &[String],
    disable_cx_log: bool,
    passthru: bool,
) -> Result<i32, String> {
    if command.is_empty() {
        return Err("missing command".to_string());
    }
    let mut cmd = Command::new(&command[0]);
    if command.len() > 1 {
        cmd.args(&command[1..]);
    }
    if disable_cx_log {
        cmd.env("CXLOG_ENABLED", "0");
    }
    let output = run_command_output_with_timeout(cmd, &format!("bench '{}'", command[0]))?;
    if passthru {
        let mut out = std::io::stdout();
        let mut err = std::io::stderr();
        let _ = out.write_all(&output.stdout);
        let _ = err.write_all(&output.stderr);
    }
    Ok(output.status.code().unwrap_or(1))
}

fn validate_bench_args(app_name: &str, runs: usize, command: &[String]) -> Result<(), i32> {
    if runs == 0 {
        crate::cx_eprintln!("cxrs bench: runs must be > 0");
        return Err(2);
    }
    if command.is_empty() {
        crate::cx_eprintln!("Usage: {app_name} bench <runs> -- <command...>");
        return Err(2);
    }
    Ok(())
}

pub fn cmd_bench(app_name: &str, runs: usize, command: &[String]) -> i32 {
    if let Err(code) = validate_bench_args(app_name, runs, command) {
        return code;
    }
    let cfg = app_config();
    let disable_cx_log = !cfg.cxbench_log;
    let passthru = cfg.cxbench_passthru;
    let log_file = resolve_log_file();
    let mut stats = BenchStats {
        durations: Vec::with_capacity(runs),
        ..Default::default()
    };

    for _ in 0..runs {
        let before_offset = log_file
            .as_ref()
            .map(|p| if p.exists() { file_len(p) } else { 0 })
            .unwrap_or(0);
        let started = Instant::now();
        let started_epoch = Utc::now().timestamp();
        let code = match run_command_for_bench(command, disable_cx_log, passthru) {
            Ok(c) => c,
            Err(e) => {
                crate::cx_eprintln!("cxrs bench: {e}");
                return 1;
            }
        };
        let ended_epoch = Utc::now().timestamp();
        stats.durations.push(started.elapsed().as_millis() as u64);
        if code != 0 {
            stats.failures += 1;
        }
        maybe_collect_tokens(
            &mut stats,
            &log_file,
            before_offset,
            started_epoch,
            ended_epoch,
            disable_cx_log,
        );
    }

    print_bench_summary(runs, command, disable_cx_log, passthru, &stats);
    if stats.failures > 0 { 1 } else { 0 }
}

fn parity_overlap(repo: &std::path::Path) -> Vec<ParityCase> {
    let bash_funcs = bash_function_names(repo);
    let parity_catalog: Vec<ParityCase> = vec![
        ("cxo", vec!["echo", "hi"], None),
        ("cx", vec!["echo", "hi"], None),
        ("cxj", vec!["echo", "hi"], None),
        ("cxol", vec!["echo", "hi"], None),
        ("cxcopy", vec!["echo", "hi"], None),
        ("cxnext", vec!["echo", "hi"], None),
        ("cxdiffsum_staged", vec![], None),
        ("cxcommitmsg", vec![], None),
        (
            "cxcommitjson",
            vec![],
            Some(vec!["subject", "body", "breaking", "scope", "tests"]),
        ),
    ];
    parity_catalog
        .into_iter()
        .filter(|(cmd, _, _)| {
            route_handler_for(cmd).is_some() && bash_funcs.iter().any(|f| f == cmd)
        })
        .collect()
}

struct ParityEvalCtx<'a> {
    repo: &'a std::path::Path,
    exe: &'a std::path::Path,
    temp_repo: &'a std::path::Path,
    temp_log_file: &'a std::path::Path,
    budget_chars: usize,
    mock_dir: &'a Path,
}

struct ParityCaseInput<'a> {
    cmd: &'a str,
    args: &'a [&'a str],
    schema_keys: &'a SchemaKeys,
}

fn evaluate_parity_case(ctx: &ParityEvalCtx<'_>, case: ParityCaseInput<'_>) -> ParityRow {
    let mut row = ParityRow {
        cmd: case.cmd.to_string(),
        checked: true,
        ..Default::default()
    };
    run_rust_case(&mut row, ctx, &case);
    run_bash_case(&mut row, ctx, &case);
    row
}

fn run_rust_case(row: &mut ParityRow, ctx: &ParityEvalCtx<'_>, case: &ParityCaseInput<'_>) {
    let before_rust = file_len(ctx.temp_log_file);
    let mut rust_cmd = Command::new(ctx.exe);
    rust_cmd.arg("cx-compat").arg(case.cmd).args(case.args);
    with_parity_env(&mut rust_cmd, ctx.mock_dir, ctx.temp_repo);
    rust_cmd.env("CX_EXECUTION_PATH", "rust:cxparity");
    if let Ok(out) = run_command_output_with_timeout(rust_cmd, "cxparity rust case") {
        let (ok, json_ok, logs_ok, budget_ok) = run_parity_path(
            out,
            ctx.temp_log_file,
            before_rust,
            ctx.budget_chars,
            case.schema_keys,
        );
        row.rust_ok = ok;
        row.json_ok = json_ok;
        row.logs_ok = logs_ok;
        row.budget_ok = budget_ok;
    }
}

fn run_bash_case(row: &mut ParityRow, ctx: &ParityEvalCtx<'_>, case: &ParityCaseInput<'_>) {
    let before_bash = file_len(ctx.temp_log_file);
    let bash_cmd = format!(
        "source '{}' >/dev/null 2>&1; {} {}",
        ctx.repo.join("cx.sh").display(),
        case.cmd,
        case.args.join(" ")
    );
    let mut bash_proc = Command::new("bash");
    bash_proc.arg("-lc").arg(bash_cmd);
    with_parity_env(&mut bash_proc, ctx.mock_dir, ctx.temp_repo);
    bash_proc.env("CX_EXECUTION_PATH", "bash:cxparity");
    if let Ok(out) = run_command_output_with_timeout(bash_proc, "cxparity bash case") {
        let (ok, json_ok, logs_ok, budget_ok) = run_parity_path(
            out,
            ctx.temp_log_file,
            before_bash,
            ctx.budget_chars,
            case.schema_keys,
        );
        row.bash_ok = ok;
        row.json_ok = row.json_ok && json_ok;
        row.logs_ok = row.logs_ok && logs_ok;
        row.budget_ok = row.budget_ok && budget_ok;
    }
}

fn resolve_parity_context() -> Result<(PathBuf, std::path::PathBuf, usize), i32> {
    let repo = repo_root_hint().unwrap_or_else(|| PathBuf::from("."));
    let exe = env::current_exe().map_err(|e| {
        crate::cx_eprintln!("cxparity: cannot resolve current executable: {e}");
        1
    })?;
    Ok((repo, exe, app_config().budget_chars))
}

fn parity_rows(
    overlap: Vec<ParityCase>,
    repo: &Path,
    exe: &Path,
    temp_repo: &Path,
    temp_log_file: &Path,
    budget_chars: usize,
    mock_dir: &Path,
) -> (Vec<ParityRow>, bool) {
    let ctx = ParityEvalCtx {
        repo,
        exe,
        temp_repo,
        temp_log_file,
        budget_chars,
        mock_dir,
    };
    let mut rows: Vec<ParityRow> = Vec::new();
    let mut pass_all = true;
    for (cmd, args, schema_keys) in overlap {
        let row = evaluate_parity_case(
            &ctx,
            ParityCaseInput {
                cmd,
                args: &args,
                schema_keys: &schema_keys,
            },
        );
        if !(row.rust_ok && row.bash_ok && row.json_ok && row.logs_ok && row.budget_ok) {
            pass_all = false;
            crate::cx_eprintln!(
                "cxparity: FAIL {} rust_ok={} bash_ok={} json_ok={} logs_ok={} budget_ok={}",
                row.cmd,
                row.rust_ok,
                row.bash_ok,
                row.json_ok,
                row.logs_ok,
                row.budget_ok
            );
        }
        rows.push(row);
    }
    (rows, pass_all)
}

pub fn cmd_parity() -> i32 {
    let (repo, exe, budget_chars) = match resolve_parity_context() {
        Ok(v) => v,
        Err(code) => return code,
    };
    let temp_repo = match setup_temp_repo() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{e}");
            return 1;
        }
    };
    let temp_log_file = temp_repo.join(".codex").join("cxlogs").join("runs.jsonl");
    let mock_dir = match setup_parity_mocks(&repo, &temp_repo) {
        Ok(v) => v,
        Err(e) => return parity_error(&temp_repo, &e),
    };
    let overlap = parity_overlap(&repo);
    if overlap.is_empty() {
        return parity_error(&temp_repo, "cxparity: no overlap commands found");
    }

    let (rows, pass_all) = parity_rows(
        overlap,
        &repo,
        &exe,
        &temp_repo,
        &temp_log_file,
        budget_chars,
        &mock_dir,
    );
    if let Err(e) = fs::remove_dir_all(&temp_repo) {
        crate::cx_eprintln!(
            "cxparity: WARN failed to cleanup temp repo {}: {e}",
            temp_repo.display()
        );
    }

    print_parity_table(&rows);
    if pass_all { 0 } else { 1 }
}

fn parity_error(temp_repo: &std::path::Path, message: &str) -> i32 {
    crate::cx_eprintln!("{message}");
    if let Err(e) = fs::remove_dir_all(temp_repo) {
        crate::cx_eprintln!(
            "cxparity: WARN failed to cleanup temp repo {}: {e}",
            temp_repo.display()
        );
    }
    1
}

use std::env;
use std::io::Read;

use crate::agentcmds;
use crate::analytics::{print_alert, print_metrics, print_profile, print_trace, print_worklog};
use crate::bench_parity;
use crate::capture::{chunk_text_by_budget, run_system_command_capture};
use crate::cmdctx::CmdCtx;
use crate::command_names::{is_compat_name, is_native_name};
use crate::compat_cmd;
use crate::diagnostics::cmd_diag;
use crate::doctor;
use crate::execmeta::utc_now_iso;
use crate::introspect::{
    cmd_core as introspect_cmd_core, print_version as introspect_print_version,
};
use crate::logs::cmd_logs;
use crate::logview::{cmd_budget, cmd_log_tail};
use crate::native_cmd;
use crate::optimize::{parse_optimize_args, print_optimize};
use crate::policy::cmd_policy;
use crate::prompting::{cmd_fanout, cmd_prompt, cmd_promptlint, cmd_roles};
use crate::quarantine::{cmd_quarantine_list, cmd_quarantine_show};
use crate::routing::{cmd_routes, print_where};
use crate::runtime_controls::{
    cmd_alert_off, cmd_alert_on, cmd_alert_show, cmd_log_off, cmd_log_on, cmd_rtk_status,
};
use crate::schema_ops::{cmd_ci, cmd_schema};
use crate::settings_cmds::{cmd_llm, cmd_state_get, cmd_state_set, cmd_state_show};
use crate::state::{current_task_id, current_task_parent_id, set_state_path};
use crate::structured_cmds;
use crate::task_cmds;
use crate::taskrun::{TaskRunner, run_task_by_id};
use crate::tasks::{
    cmd_task_add, cmd_task_fanout, cmd_task_list, cmd_task_show, read_tasks, write_tasks,
};
use crate::types::{ExecutionResult, TaskSpec};

const APP_NAME: &str = "cxrs";
const APP_DESC: &str = "Rust spike for the cx toolchain";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("{APP_NAME} - {APP_DESC}");
    println!();
    println!("Usage:");
    println!("  {APP_NAME} <command> [args]");
    println!();
    println!("Commands:");
    println!("  version            Print tool version");
    println!("  where              Show binary/source/log resolution details");
    println!("  routes [--json] [cmd...]  Show routing map/introspection");
    println!("  diag               Non-interactive diagnostic report");
    println!("  parity             Run Rust/Bash parity invariants");
    println!("  schema list [--json]  List registered schemas");
    println!("  logs validate [--fix=false] [--legacy-ok]  Validate execution log JSONL contract");
    println!(
        "  logs migrate [--out PATH] [--in-place]  Normalize legacy run logs to current contract"
    );
    println!(
        "  ci validate [--strict] [--legacy-ok] [--json]  CI-friendly validation gate (no network)"
    );
    println!("  core               Show execution-core pipeline config");
    println!(
        "  task <op> [...]    Task graph management (add/list/claim/complete/fail/show/fanout)"
    );
    println!("  doctor             Run non-interactive environment checks");
    println!("  supports <name>    Exit 0 if subcommand is supported by cxrs");
    println!(
        "  llm <op> [...]     Manage LLM backend/model defaults (show|use|unset|set-backend|set-model|clear-model)"
    );
    println!("  state <op> [...]   Manage repo state JSON (show|get|set)");
    println!("  policy [show|check ...] Show safety rules or classify a command");
    println!("  bench <N> -- <cmd...>  Benchmark command runtime and tokens");
    println!("  cx <cmd...>        Run command output through LLM text mode");
    println!("  cxj <cmd...>       Run command output through LLM JSONL mode");
    println!("  cxo <cmd...>       Run command output and print last agent message");
    println!("  cxol <cmd...>      Run command output through LLM plain mode");
    println!("  cxcopy <cmd...>    Copy cxo output to clipboard (pbcopy)");
    println!("  fix <cmd...>       Explain failures and suggest next steps (text)");
    println!("  budget             Show context budget settings and last clip fields");
    println!("  log-tail [N]       Pretty-print last N log entries");
    println!("  health             Run end-to-end selected-LLM/cx smoke checks");
    println!("  rtk-status         Show RTK version/range decision and fallback mode");
    println!("  log-on             Enable cx logging (process-local)");
    println!("  log-off            Disable cx logging in this process");
    println!("  alert-show         Show active alert thresholds/toggles");
    println!("  alert-on           Enable alerts (process-local)");
    println!("  alert-off          Disable alerts in this process");
    println!("  chunk              Chunk stdin text by context budget chars");
    println!("  metrics [N]        Token and duration aggregates from last N runs");
    println!("  prompt <mode> <request>  Generate Codex-ready prompt block");
    println!("  roles [role]       List roles or print role-specific prompt header");
    println!("  fanout <objective> Generate role-tagged parallelizable subtasks");
    println!("  promptlint [N]     Lint prompt/cost patterns from last N runs");
    println!("  cx-compat <cmd...> Compatibility shim for bash-style cx command names");
    println!("  profile [N]        Summarize last N runs from resolved cx log (default 50)");
    println!("  alert [N]          Report anomalies from last N runs (default 50)");
    println!("  optimize [N] [--json]  Recommend cost/latency improvements from last N runs");
    println!("  worklog [N]        Emit Markdown worklog from last N runs (default 50)");
    println!("  trace [N]          Show Nth most-recent run from resolved cx log (default 1)");
    println!("  next <cmd...>      Suggest next shell commands from command output (strict JSON)");
    println!("  diffsum            Summarize unstaged diff (strict schema)");
    println!("  diffsum-staged     Summarize staged diff (strict schema)");
    println!("  fix-run <cmd...>   Suggest remediation commands for a failed command");
    println!("  commitjson         Generate strict JSON commit object from staged diff");
    println!("  commitmsg          Generate commit message text from staged diff");
    println!("  replay <id>        Replay quarantined schema run in strict mode");
    println!("  quarantine list [N]  Show recent quarantine entries (default 20)");
    println!("  quarantine show <id> Show quarantined entry payload");
    println!("  help               Print this help");
}

fn print_task_help() {
    println!("cx help task");
    println!();
    println!("Task commands:");
    println!("  cx task add \"<objective>\" --role <architect|implementer|reviewer|tester|doc>");
    println!("  cx task list [--status pending|in_progress|complete|failed]");
    println!("  cx task claim <id>");
    println!("  cx task complete <id>");
    println!("  cx task fail <id>");
    println!("  cx task show <id>");
    println!("  cx task fanout \"<objective>\" [--from staged-diff|worktree|log|file:PATH]");
    println!("  cx task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]");
    println!("  cx task run-all [--status pending]");
}

// path resolution moved to `paths.rs`
// schema helpers moved to `schema.rs`
// ensure_parent_dir moved to `paths.rs`
// run logging moved to `runlog.rs`
// state read/write moved to `state.rs`

fn task_runner() -> TaskRunner {
    TaskRunner {
        read_tasks,
        write_tasks,
        current_task_id,
        current_task_parent_id,
        set_state_path,
        utc_now_iso,
        cmd_commitjson,
        cmd_commitmsg,
        cmd_diffsum,
        cmd_next,
        cmd_fix_run,
        cmd_fix,
        cmd_cx,
        cmd_cxj,
        cmd_cxo,
        execute_task,
    }
}

fn task_cmd_deps() -> task_cmds::TaskCmdDeps {
    task_cmds::TaskCmdDeps {
        cmd_task_add,
        cmd_task_list,
        cmd_task_show,
        cmd_task_fanout,
        read_tasks,
        run_task_by_id,
        make_task_runner: task_runner,
    }
}

fn cmd_ctx() -> CmdCtx {
    CmdCtx {
        app_name: APP_NAME,
        app_version: APP_VERSION,
        execute_task,
        run_llm_jsonl: crate::execution::run_llm_jsonl,
    }
}

fn cmd_task(args: &[String]) -> i32 {
    task_cmds::handler(&cmd_ctx(), args, &task_cmd_deps())
}

// runs.jsonl readers moved to `logs.rs`

fn execute_task(spec: TaskSpec) -> Result<ExecutionResult, String> {
    crate::execution::execute_task(spec)
}

fn cmd_bench(runs: usize, command: &[String]) -> i32 {
    bench_parity::cmd_bench(APP_NAME, runs, command)
}

fn cmd_cx(command: &[String]) -> i32 {
    agentcmds::cmd_cx(command, execute_task)
}

fn cmd_cxj(command: &[String]) -> i32 {
    agentcmds::cmd_cxj(command, execute_task)
}

fn cmd_cxo(command: &[String]) -> i32 {
    agentcmds::cmd_cxo(command, execute_task)
}

fn cmd_cxol(command: &[String]) -> i32 {
    agentcmds::cmd_cxol(command, execute_task)
}

fn cmd_cxcopy(command: &[String]) -> i32 {
    agentcmds::cmd_cxcopy(command, execute_task)
}

fn cmd_fix(command: &[String]) -> i32 {
    agentcmds::cmd_fix(command, run_system_command_capture, execute_task)
}

fn cmd_parity() -> i32 {
    bench_parity::cmd_parity()
}

fn cmd_chunk() -> i32 {
    let mut buf = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
        eprintln!("cxrs chunk: failed to read stdin: {e}");
        return 1;
    }
    let budget = env::var("CX_CONTEXT_BUDGET_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(12000);
    let chunks = chunk_text_by_budget(&buf, budget);
    let total = chunks.len();
    for (i, ch) in chunks.iter().enumerate() {
        println!("----- cx chunk {}/{} -----", i + 1, total);
        print!("{ch}");
        if !ch.ends_with('\n') {
            println!();
        }
    }
    0
}

fn cmd_next(command: &[String]) -> i32 {
    structured_cmds::cmd_next(command, execute_task)
}

fn cmd_fix_run(command: &[String]) -> i32 {
    structured_cmds::cmd_fix_run(APP_NAME, command, execute_task)
}

fn cmd_diffsum(staged: bool) -> i32 {
    structured_cmds::cmd_diffsum(staged, execute_task)
}

fn cmd_commitjson() -> i32 {
    structured_cmds::cmd_commitjson(execute_task)
}

fn cmd_commitmsg() -> i32 {
    structured_cmds::cmd_commitmsg(execute_task)
}

fn cmd_replay(id: &str) -> i32 {
    structured_cmds::cmd_replay(id, crate::execution::run_llm_jsonl)
}

fn compat_print_version() {
    introspect_print_version(APP_NAME, APP_VERSION);
}

fn compat_cmd_where(args: &[String]) -> i32 {
    print_where(args, APP_VERSION)
}

fn compat_cmd_diag() -> i32 {
    cmd_diag(APP_VERSION)
}

fn compat_cmd_core() -> i32 {
    introspect_cmd_core(APP_VERSION)
}

fn compat_cmd_logs(args: &[String]) -> i32 {
    cmd_logs(APP_NAME, args)
}

fn compat_cmd_policy(args: &[String]) -> i32 {
    cmd_policy(args, APP_NAME)
}

fn compat_cmd_llm(args: &[String]) -> i32 {
    cmd_llm(APP_NAME, args)
}

fn compat_cmd_doctor() -> i32 {
    doctor::print_doctor(crate::execution::run_llm_jsonl)
}

fn compat_cmd_health() -> i32 {
    doctor::cmd_health(crate::execution::run_llm_jsonl, cmd_cxo)
}

fn compat_deps() -> compat_cmd::CompatDeps {
    compat_cmd::CompatDeps {
        print_help,
        print_task_help,
        print_version: compat_print_version,
        cmd_doctor: compat_cmd_doctor,
        cmd_where: compat_cmd_where,
        cmd_routes,
        cmd_diag: compat_cmd_diag,
        cmd_parity,
        cmd_core: compat_cmd_core,
        cmd_logs: compat_cmd_logs,
        cmd_task,
        print_metrics,
        print_profile,
        print_trace,
        print_alert,
        parse_optimize_args,
        print_optimize,
        print_worklog,
        cmd_cx,
        cmd_cxj,
        cmd_cxo,
        cmd_cxol,
        cmd_cxcopy,
        cmd_policy: compat_cmd_policy,
        cmd_state_show,
        cmd_state_get,
        cmd_state_set,
        cmd_llm: compat_cmd_llm,
        cmd_bench,
        cmd_prompt,
        cmd_roles,
        cmd_fanout,
        cmd_promptlint,
        cmd_next,
        cmd_fix,
        cmd_diffsum,
        cmd_commitjson,
        cmd_commitmsg,
        cmd_budget,
        cmd_log_tail,
        cmd_health: compat_cmd_health,
        cmd_rtk_status,
        cmd_log_on,
        cmd_log_off,
        cmd_alert_show,
        cmd_alert_on,
        cmd_alert_off,
        cmd_chunk,
        cmd_fix_run,
        cmd_replay,
        cmd_quarantine_list,
        cmd_quarantine_show,
    }
}

fn cmd_cx_compat(args: &[String]) -> i32 {
    compat_cmd::handler(&cmd_ctx(), args, &compat_deps())
}

fn native_print_version() {
    introspect_print_version(APP_NAME, APP_VERSION);
}

fn native_cmd_schema(args: &[String]) -> i32 {
    cmd_schema(APP_NAME, args)
}

fn native_cmd_logs(args: &[String]) -> i32 {
    cmd_logs(APP_NAME, args)
}

fn native_cmd_ci(args: &[String]) -> i32 {
    cmd_ci(APP_NAME, args)
}

fn native_cmd_where(args: &[String]) -> i32 {
    print_where(args, APP_VERSION)
}

fn native_cmd_diag() -> i32 {
    cmd_diag(APP_VERSION)
}

fn native_cmd_core() -> i32 {
    introspect_cmd_core(APP_VERSION)
}

fn native_cmd_llm(args: &[String]) -> i32 {
    cmd_llm(APP_NAME, args)
}

fn native_cmd_policy(args: &[String]) -> i32 {
    cmd_policy(args, APP_NAME)
}

fn native_cmd_doctor() -> i32 {
    doctor::print_doctor(crate::execution::run_llm_jsonl)
}

fn native_cmd_health() -> i32 {
    doctor::cmd_health(crate::execution::run_llm_jsonl, cmd_cxo)
}

fn native_deps() -> native_cmd::NativeDeps {
    native_cmd::NativeDeps {
        print_help,
        print_task_help,
        print_version: native_print_version,
        cmd_schema: native_cmd_schema,
        cmd_logs: native_cmd_logs,
        cmd_ci: native_cmd_ci,
        cmd_core: native_cmd_core,
        cmd_task,
        cmd_where: native_cmd_where,
        cmd_routes,
        cmd_diag: native_cmd_diag,
        cmd_parity,
        is_native_name,
        is_compat_name,
        cmd_doctor: native_cmd_doctor,
        cmd_state_show,
        cmd_state_get,
        cmd_state_set,
        cmd_llm: native_cmd_llm,
        cmd_policy: native_cmd_policy,
        cmd_bench,
        print_metrics,
        cmd_prompt,
        cmd_roles,
        cmd_fanout,
        cmd_promptlint,
        cmd_cx_compat,
        cmd_cx,
        cmd_cxj,
        cmd_cxo,
        cmd_cxol,
        cmd_cxcopy,
        cmd_fix,
        cmd_budget,
        cmd_log_tail,
        cmd_health: native_cmd_health,
        cmd_rtk_status,
        cmd_log_on,
        cmd_log_off,
        cmd_alert_show,
        cmd_alert_on,
        cmd_alert_off,
        cmd_chunk,
        print_profile,
        print_alert,
        parse_optimize_args,
        print_optimize,
        print_worklog,
        print_trace,
        cmd_next,
        cmd_diffsum,
        cmd_fix_run,
        cmd_commitjson,
        cmd_commitmsg,
        cmd_replay,
        cmd_quarantine_list,
        cmd_quarantine_show,
    }
}

pub fn run() -> i32 {
    let args: Vec<String> = env::args().collect();
    native_cmd::handler(&cmd_ctx(), &args, &native_deps())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{BudgetConfig, choose_clip_mode, clip_text_with_config, should_use_rtk};
    use crate::logs::append_jsonl;
    use crate::runlog::log_schema_failure;
    use serde_json::Value;
    use serde_json::json;
    use std::fs;
    use std::process::Command;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn smart_mode_prefers_tail_on_error_keywords() {
        assert_eq!(choose_clip_mode("all good", "smart"), "head");
        assert_eq!(choose_clip_mode("WARNING: issue", "smart"), "tail");
        assert_eq!(choose_clip_mode("failed to run", "smart"), "tail");
    }

    #[test]
    fn clip_text_respects_line_and_char_budget() {
        let cfg = BudgetConfig {
            budget_chars: 12,
            budget_lines: 2,
            clip_mode: "head".to_string(),
            clip_footer: false,
        };
        let (out, stats) = clip_text_with_config("line1\nline2\nline3\n", &cfg);
        assert!(out.starts_with("line1\nline2"));
        assert_eq!(stats.budget_chars, Some(12));
        assert_eq!(stats.budget_lines, Some(2));
        assert_eq!(stats.clipped, Some(true));
    }

    #[test]
    fn jsonl_append_integrity() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("runs.jsonl");
        append_jsonl(&file, &json!({"a":1})).expect("append 1");
        append_jsonl(&file, &json!({"b":2})).expect("append 2");
        let content = fs::read_to_string(&file).expect("read");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        let v1: Value = serde_json::from_str(lines[0]).expect("line1 json");
        let v2: Value = serde_json::from_str(lines[1]).expect("line2 json");
        assert_eq!(v1.get("a").and_then(Value::as_i64), Some(1));
        assert_eq!(v2.get("b").and_then(Value::as_i64), Some(2));
    }

    #[test]
    fn rtk_unavailable_path_uses_native() {
        let cmd = vec!["git".to_string(), "status".to_string()];
        assert!(!should_use_rtk(&cmd, "auto", true, false));
        assert!(!should_use_rtk(&cmd, "native", true, true));
    }

    #[test]
    fn schema_failure_writes_quarantine_and_logs() {
        let _guard = cwd_lock().lock().expect("lock");
        let dir = tempdir().expect("tempdir");
        let prev = env::current_dir().expect("cwd");
        env::set_current_dir(dir.path()).expect("cd temp");
        let _ = Command::new("git")
            .args(["init"])
            .output()
            .expect("git init");

        let qid = log_schema_failure(
            "cxrs_next",
            "invalid_json",
            "raw",
            "{}",
            "prompt",
            Vec::new(),
        )
        .expect("schema failure log");
        assert!(!qid.is_empty());

        let qfile = dir
            .path()
            .join(".codex")
            .join("quarantine")
            .join(format!("{qid}.json"));
        assert!(qfile.exists());

        let sf_log = dir
            .path()
            .join(".codex")
            .join("cxlogs")
            .join("schema_failures.jsonl");
        let sf = fs::read_to_string(&sf_log).expect("read schema fail log");
        let last_sf: Value =
            serde_json::from_str(sf.lines().last().expect("sf line")).expect("sf json");
        assert_eq!(
            last_sf.get("quarantine_id").and_then(Value::as_str),
            Some(qid.as_str())
        );

        let runs_log = dir.path().join(".codex").join("cxlogs").join("runs.jsonl");
        let runs = fs::read_to_string(&runs_log).expect("read runs");
        let last_run: Value =
            serde_json::from_str(runs.lines().last().expect("run line")).expect("run json");
        assert_eq!(
            last_run.get("quarantine_id").and_then(Value::as_str),
            Some(qid.as_str())
        );
        assert_eq!(
            last_run.get("schema_valid").and_then(Value::as_bool),
            Some(false)
        );

        env::set_current_dir(prev).expect("restore cwd");
    }
}

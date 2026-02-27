use std::env;
use std::io::Read;

mod deps;

use crate::agentcmds;
use crate::analytics::{print_alert, print_metrics, print_profile, print_trace, print_worklog};
use crate::bench_parity;
use crate::broker::cmd_broker as broker_cmd;
use crate::capture::{chunk_text_by_budget, run_system_command_capture};
use crate::cmdctx::CmdCtx;
use crate::command_names::{is_compat_name, is_native_name};
use crate::compat_cmd;
use crate::config::{
    APP_DESC, APP_NAME, APP_VERSION, DEFAULT_QUARANTINE_LIST, DEFAULT_RUN_WINDOW, app_config,
    init_app_config,
};
use crate::diagnostics::cmd_diag;
use crate::doctor;
use crate::execmeta::utc_now_iso;
use crate::help::{render_help, render_task_help};
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

fn print_help() {
    print!(
        "{}",
        render_help(
            APP_NAME,
            APP_DESC,
            DEFAULT_RUN_WINDOW,
            DEFAULT_QUARANTINE_LIST
        )
    );
}

fn print_task_help() {
    print!("{}", render_task_help());
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
        crate::cx_eprintln!("cxrs chunk: failed to read stdin: {e}");
        return 1;
    }
    let budget = app_config().budget_chars;
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

fn compat_cmd_diag(args: &[String]) -> i32 {
    cmd_diag(APP_VERSION, args)
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

fn compat_cmd_broker(args: &[String]) -> i32 {
    broker_cmd(APP_NAME, args)
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

fn cmd_cx_compat(args: &[String]) -> i32 {
    compat_cmd::handler(&cmd_ctx(), args, &deps::compat_deps())
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

fn native_cmd_diag(args: &[String]) -> i32 {
    cmd_diag(APP_VERSION, args)
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

fn native_cmd_broker(args: &[String]) -> i32 {
    broker_cmd(APP_NAME, args)
}

fn native_cmd_doctor() -> i32 {
    doctor::print_doctor(crate::execution::run_llm_jsonl)
}

fn native_cmd_health() -> i32 {
    doctor::cmd_health(crate::execution::run_llm_jsonl, cmd_cxo)
}

pub fn run() -> i32 {
    init_app_config();
    let args: Vec<String> = env::args().collect();
    native_cmd::handler(&cmd_ctx(), &args, &deps::native_deps())
}

#[cfg(test)]
mod tests;

use crate::cmdctx::CmdCtx;
use crate::config::{DEFAULT_OPTIMIZE_WINDOW, DEFAULT_QUARANTINE_LIST, DEFAULT_RUN_WINDOW};
use crate::error::{EXIT_OK, EXIT_USAGE, format_error, print_usage_error};

pub struct CompatDeps {
    pub print_help: fn(),
    pub print_task_help: fn(),
    pub print_version: fn(),
    pub cmd_doctor: fn() -> i32,
    pub cmd_where: fn(&[String]) -> i32,
    pub cmd_routes: fn(&[String]) -> i32,
    pub cmd_diag: fn() -> i32,
    pub cmd_parity: fn() -> i32,
    pub cmd_core: fn() -> i32,
    pub cmd_logs: fn(&[String]) -> i32,
    pub cmd_task: fn(&[String]) -> i32,
    pub print_metrics: fn(usize) -> i32,
    pub print_profile: fn(usize) -> i32,
    pub print_trace: fn(usize) -> i32,
    pub print_alert: fn(usize) -> i32,
    pub parse_optimize_args: fn(&[String], usize) -> Result<(usize, bool), String>,
    pub print_optimize: fn(usize, bool) -> i32,
    pub print_worklog: fn(usize) -> i32,
    pub cmd_cx: fn(&[String]) -> i32,
    pub cmd_cxj: fn(&[String]) -> i32,
    pub cmd_cxo: fn(&[String]) -> i32,
    pub cmd_cxol: fn(&[String]) -> i32,
    pub cmd_cxcopy: fn(&[String]) -> i32,
    pub cmd_policy: fn(&[String]) -> i32,
    pub cmd_state_show: fn() -> i32,
    pub cmd_state_get: fn(&str) -> i32,
    pub cmd_state_set: fn(&str, &str) -> i32,
    pub cmd_llm: fn(&[String]) -> i32,
    pub cmd_bench: fn(usize, &[String]) -> i32,
    pub cmd_prompt: fn(&str, &str) -> i32,
    pub cmd_roles: fn(Option<&str>) -> i32,
    pub cmd_fanout: fn(&str) -> i32,
    pub cmd_promptlint: fn(usize) -> i32,
    pub cmd_next: fn(&[String]) -> i32,
    pub cmd_fix: fn(&[String]) -> i32,
    pub cmd_diffsum: fn(bool) -> i32,
    pub cmd_commitjson: fn() -> i32,
    pub cmd_commitmsg: fn() -> i32,
    pub cmd_budget: fn() -> i32,
    pub cmd_log_tail: fn(usize) -> i32,
    pub cmd_health: fn() -> i32,
    pub cmd_rtk_status: fn() -> i32,
    pub cmd_log_on: fn() -> i32,
    pub cmd_log_off: fn() -> i32,
    pub cmd_alert_show: fn() -> i32,
    pub cmd_alert_on: fn() -> i32,
    pub cmd_alert_off: fn() -> i32,
    pub cmd_chunk: fn() -> i32,
    pub cmd_fix_run: fn(&[String]) -> i32,
    pub cmd_replay: fn(&str) -> i32,
    pub cmd_quarantine_list: fn(usize) -> i32,
    pub cmd_quarantine_show: fn(&str) -> i32,
}

fn parse_n(args: &[String], idx: usize, default: usize) -> usize {
    args.get(idx)
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn require_prefixed_arg(args: &[String], usage: &str) -> Result<(), i32> {
    if args.len() < 2 {
        return Err(print_usage_error("cx", usage));
    }
    Ok(())
}

fn run_prefixed_cmd(args: &[String], usage: &str, f: fn(&[String]) -> i32) -> i32 {
    if require_prefixed_arg(args, usage).is_err() {
        return EXIT_USAGE;
    }
    f(&args[1..])
}

fn handle_state(app_name: &str, args: &[String], deps: &CompatDeps) -> i32 {
    match args.get(1).map(String::as_str).unwrap_or("show") {
        "show" => (deps.cmd_state_show)(),
        "get" => match args.get(2) {
            Some(key) => (deps.cmd_state_get)(key),
            None => print_usage_error("state", &format!("{app_name} cx state get <key>")),
        },
        "set" => match (args.get(2), args.get(3)) {
            (Some(key), Some(value)) => (deps.cmd_state_set)(key, value),
            _ => print_usage_error("state", &format!("{app_name} cx state set <key> <value>")),
        },
        other => {
            crate::cx_eprintln!("{app_name} cx state: unknown subcommand '{other}'");
            EXIT_USAGE
        }
    }
}

fn handle_bench(app_name: &str, args: &[String], deps: &CompatDeps) -> i32 {
    let Some(runs) = args
        .get(1)
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
    else {
        return print_usage_error(
            "bench",
            &format!("{app_name} cx bench <runs> -- <command...>"),
        );
    };
    let Some(i) = args.iter().position(|v| v == "--") else {
        return print_usage_error(
            "bench",
            &format!("{app_name} cx bench <runs> -- <command...>"),
        );
    };
    if i + 1 >= args.len() {
        return print_usage_error(
            "bench",
            &format!("{app_name} cx bench <runs> -- <command...>"),
        );
    }
    (deps.cmd_bench)(runs, &args[i + 1..])
}

fn handle_prompt(app_name: &str, args: &[String], deps: &CompatDeps) -> i32 {
    let Some(mode) = args.get(1) else {
        return print_usage_error("prompt", &format!("{app_name} cx prompt <mode> <request>"));
    };
    if args.len() < 3 {
        return print_usage_error("prompt", &format!("{app_name} cx prompt <mode> <request>"));
    }
    (deps.cmd_prompt)(mode, &args[2..].join(" "))
}

fn handle_optimize(args: &[String], deps: &CompatDeps) -> i32 {
    let (n, json_out) = match (deps.parse_optimize_args)(&args[1..], DEFAULT_OPTIMIZE_WINDOW) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{}", format_error("cx optimize", &e));
            return EXIT_USAGE;
        }
    };
    (deps.print_optimize)(n, json_out)
}

fn handle_replay(app_name: &str, args: &[String], deps: &CompatDeps) -> i32 {
    match args.get(1) {
        Some(id) => (deps.cmd_replay)(id),
        None => print_usage_error("replay", &format!("{app_name} cx replay <quarantine_id>")),
    }
}

fn handle_quarantine(app_name: &str, args: &[String], deps: &CompatDeps) -> i32 {
    match args.get(1).map(String::as_str).unwrap_or("list") {
        "list" => (deps.cmd_quarantine_list)(parse_n(args, 2, DEFAULT_QUARANTINE_LIST)),
        "show" => match args.get(2) {
            Some(id) => (deps.cmd_quarantine_show)(id),
            None => print_usage_error(
                "quarantine",
                &format!("{app_name} cx quarantine show <quarantine_id>"),
            ),
        },
        other => {
            crate::cx_eprintln!("{app_name} cx quarantine: unknown subcommand '{other}'");
            EXIT_USAGE
        }
    }
}

fn dispatch_meta_commands(
    sub: &str,
    app_name: &str,
    args: &[String],
    deps: &CompatDeps,
) -> Option<i32> {
    let out = match sub {
        "help" => {
            if args.get(1).map(String::as_str) == Some("task") {
                (deps.print_task_help)();
            } else {
                (deps.print_help)();
            }
            EXIT_OK
        }
        "cxversion" | "version" => {
            (deps.print_version)();
            EXIT_OK
        }
        "cxdoctor" | "doctor" => (deps.cmd_doctor)(),
        "cxwhere" | "where" => (deps.cmd_where)(&args[1..]),
        "cxroutes" | "routes" => (deps.cmd_routes)(&args[1..]),
        "cxdiag" | "diag" => (deps.cmd_diag)(),
        "cxparity" | "parity" => (deps.cmd_parity)(),
        "cxcore" | "core" => (deps.cmd_core)(),
        "cxlogs" | "logs" => (deps.cmd_logs)(&args[1..]),
        "cxtask" | "task" => (deps.cmd_task)(&args[1..]),
        "cxpolicy" | "policy" => (deps.cmd_policy)(&args[1..]),
        "cxstate" | "state" => handle_state(app_name, args, deps),
        "cxllm" | "llm" => (deps.cmd_llm)(&args[1..]),
        _ => return None,
    };
    Some(out)
}

fn dispatch_analytics_commands(sub: &str, args: &[String], deps: &CompatDeps) -> Option<i32> {
    let out = match sub {
        "cxmetrics" | "metrics" => (deps.print_metrics)(parse_n(args, 1, DEFAULT_RUN_WINDOW)),
        "cxprofile" | "profile" => (deps.print_profile)(parse_n(args, 1, DEFAULT_RUN_WINDOW)),
        "cxtrace" | "trace" => (deps.print_trace)(parse_n(args, 1, 1)),
        "cxalert" | "alert" => (deps.print_alert)(parse_n(args, 1, DEFAULT_RUN_WINDOW)),
        "cxworklog" | "worklog" => (deps.print_worklog)(parse_n(args, 1, DEFAULT_RUN_WINDOW)),
        "cxoptimize" | "optimize" => handle_optimize(args, deps),
        _ => return None,
    };
    Some(out)
}

fn dispatch_prompt_commands(
    sub: &str,
    app_name: &str,
    args: &[String],
    deps: &CompatDeps,
) -> Option<i32> {
    let out = match sub {
        "cxbench" | "bench" => handle_bench(app_name, args, deps),
        "cxprompt" | "prompt" => handle_prompt(app_name, args, deps),
        "cxroles" | "roles" => (deps.cmd_roles)(args.get(1).map(String::as_str)),
        "cxfanout" | "fanout" => {
            if args.len() < 2 {
                return Some(print_usage_error(
                    "fanout",
                    &format!("{app_name} cx fanout <objective>"),
                ));
            }
            (deps.cmd_fanout)(&args[1..].join(" "))
        }
        "cxpromptlint" | "promptlint" => (deps.cmd_promptlint)(parse_n(args, 1, 200)),
        _ => return None,
    };
    Some(out)
}

fn dispatch_agent_prefixed_shortcuts(
    sub: &str,
    app_name: &str,
    args: &[String],
    deps: &CompatDeps,
) -> Option<i32> {
    let out = match sub {
        "cx" => run_prefixed_cmd(
            args,
            &format!("{app_name} cx <command> [args...]"),
            deps.cmd_cx,
        ),
        "cxj" => run_prefixed_cmd(
            args,
            &format!("{app_name} cxj <command> [args...]"),
            deps.cmd_cxj,
        ),
        "cxo" => run_prefixed_cmd(
            args,
            &format!("{app_name} cxo <command> [args...]"),
            deps.cmd_cxo,
        ),
        "cxol" => run_prefixed_cmd(
            args,
            &format!("{app_name} cxol <command> [args...]"),
            deps.cmd_cxol,
        ),
        "cxcopy" => run_prefixed_cmd(
            args,
            &format!("{app_name} cxcopy <command> [args...]"),
            deps.cmd_cxcopy,
        ),
        _ => return None,
    };
    Some(out)
}

fn dispatch_agent_commands(
    sub: &str,
    app_name: &str,
    args: &[String],
    deps: &CompatDeps,
) -> Option<i32> {
    if let Some(out) = dispatch_agent_prefixed_shortcuts(sub, app_name, args, deps) {
        return Some(out);
    }
    let out = match sub {
        "cxnext" | "next" => run_prefixed_cmd(
            args,
            &format!("{app_name} cx next <command> [args...]"),
            deps.cmd_next,
        ),
        "cxfix" | "fix" => run_prefixed_cmd(
            args,
            &format!("{app_name} cx fix <command> [args...]"),
            deps.cmd_fix,
        ),
        "cxfix_run" | "fix-run" => run_prefixed_cmd(
            args,
            &format!("{app_name} cx fix-run <command> [args...]"),
            deps.cmd_fix_run,
        ),
        _ => return None,
    };
    Some(out)
}
fn dispatch_runtime_commands(
    sub: &str,
    app_name: &str,
    args: &[String],
    deps: &CompatDeps,
) -> Option<i32> {
    let out = match sub {
        "cxbudget" | "budget" => (deps.cmd_budget)(),
        "cxlog_tail" | "log-tail" => (deps.cmd_log_tail)(parse_n(args, 1, 10)),
        "cxhealth" | "health" => (deps.cmd_health)(),
        "cxrtk" | "rtk-status" => (deps.cmd_rtk_status)(),
        "cxlog_on" | "log-on" => (deps.cmd_log_on)(),
        "cxlog_off" | "log-off" => (deps.cmd_log_off)(),
        "cxalert_show" | "alert-show" => (deps.cmd_alert_show)(),
        "cxalert_on" | "alert-on" => (deps.cmd_alert_on)(),
        "cxalert_off" | "alert-off" => (deps.cmd_alert_off)(),
        "cxchunk" | "chunk" => (deps.cmd_chunk)(),
        "cxdiffsum" | "diffsum" => (deps.cmd_diffsum)(false),
        "cxdiffsum_staged" | "diffsum-staged" => (deps.cmd_diffsum)(true),
        "cxcommitjson" | "commitjson" => (deps.cmd_commitjson)(),
        "cxcommitmsg" | "commitmsg" => (deps.cmd_commitmsg)(),
        "cxreplay" | "replay" => handle_replay(app_name, args, deps),
        "cxquarantine" | "quarantine" => handle_quarantine(app_name, args, deps),
        _ => return None,
    };
    Some(out)
}

pub fn handler(ctx: &CmdCtx, args: &[String], deps: &CompatDeps) -> i32 {
    let app_name = ctx.app_name;
    if args.is_empty() {
        return print_usage_error("cx", &format!("{app_name} cx <command> [args...]"));
    }
    let sub = args[0].as_str();

    dispatch_meta_commands(sub, app_name, args, deps)
        .or_else(|| dispatch_analytics_commands(sub, args, deps))
        .or_else(|| dispatch_prompt_commands(sub, app_name, args, deps))
        .or_else(|| dispatch_agent_commands(sub, app_name, args, deps))
        .or_else(|| dispatch_runtime_commands(sub, app_name, args, deps))
        .unwrap_or_else(|| {
            crate::cx_eprintln!(
                "{}",
                format_error("cx", &format!("unsupported command '{sub}'"))
            );
            EXIT_USAGE
        })
}

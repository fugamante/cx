use crate::cmdctx::CmdCtx;
use crate::config::{DEFAULT_OPTIMIZE_WINDOW, DEFAULT_QUARANTINE_LIST, DEFAULT_RUN_WINDOW};
use crate::error::{EXIT_OK, EXIT_RUNTIME, EXIT_USAGE, format_error, print_usage_error};

pub struct NativeDeps {
    pub print_help: fn(),
    pub print_task_help: fn(),
    pub print_version: fn(),
    pub cmd_schema: fn(&[String]) -> i32,
    pub cmd_logs: fn(&[String]) -> i32,
    pub cmd_ci: fn(&[String]) -> i32,
    pub cmd_core: fn() -> i32,
    pub cmd_task: fn(&[String]) -> i32,
    pub cmd_where: fn(&[String]) -> i32,
    pub cmd_routes: fn(&[String]) -> i32,
    pub cmd_diag: fn() -> i32,
    pub cmd_parity: fn() -> i32,
    pub is_native_name: fn(&str) -> bool,
    pub is_compat_name: fn(&str) -> bool,
    pub cmd_doctor: fn() -> i32,
    pub cmd_state_show: fn() -> i32,
    pub cmd_state_get: fn(&str) -> i32,
    pub cmd_state_set: fn(&str, &str) -> i32,
    pub cmd_llm: fn(&[String]) -> i32,
    pub cmd_policy: fn(&[String]) -> i32,
    pub cmd_bench: fn(usize, &[String]) -> i32,
    pub print_metrics: fn(usize) -> i32,
    pub cmd_prompt: fn(&str, &str) -> i32,
    pub cmd_roles: fn(Option<&str>) -> i32,
    pub cmd_fanout: fn(&str) -> i32,
    pub cmd_promptlint: fn(usize) -> i32,
    pub cmd_cx_compat: fn(&[String]) -> i32,
    pub cmd_cx: fn(&[String]) -> i32,
    pub cmd_cxj: fn(&[String]) -> i32,
    pub cmd_cxo: fn(&[String]) -> i32,
    pub cmd_cxol: fn(&[String]) -> i32,
    pub cmd_cxcopy: fn(&[String]) -> i32,
    pub cmd_fix: fn(&[String]) -> i32,
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
    pub print_profile: fn(usize) -> i32,
    pub print_alert: fn(usize) -> i32,
    pub parse_optimize_args: fn(&[String], usize) -> Result<(usize, bool), String>,
    pub print_optimize: fn(usize, bool) -> i32,
    pub print_worklog: fn(usize) -> i32,
    pub print_trace: fn(usize) -> i32,
    pub cmd_next: fn(&[String]) -> i32,
    pub cmd_diffsum: fn(bool) -> i32,
    pub cmd_fix_run: fn(&[String]) -> i32,
    pub cmd_commitjson: fn() -> i32,
    pub cmd_commitmsg: fn() -> i32,
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

fn require_min_args(args: &[String], min: usize, usage: &str) -> Result<(), i32> {
    if args.len() < min {
        return Err(print_usage_error(usage, usage));
    }
    Ok(())
}

fn run_agent_cmd(args: &[String], min: usize, usage: &str, f: fn(&[String]) -> i32) -> i32 {
    if require_min_args(args, min, usage).is_err() {
        return EXIT_USAGE;
    }
    f(&args[2..])
}

fn handle_state(app_name: &str, args: &[String], deps: &NativeDeps) -> i32 {
    match args.get(2).map(String::as_str).unwrap_or("show") {
        "show" => (deps.cmd_state_show)(),
        "get" => match args.get(3) {
            Some(key) => (deps.cmd_state_get)(key),
            None => print_usage_error("state", &format!("{app_name} state get <key>")),
        },
        "set" => match (args.get(3), args.get(4)) {
            (Some(key), Some(value)) => (deps.cmd_state_set)(key, value),
            _ => print_usage_error("state", &format!("{app_name} state set <key> <value>")),
        },
        other => {
            crate::cx_eprintln!("{app_name}: unknown state subcommand '{other}'");
            crate::cx_eprintln!("Usage: {app_name} state <show|get <key>|set <key> <value>>");
            EXIT_USAGE
        }
    }
}

fn handle_supports(app_name: &str, args: &[String], deps: &NativeDeps) -> i32 {
    let Some(name) = args.get(2) else {
        return print_usage_error("supports", &format!("{app_name} supports <subcommand>"));
    };
    if (deps.is_native_name)(name) || (deps.is_compat_name)(name) {
        println!("true");
        EXIT_OK
    } else {
        println!("false");
        EXIT_RUNTIME
    }
}

fn handle_telemetry(args: &[String], deps: &NativeDeps) -> i32 {
    let mut logs_args = vec!["stats".to_string()];
    if args.len() > 2 {
        logs_args.extend_from_slice(&args[2..]);
    }
    (deps.cmd_logs)(&logs_args)
}

fn handle_bench(app_name: &str, args: &[String], deps: &NativeDeps) -> i32 {
    let usage = format!("{app_name} bench <runs> -- <command...>");
    let runs = parse_n(args, 2, 0);
    if runs == 0 {
        return print_usage_error("bench", &usage);
    }
    let Some(i) = args.iter().position(|v| v == "--") else {
        return print_usage_error("bench", &usage);
    };
    if i + 1 >= args.len() {
        return print_usage_error("bench", &usage);
    }
    (deps.cmd_bench)(runs, &args[i + 1..])
}

fn handle_prompt(app_name: &str, args: &[String], deps: &NativeDeps) -> i32 {
    let usage = format!("{app_name} prompt <implement|fix|test|doc|ops> <request>");
    let Some(mode) = args.get(2) else {
        return print_usage_error("prompt", &usage);
    };
    if args.len() < 4 {
        return print_usage_error("prompt", &usage);
    }
    (deps.cmd_prompt)(mode, &args[3..].join(" "))
}

fn handle_cx(args: &[String], deps: &NativeDeps) -> i32 {
    if args.len() < 3 {
        return print_usage_error("cx", "cx <command> [args...]");
    }
    if (deps.is_compat_name)(&args[2]) {
        (deps.cmd_cx_compat)(&args[2..])
    } else {
        (deps.cmd_cx)(&args[2..])
    }
}

fn handle_optimize(args: &[String], deps: &NativeDeps) -> i32 {
    let (n, json_out) = match (deps.parse_optimize_args)(&args[2..], DEFAULT_OPTIMIZE_WINDOW) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{}", format_error("optimize", &e));
            return EXIT_USAGE;
        }
    };
    (deps.print_optimize)(n, json_out)
}

fn handle_replay(app_name: &str, args: &[String], deps: &NativeDeps) -> i32 {
    match args.get(2) {
        Some(id) => (deps.cmd_replay)(id),
        None => print_usage_error("replay", &format!("{app_name} replay <quarantine_id>")),
    }
}

fn handle_quarantine(app_name: &str, args: &[String], deps: &NativeDeps) -> i32 {
    match args.get(2).map(String::as_str).unwrap_or("list") {
        "list" => (deps.cmd_quarantine_list)(parse_n(args, 3, DEFAULT_QUARANTINE_LIST)),
        "show" => match args.get(3) {
            Some(id) => (deps.cmd_quarantine_show)(id),
            None => print_usage_error(
                "quarantine",
                &format!("{app_name} quarantine show <quarantine_id>"),
            ),
        },
        other => {
            crate::cx_eprintln!("{app_name}: unknown quarantine subcommand '{other}'");
            crate::cx_eprintln!("Usage: {app_name} quarantine <list [N]|show <id>>");
            EXIT_USAGE
        }
    }
}

fn dispatch_meta_commands(
    cmd: &str,
    app_name: &str,
    args: &[String],
    deps: &NativeDeps,
) -> Option<i32> {
    let out = match cmd {
        "help" | "-h" | "--help" => {
            if args.get(2).map(String::as_str) == Some("task") {
                (deps.print_task_help)();
            } else {
                (deps.print_help)();
            }
            EXIT_OK
        }
        "version" | "-V" | "--version" => {
            (deps.print_version)();
            EXIT_OK
        }
        "schema" => (deps.cmd_schema)(&args[2..]),
        "logs" => (deps.cmd_logs)(&args[2..]),
        "telemetry" => handle_telemetry(args, deps),
        "ci" => (deps.cmd_ci)(&args[2..]),
        "core" => (deps.cmd_core)(),
        "task" => (deps.cmd_task)(&args[2..]),
        "where" => (deps.cmd_where)(&args[2..]),
        "routes" => (deps.cmd_routes)(&args[2..]),
        "diag" => (deps.cmd_diag)(),
        "parity" => (deps.cmd_parity)(),
        "supports" => handle_supports(app_name, args, deps),
        "doctor" => (deps.cmd_doctor)(),
        "state" => handle_state(app_name, args, deps),
        "llm" => (deps.cmd_llm)(&args[2..]),
        "policy" => (deps.cmd_policy)(&args[2..]),
        _ => return None,
    };
    Some(out)
}

fn dispatch_prompt_commands(
    cmd: &str,
    app_name: &str,
    args: &[String],
    deps: &NativeDeps,
) -> Option<i32> {
    let out = match cmd {
        "bench" => handle_bench(app_name, args, deps),
        "metrics" => (deps.print_metrics)(parse_n(args, 2, DEFAULT_RUN_WINDOW)),
        "prompt" => handle_prompt(app_name, args, deps),
        "roles" => (deps.cmd_roles)(args.get(2).map(String::as_str)),
        "fanout" => {
            if args.len() < 3 {
                return Some(print_usage_error(
                    "fanout",
                    &format!("{app_name} fanout <objective>"),
                ));
            }
            (deps.cmd_fanout)(&args[2..].join(" "))
        }
        "promptlint" => (deps.cmd_promptlint)(parse_n(args, 2, DEFAULT_OPTIMIZE_WINDOW)),
        _ => return None,
    };
    Some(out)
}

fn dispatch_agent_commands(cmd: &str, args: &[String], deps: &NativeDeps) -> Option<i32> {
    let out = match cmd {
        "cx" => handle_cx(args, deps),
        "cxj" => run_agent_cmd(args, 3, "cxj <command> [args...]", deps.cmd_cxj),
        "cxo" => run_agent_cmd(args, 3, "cxo <command> [args...]", deps.cmd_cxo),
        "cxol" => run_agent_cmd(args, 3, "cxol <command> [args...]", deps.cmd_cxol),
        "cxcopy" => run_agent_cmd(args, 3, "cxcopy <command> [args...]", deps.cmd_cxcopy),
        "fix" => run_agent_cmd(args, 3, "fix <command> [args...]", deps.cmd_fix),
        "cx-compat" => (deps.cmd_cx_compat)(&args[2..]),
        "next" => run_agent_cmd(args, 3, "next <command> [args...]", deps.cmd_next),
        "fix-run" => run_agent_cmd(args, 3, "fix-run <command> [args...]", deps.cmd_fix_run),
        _ => return None,
    };
    Some(out)
}

fn dispatch_runtime_commands(cmd: &str, args: &[String], deps: &NativeDeps) -> Option<i32> {
    let out = match cmd {
        "budget" => (deps.cmd_budget)(),
        "log-tail" => (deps.cmd_log_tail)(parse_n(args, 2, 10)),
        "health" => (deps.cmd_health)(),
        "rtk-status" => (deps.cmd_rtk_status)(),
        "log-on" => (deps.cmd_log_on)(),
        "log-off" => (deps.cmd_log_off)(),
        "alert-show" => (deps.cmd_alert_show)(),
        "alert-on" => (deps.cmd_alert_on)(),
        "alert-off" => (deps.cmd_alert_off)(),
        "chunk" => (deps.cmd_chunk)(),
        "profile" => (deps.print_profile)(parse_n(args, 2, DEFAULT_RUN_WINDOW)),
        "alert" => (deps.print_alert)(parse_n(args, 2, DEFAULT_RUN_WINDOW)),
        "optimize" => handle_optimize(args, deps),
        "worklog" => (deps.print_worklog)(parse_n(args, 2, DEFAULT_RUN_WINDOW)),
        "trace" => (deps.print_trace)(parse_n(args, 2, 1)),
        _ => return None,
    };
    Some(out)
}

fn dispatch_structured_commands(
    cmd: &str,
    app_name: &str,
    args: &[String],
    deps: &NativeDeps,
) -> Option<i32> {
    let out = match cmd {
        "diffsum" => (deps.cmd_diffsum)(false),
        "diffsum-staged" => (deps.cmd_diffsum)(true),
        "commitjson" => (deps.cmd_commitjson)(),
        "commitmsg" => (deps.cmd_commitmsg)(),
        "replay" => handle_replay(app_name, args, deps),
        "quarantine" => handle_quarantine(app_name, args, deps),
        _ => return None,
    };
    Some(out)
}

pub fn handler(ctx: &CmdCtx, args: &[String], deps: &NativeDeps) -> i32 {
    let app_name = ctx.app_name;
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");

    dispatch_meta_commands(cmd, app_name, args, deps)
        .or_else(|| dispatch_prompt_commands(cmd, app_name, args, deps))
        .or_else(|| dispatch_agent_commands(cmd, args, deps))
        .or_else(|| dispatch_runtime_commands(cmd, args, deps))
        .or_else(|| dispatch_structured_commands(cmd, app_name, args, deps))
        .unwrap_or_else(|| {
            crate::cx_eprintln!(
                "{}",
                format_error("native", &format!("unknown command '{cmd}'"))
            );
            crate::cx_eprintln!("Run '{app_name} help' for usage.");
            EXIT_USAGE
        })
}

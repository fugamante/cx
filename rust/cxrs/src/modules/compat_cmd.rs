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

pub fn handler(ctx: &CmdCtx, args: &[String], deps: &CompatDeps) -> i32 {
    let app_name = ctx.app_name;
    if args.is_empty() {
        return print_usage_error("cx", &format!("{app_name} cx <command> [args...]"));
    }
    let sub = args[0].as_str();
    match sub {
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
        "cxmetrics" | "metrics" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_RUN_WINDOW);
            (deps.print_metrics)(n)
        }
        "cxprofile" | "profile" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_RUN_WINDOW);
            (deps.print_profile)(n)
        }
        "cxtrace" | "trace" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(1);
            (deps.print_trace)(n)
        }
        "cxalert" | "alert" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_RUN_WINDOW);
            (deps.print_alert)(n)
        }
        "cxoptimize" | "optimize" => {
            let (n, json_out) =
                match (deps.parse_optimize_args)(&args[1..], DEFAULT_OPTIMIZE_WINDOW) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("{}", format_error("cx optimize", &e));
                        return EXIT_USAGE;
                    }
                };
            (deps.print_optimize)(n, json_out)
        }
        "cxworklog" | "worklog" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_RUN_WINDOW);
            (deps.print_worklog)(n)
        }
        "cx" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} cx <command> [args...]");
                return EXIT_USAGE;
            }
            (deps.cmd_cx)(&args[1..])
        }
        "cxj" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} cxj <command> [args...]");
                return EXIT_USAGE;
            }
            (deps.cmd_cxj)(&args[1..])
        }
        "cxo" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} cxo <command> [args...]");
                return EXIT_USAGE;
            }
            (deps.cmd_cxo)(&args[1..])
        }
        "cxol" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} cxol <command> [args...]");
                return EXIT_USAGE;
            }
            (deps.cmd_cxol)(&args[1..])
        }
        "cxcopy" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} cxcopy <command> [args...]");
                return EXIT_USAGE;
            }
            (deps.cmd_cxcopy)(&args[1..])
        }
        "cxpolicy" | "policy" => (deps.cmd_policy)(&args[1..]),
        "cxstate" | "state" => match args.get(1).map(String::as_str).unwrap_or("show") {
            "show" => (deps.cmd_state_show)(),
            "get" => {
                let Some(key) = args.get(2) else {
                    eprintln!("Usage: {app_name} cx state get <key>");
                    return EXIT_USAGE;
                };
                (deps.cmd_state_get)(key)
            }
            "set" => {
                let Some(key) = args.get(2) else {
                    eprintln!("Usage: {app_name} cx state set <key> <value>");
                    return EXIT_USAGE;
                };
                let Some(value) = args.get(3) else {
                    eprintln!("Usage: {app_name} cx state set <key> <value>");
                    return EXIT_USAGE;
                };
                (deps.cmd_state_set)(key, value)
            }
            other => {
                eprintln!("{app_name} cx state: unknown subcommand '{other}'");
                EXIT_USAGE
            }
        },
        "cxllm" | "llm" => (deps.cmd_llm)(&args[1..]),
        "cxbench" | "bench" => {
            let Some(runs) = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
            else {
                eprintln!("Usage: {app_name} cx bench <runs> -- <command...>");
                return EXIT_USAGE;
            };
            let delim = args.iter().position(|v| v == "--");
            let Some(i) = delim else {
                eprintln!("Usage: {app_name} cx bench <runs> -- <command...>");
                return EXIT_USAGE;
            };
            if i + 1 >= args.len() {
                eprintln!("Usage: {app_name} cx bench <runs> -- <command...>");
                return EXIT_USAGE;
            }
            (deps.cmd_bench)(runs, &args[i + 1..])
        }
        "cxprompt" | "prompt" => {
            let Some(mode) = args.get(1) else {
                eprintln!("Usage: {app_name} cx prompt <mode> <request>");
                return EXIT_USAGE;
            };
            if args.len() < 3 {
                eprintln!("Usage: {app_name} cx prompt <mode> <request>");
                return EXIT_USAGE;
            }
            (deps.cmd_prompt)(mode, &args[2..].join(" "))
        }
        "cxroles" | "roles" => (deps.cmd_roles)(args.get(1).map(String::as_str)),
        "cxfanout" | "fanout" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} cx fanout <objective>");
                return EXIT_USAGE;
            }
            (deps.cmd_fanout)(&args[1..].join(" "))
        }
        "cxpromptlint" | "promptlint" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(200);
            (deps.cmd_promptlint)(n)
        }
        "cxnext" | "next" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} cx next <command> [args...]");
                return EXIT_USAGE;
            }
            (deps.cmd_next)(&args[1..])
        }
        "cxfix" | "fix" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} cx fix <command> [args...]");
                return EXIT_USAGE;
            }
            (deps.cmd_fix)(&args[1..])
        }
        "cxdiffsum" | "diffsum" => (deps.cmd_diffsum)(false),
        "cxdiffsum_staged" | "diffsum-staged" => (deps.cmd_diffsum)(true),
        "cxcommitjson" | "commitjson" => (deps.cmd_commitjson)(),
        "cxcommitmsg" | "commitmsg" => (deps.cmd_commitmsg)(),
        "cxbudget" | "budget" => (deps.cmd_budget)(),
        "cxlog_tail" | "log-tail" => {
            let n = args
                .get(1)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(10);
            (deps.cmd_log_tail)(n)
        }
        "cxhealth" | "health" => (deps.cmd_health)(),
        "cxrtk" | "rtk-status" => (deps.cmd_rtk_status)(),
        "cxlog_on" | "log-on" => (deps.cmd_log_on)(),
        "cxlog_off" | "log-off" => (deps.cmd_log_off)(),
        "cxalert_show" | "alert-show" => (deps.cmd_alert_show)(),
        "cxalert_on" | "alert-on" => (deps.cmd_alert_on)(),
        "cxalert_off" | "alert-off" => (deps.cmd_alert_off)(),
        "cxchunk" | "chunk" => (deps.cmd_chunk)(),
        "cxfix_run" | "fix-run" => {
            if args.len() < 2 {
                eprintln!("Usage: {app_name} cx fix-run <command> [args...]");
                return EXIT_USAGE;
            }
            (deps.cmd_fix_run)(&args[1..])
        }
        "cxreplay" | "replay" => {
            let Some(id) = args.get(1) else {
                eprintln!("Usage: {app_name} cx replay <quarantine_id>");
                return EXIT_USAGE;
            };
            (deps.cmd_replay)(id)
        }
        "cxquarantine" | "quarantine" => match args.get(1).map(String::as_str).unwrap_or("list") {
            "list" => {
                let n = args
                    .get(2)
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0)
                    .unwrap_or(DEFAULT_QUARANTINE_LIST);
                (deps.cmd_quarantine_list)(n)
            }
            "show" => {
                let Some(id) = args.get(2) else {
                    eprintln!("Usage: {app_name} cx quarantine show <quarantine_id>");
                    return EXIT_USAGE;
                };
                (deps.cmd_quarantine_show)(id)
            }
            other => {
                eprintln!("{app_name} cx quarantine: unknown subcommand '{other}'");
                EXIT_USAGE
            }
        },
        other => {
            eprintln!(
                "{}",
                format_error("cx", &format!("unsupported command '{other}'"))
            );
            EXIT_USAGE
        }
    }
}

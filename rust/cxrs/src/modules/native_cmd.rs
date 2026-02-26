use crate::cmdctx::CmdCtx;
use crate::config::{DEFAULT_OPTIMIZE_WINDOW, DEFAULT_QUARANTINE_LIST, DEFAULT_RUN_WINDOW};

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

pub fn handler(ctx: &CmdCtx, args: &[String], deps: &NativeDeps) -> i32 {
    let app_name = ctx.app_name;
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    match cmd {
        "help" | "-h" | "--help" => {
            if args.get(2).map(String::as_str) == Some("task") {
                (deps.print_task_help)();
            } else {
                (deps.print_help)();
            }
            0
        }
        "version" | "-V" | "--version" => {
            (deps.print_version)();
            0
        }
        "schema" => (deps.cmd_schema)(&args[2..]),
        "logs" => (deps.cmd_logs)(&args[2..]),
        "ci" => (deps.cmd_ci)(&args[2..]),
        "core" => (deps.cmd_core)(),
        "task" => (deps.cmd_task)(&args[2..]),
        "where" => (deps.cmd_where)(&args[2..]),
        "routes" => (deps.cmd_routes)(&args[2..]),
        "diag" => (deps.cmd_diag)(),
        "parity" => (deps.cmd_parity)(),
        "supports" => {
            let Some(name) = args.get(2) else {
                eprintln!("Usage: {app_name} supports <subcommand>");
                return 2;
            };
            if (deps.is_native_name)(name) || (deps.is_compat_name)(name) {
                println!("true");
                0
            } else {
                println!("false");
                1
            }
        }
        "doctor" => (deps.cmd_doctor)(),
        "state" => match args.get(2).map(String::as_str).unwrap_or("show") {
            "show" => (deps.cmd_state_show)(),
            "get" => {
                let Some(key) = args.get(3) else {
                    eprintln!("Usage: {app_name} state get <key>");
                    return 2;
                };
                (deps.cmd_state_get)(key)
            }
            "set" => {
                let Some(key) = args.get(3) else {
                    eprintln!("Usage: {app_name} state set <key> <value>");
                    return 2;
                };
                let Some(value) = args.get(4) else {
                    eprintln!("Usage: {app_name} state set <key> <value>");
                    return 2;
                };
                (deps.cmd_state_set)(key, value)
            }
            other => {
                eprintln!("{app_name}: unknown state subcommand '{other}'");
                eprintln!("Usage: {app_name} state <show|get <key>|set <key> <value>>");
                2
            }
        },
        "llm" => (deps.cmd_llm)(&args[2..]),
        "policy" => (deps.cmd_policy)(&args[2..]),
        "bench" => {
            if args.len() < 5 {
                eprintln!("Usage: {app_name} bench <runs> -- <command...>");
                return 2;
            }
            let runs = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(0);
            let delim = args.iter().position(|v| v == "--");
            let Some(i) = delim else {
                eprintln!("Usage: {app_name} bench <runs> -- <command...>");
                return 2;
            };
            if i + 1 >= args.len() {
                eprintln!("Usage: {app_name} bench <runs> -- <command...>");
                return 2;
            }
            (deps.cmd_bench)(runs, &args[i + 1..])
        }
        "metrics" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_RUN_WINDOW);
            (deps.print_metrics)(n)
        }
        "prompt" => {
            let Some(mode) = args.get(2) else {
                eprintln!("Usage: {app_name} prompt <implement|fix|test|doc|ops> <request>");
                return 2;
            };
            if args.len() < 4 {
                eprintln!("Usage: {app_name} prompt <implement|fix|test|doc|ops> <request>");
                return 2;
            }
            let request = args[3..].join(" ");
            (deps.cmd_prompt)(mode, &request)
        }
        "roles" => (deps.cmd_roles)(args.get(2).map(String::as_str)),
        "fanout" => {
            if args.len() < 3 {
                eprintln!("Usage: {app_name} fanout <objective>");
                return 2;
            }
            (deps.cmd_fanout)(&args[2..].join(" "))
        }
        "promptlint" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_OPTIMIZE_WINDOW);
            (deps.cmd_promptlint)(n)
        }
        "cx" => {
            if args.len() < 3 {
                eprintln!("Usage: {app_name} cx <command> [args...]");
                return 2;
            }
            if (deps.is_compat_name)(&args[2]) {
                (deps.cmd_cx_compat)(&args[2..])
            } else {
                (deps.cmd_cx)(&args[2..])
            }
        }
        "cxj" => {
            if args.len() < 3 {
                eprintln!("Usage: {app_name} cxj <command> [args...]");
                return 2;
            }
            (deps.cmd_cxj)(&args[2..])
        }
        "cxo" => {
            if args.len() < 3 {
                eprintln!("Usage: {app_name} cxo <command> [args...]");
                return 2;
            }
            (deps.cmd_cxo)(&args[2..])
        }
        "cxol" => {
            if args.len() < 3 {
                eprintln!("Usage: {app_name} cxol <command> [args...]");
                return 2;
            }
            (deps.cmd_cxol)(&args[2..])
        }
        "cxcopy" => {
            if args.len() < 3 {
                eprintln!("Usage: {app_name} cxcopy <command> [args...]");
                return 2;
            }
            (deps.cmd_cxcopy)(&args[2..])
        }
        "fix" => {
            if args.len() < 3 {
                eprintln!("Usage: {app_name} fix <command> [args...]");
                return 2;
            }
            (deps.cmd_fix)(&args[2..])
        }
        "budget" => (deps.cmd_budget)(),
        "log-tail" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(10);
            (deps.cmd_log_tail)(n)
        }
        "health" => (deps.cmd_health)(),
        "rtk-status" => (deps.cmd_rtk_status)(),
        "log-on" => (deps.cmd_log_on)(),
        "log-off" => (deps.cmd_log_off)(),
        "alert-show" => (deps.cmd_alert_show)(),
        "alert-on" => (deps.cmd_alert_on)(),
        "alert-off" => (deps.cmd_alert_off)(),
        "chunk" => (deps.cmd_chunk)(),
        "cx-compat" => (deps.cmd_cx_compat)(&args[2..]),
        "profile" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_RUN_WINDOW);
            (deps.print_profile)(n)
        }
        "alert" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_RUN_WINDOW);
            (deps.print_alert)(n)
        }
        "optimize" => {
            let (n, json_out) =
                match (deps.parse_optimize_args)(&args[2..], DEFAULT_OPTIMIZE_WINDOW) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("{app_name} optimize: {e}");
                        return 2;
                    }
                };
            (deps.print_optimize)(n, json_out)
        }
        "worklog" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_RUN_WINDOW);
            (deps.print_worklog)(n)
        }
        "trace" => {
            let n = args
                .get(2)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(1);
            (deps.print_trace)(n)
        }
        "next" => {
            if args.len() < 3 {
                eprintln!("Usage: {app_name} next <command> [args...]");
                return 2;
            }
            (deps.cmd_next)(&args[2..])
        }
        "diffsum" => (deps.cmd_diffsum)(false),
        "diffsum-staged" => (deps.cmd_diffsum)(true),
        "fix-run" => {
            if args.len() < 3 {
                eprintln!("Usage: {app_name} fix-run <command> [args...]");
                return 2;
            }
            (deps.cmd_fix_run)(&args[2..])
        }
        "commitjson" => (deps.cmd_commitjson)(),
        "commitmsg" => (deps.cmd_commitmsg)(),
        "replay" => {
            let Some(id) = args.get(2) else {
                eprintln!("Usage: {app_name} replay <quarantine_id>");
                return 2;
            };
            (deps.cmd_replay)(id)
        }
        "quarantine" => match args.get(2).map(String::as_str).unwrap_or("list") {
            "list" => {
                let n = args
                    .get(3)
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0)
                    .unwrap_or(DEFAULT_QUARANTINE_LIST);
                (deps.cmd_quarantine_list)(n)
            }
            "show" => {
                let Some(id) = args.get(3) else {
                    eprintln!("Usage: {app_name} quarantine show <quarantine_id>");
                    return 2;
                };
                (deps.cmd_quarantine_show)(id)
            }
            other => {
                eprintln!("{app_name}: unknown quarantine subcommand '{other}'");
                eprintln!("Usage: {app_name} quarantine <list [N]|show <id>>");
                2
            }
        },
        _ => {
            eprintln!("{app_name}: unknown command '{cmd}'");
            eprintln!("Run '{app_name} help' for usage.");
            2
        }
    }
}

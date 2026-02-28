#[path = "modules/agentcmds.rs"]
mod agentcmds;
#[path = "modules/analytics.rs"]
mod analytics;
#[path = "modules/analytics_trace.rs"]
mod analytics_trace;
#[path = "modules/analytics_worklog.rs"]
mod analytics_worklog;
mod app;
#[path = "modules/bench_parity.rs"]
mod bench_parity;
#[path = "modules/bench_parity_mocks.rs"]
mod bench_parity_mocks;
#[path = "modules/bench_parity_support.rs"]
mod bench_parity_support;
#[path = "modules/broker.rs"]
mod broker;
#[path = "modules/capture.rs"]
mod capture;
#[path = "modules/cmdctx.rs"]
mod cmdctx;
#[path = "modules/command_names.rs"]
mod command_names;
#[path = "modules/compat_cmd.rs"]
mod compat_cmd;
#[path = "modules/config.rs"]
mod config;
#[path = "modules/diagnostics.rs"]
mod diagnostics;
#[path = "modules/doctor.rs"]
mod doctor;
#[path = "modules/error.rs"]
mod error;
#[path = "modules/execmeta.rs"]
mod execmeta;
#[path = "modules/execution.rs"]
mod execution;
#[path = "modules/execution_logging.rs"]
mod execution_logging;
#[path = "modules/help.rs"]
mod help;
#[path = "modules/introspect.rs"]
mod introspect;
#[path = "modules/llm.rs"]
mod llm;
#[path = "modules/log_contract.rs"]
mod log_contract;
#[path = "modules/logs.rs"]
mod logs;
#[path = "modules/logs_stats.rs"]
mod logs_stats;
#[path = "modules/logview.rs"]
mod logview;
#[path = "modules/native_cmd.rs"]
mod native_cmd;
#[path = "modules/optimize.rs"]
mod optimize;
#[path = "modules/optimize_print.rs"]
mod optimize_print;
#[path = "modules/optimize_report.rs"]
mod optimize_report;
#[path = "modules/optimize_rules.rs"]
mod optimize_rules;
#[path = "modules/paths.rs"]
mod paths;
#[path = "modules/policy.rs"]
mod policy;
#[path = "modules/process.rs"]
mod process;
#[path = "modules/prompting.rs"]
mod prompting;
#[path = "modules/provider_adapter.rs"]
mod provider_adapter;
#[path = "modules/quarantine.rs"]
mod quarantine;
#[path = "modules/routing.rs"]
mod routing;
#[path = "modules/runlog.rs"]
mod runlog;
#[path = "modules/runtime.rs"]
mod runtime;
#[path = "modules/runtime_controls.rs"]
mod runtime_controls;
#[path = "modules/schema.rs"]
mod schema;
#[path = "modules/schema_ops.rs"]
mod schema_ops;
#[path = "modules/settings_cmds.rs"]
mod settings_cmds;
#[path = "modules/state.rs"]
mod state;
#[path = "modules/structured_cmds.rs"]
mod structured_cmds;
#[path = "modules/structured_fixrun.rs"]
mod structured_fixrun;
#[path = "modules/structured_replay.rs"]
mod structured_replay;
#[path = "modules/task_cmds.rs"]
mod task_cmds;
#[path = "modules/taskrun.rs"]
mod taskrun;
#[path = "modules/tasks.rs"]
mod tasks;
#[path = "modules/tasks_plan.rs"]
mod tasks_plan;
#[path = "modules/types.rs"]
mod types;
#[path = "modules/util.rs"]
mod util;

fn main() {
    std::process::exit(app::run());
}

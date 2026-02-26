#[path = "modules/agentcmds.rs"]
mod agentcmds;
#[path = "modules/analytics.rs"]
mod analytics;
mod app;
#[path = "modules/bench_parity.rs"]
mod bench_parity;
#[path = "modules/capture.rs"]
mod capture;
#[path = "modules/cmdctx.rs"]
mod cmdctx;
#[path = "modules/compat_cmd.rs"]
mod compat_cmd;
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
#[path = "modules/introspect.rs"]
mod introspect;
#[path = "modules/llm.rs"]
mod llm;
#[path = "modules/logs.rs"]
mod logs;
#[path = "modules/logview.rs"]
mod logview;
#[path = "modules/optimize.rs"]
mod optimize;
#[path = "modules/paths.rs"]
mod paths;
#[path = "modules/policy.rs"]
mod policy;
#[path = "modules/prompting.rs"]
mod prompting;
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
#[path = "modules/task_cmds.rs"]
mod task_cmds;
#[path = "modules/taskrun.rs"]
mod taskrun;
#[path = "modules/tasks.rs"]
mod tasks;
#[path = "modules/types.rs"]
mod types;
#[path = "modules/util.rs"]
mod util;

fn main() {
    std::process::exit(app::run());
}

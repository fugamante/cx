mod agentcmds;
mod analytics;
mod app;
mod capture;
mod diagnostics;
mod doctor;
mod error;
mod execmeta;
mod introspect;
mod llm;
mod logs;
mod logview;
mod optimize;
mod paths;
mod policy;
mod prompting;
mod quarantine;
mod routing;
mod runlog;
mod runtime;
mod runtime_controls;
mod schema;
mod schema_ops;
mod settings_cmds;
mod state;
mod structured_cmds;
mod taskrun;
mod tasks;
mod types;
mod util;

fn main() {
    std::process::exit(app::run());
}

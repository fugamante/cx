mod app;
mod capture;
mod error;
mod execmeta;
mod llm;
mod logs;
mod optimize;
mod paths;
mod policy;
mod quarantine;
mod runlog;
mod runtime;
mod schema;
mod state;
mod taskrun;
mod tasks;
mod types;
mod util;

fn main() {
    std::process::exit(app::run());
}

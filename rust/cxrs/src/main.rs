mod app;
mod capture;
mod error;
mod llm;
mod logs;
mod paths;
mod policy;
mod quarantine;
mod execmeta;
mod runtime;
mod runlog;
mod schema;
mod state;
mod tasks;
mod taskrun;
mod types;
mod util;

fn main() {
    std::process::exit(app::run());
}

mod app;
mod capture;
mod error;
mod logs;
mod paths;
mod schema;
mod state;
mod types;
mod util;

fn main() {
    std::process::exit(app::run());
}

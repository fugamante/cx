mod app;
mod error;
mod logs;
mod paths;
mod state;
mod types;

fn main() {
    std::process::exit(app::run());
}

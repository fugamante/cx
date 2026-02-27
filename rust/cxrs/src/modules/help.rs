pub struct CommandHelp {
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
}

#[path = "help_data.rs"]
mod help_data;
#[path = "help_render.rs"]
mod help_render;

pub use help_render::{render_help, render_task_help};

use super::help_data::{MAIN_COMMANDS, TASK_COMMANDS};

fn replace_tokens(input: &str, run_window: usize, quarantine_list: usize) -> String {
    input
        .replace("{RUN_WINDOW}", &run_window.to_string())
        .replace("{QUARANTINE_LIST}", &quarantine_list.to_string())
}

pub fn render_help(
    app_name: &str,
    app_desc: &str,
    run_window: usize,
    quarantine_list: usize,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("{app_name} - {app_desc}\n\n"));
    out.push_str("Usage:\n");
    out.push_str(&format!("  {app_name} <command> [args]\n\n"));
    out.push_str("Commands:\n");
    let width = MAIN_COMMANDS
        .iter()
        .map(|c| c.usage.len().max(c.name.len()))
        .max()
        .unwrap_or(24)
        + 2;
    for c in MAIN_COMMANDS {
        let usage = replace_tokens(c.usage, run_window, quarantine_list);
        let desc = replace_tokens(c.description, run_window, quarantine_list);
        out.push_str(&format!("  {usage:<width$}{desc}\n"));
    }
    out
}

pub fn render_task_help() -> String {
    let mut out = String::new();
    out.push_str("cx help task\n\n");
    out.push_str("Task commands:\n");
    let width = TASK_COMMANDS
        .iter()
        .map(|c| c.usage.len().max(c.name.len()))
        .max()
        .unwrap_or(24)
        + 2;
    for c in TASK_COMMANDS {
        out.push_str(&format!("  {:<width$}{}\n", c.usage, c.description));
    }
    out
}

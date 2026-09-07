use super::help_data::{MAIN_COMMANDS, TASK_COMMANDS};

fn replace_tokens(input: &str, run_window: usize, quarantine_list: usize) -> String {
    input
        .replace("{RUN_WINDOW}", &run_window.to_string())
        .replace("{QUARANTINE_LIST}", &quarantine_list.to_string())
}

fn replace_tokens_app(
    input: &str,
    app_name: &str,
    run_window: usize,
    quarantine_list: usize,
) -> String {
    replace_tokens(input, run_window, quarantine_list).replace("{APP}", app_name)
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
        let usage = replace_tokens_app(c.usage, app_name, run_window, quarantine_list);
        let desc = replace_tokens_app(c.description, app_name, run_window, quarantine_list);
        out.push_str(&format!("  {usage:<width$}{desc}\n"));
    }
    out
}

pub fn render_task_help(app_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("{app_name} help task\n\n"));
    out.push_str("Task commands:\n");
    let width = TASK_COMMANDS
        .iter()
        .map(|c| c.usage.len().max(c.name.len()))
        .max()
        .unwrap_or(24)
        + 2;
    for c in TASK_COMMANDS {
        let usage = replace_tokens_app(c.usage, app_name, 0, 0);
        out.push_str(&format!("  {usage:<width$}{}\n", c.description));
    }
    out.push_str("\nExamples:\n");
    out.push_str(&format!(
        "  {app_name} task run task_001 --mode deterministic --backend primary\n"
    ));
    out.push_str(&format!(
        "  {app_name} task run-all --status pending --mode mixed\n"
    ));
    out.push_str(&format!(
        "  {app_name} task run-all --status pending --mode parallel --strict-plan --max-workers 2\n"
    ));
    out.push_str(&format!(
        "  {app_name} task run-all --status pending --mode parallel --plan-json --json | jq .\n"
    ));
    out.push_str(&format!(
        "  {app_name} task run-all --status pending --dry-run --json | jq .\n"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::render_task_help;

    #[test]
    fn task_help_examples() {
        let text = render_task_help("xshelf");
        assert!(text.contains("Examples:"));
        assert!(text.contains("xshelf task run task_001 --mode deterministic --backend primary"));
        assert!(text.contains("xshelf task run-all --status pending --mode mixed"));
        assert!(text.contains(
            "xshelf task run-all --status pending --mode parallel --strict-plan --max-workers 2"
        ));
    }
}

use crate::config::cli_app_name;
use crate::logs::load_runs;
use crate::paths::resolve_log_file;

fn show_field<T: ToString>(label: &str, value: Option<T>) {
    match value {
        Some(v) => println!("{label}: {}", v.to_string()),
        None => println!("{label}: n/a"),
    }
}

pub fn print_trace(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        crate::cx_eprintln!("{}: unable to resolve log file", cli_app_name());
        return 1;
    };
    if !log_file.exists() {
        crate::cx_eprintln!(
            "{} trace: no log file at {}",
            cli_app_name(),
            log_file.display()
        );
        return 1;
    }

    let runs = match load_runs(&log_file, usize::MAX) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} trace: {e}", cli_app_name());
            return 1;
        }
    };
    if runs.is_empty() {
        crate::cx_eprintln!(
            "{} trace: no runs in {}",
            cli_app_name(),
            log_file.display()
        );
        return 1;
    }
    if n == 0 || n > runs.len() {
        crate::cx_eprintln!(
            "{} trace: run index out of range (requested {}, available {})",
            cli_app_name(),
            n,
            runs.len()
        );
        return 2;
    }
    let idx = runs.len() - n;
    let run = runs.get(idx).cloned().unwrap_or_default();

    println!("== {} trace (run #{n} most recent) ==", cli_app_name());
    show_field("ts", run.ts);
    show_field("tool", run.tool);
    show_field("cwd", run.cwd);
    show_field("duration_ms", run.duration_ms);
    show_field("input_tokens", run.input_tokens);
    show_field("cached_input_tokens", run.cached_input_tokens);
    show_field("effective_input_tokens", run.effective_input_tokens);
    show_field("output_tokens", run.output_tokens);
    show_field("scope", run.scope);
    show_field("repo_root", run.repo_root);
    show_field("llm_backend", run.llm_backend);
    show_field("llm_model", run.llm_model);
    show_field("prompt_sha256", run.prompt_sha256);
    show_field("prompt_preview", run.prompt_preview);
    println!("log_file: {}", log_file.display());
    0
}

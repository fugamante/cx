use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReduceProfile {
    Fast,
    Balanced,
    Deep,
}

fn reduce_profile_from_env() -> ReduceProfile {
    match env::var("CX_CAPTURE_PROFILE")
        .unwrap_or_else(|_| "balanced".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "fast" => ReduceProfile::Fast,
        "deep" => ReduceProfile::Deep,
        _ => ReduceProfile::Balanced,
    }
}

fn normalize_generic(input: &str) -> String {
    let mut out = String::new();
    let mut blank_seen = false;
    for mut line in input.lines().map(|l| l.to_string()) {
        if line.trim().is_empty() {
            if !blank_seen {
                out.push('\n');
            }
            blank_seen = true;
            continue;
        }
        blank_seen = false;
        if line.chars().count() > 600 {
            line = format!("{}...", line.chars().take(600).collect::<String>());
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn reduce_git_status(input: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in input.lines() {
        let t = line.trim_start();
        if line.starts_with("On branch ")
            || line.starts_with("HEAD detached")
            || line.starts_with("Your branch ")
            || line.starts_with("Changes to be committed:")
            || line.starts_with("Changes not staged for commit:")
            || line.starts_with("Untracked files:")
            || line.starts_with("nothing to commit")
            || line.starts_with("no changes added to commit")
            || t.starts_with("modified:")
            || t.starts_with("new file:")
            || t.starts_with("deleted:")
            || t.starts_with("renamed:")
            || t.starts_with("both modified:")
            || t.starts_with("both added:")
            || t.starts_with("both deleted:")
        {
            out.push(line.to_string());
        }
    }
    if out.is_empty() {
        input
            .lines()
            .take(120)
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        out.join("\n")
    }
}

fn reduce_diff_like(input: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut changed = 0usize;
    for line in input.lines() {
        if line.starts_with("diff --git ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("@@ ")
            || line.starts_with("Binary files ")
            || line.starts_with("rename from ")
            || line.starts_with("rename to ")
        {
            out.push(line.to_string());
        } else if (line.starts_with('+') || line.starts_with('-')) && changed < 300 {
            out.push(line.to_string());
            changed += 1;
        }
    }
    if out.is_empty() {
        input.to_string()
    } else {
        out.join("\n")
    }
}

fn reduce_git_log(input: &str) -> String {
    input
        .lines()
        .filter(|line| {
            line.starts_with("commit ")
                || line.starts_with("Author:")
                || line.starts_with("Date:")
                || line.trim_start().starts_with('*')
                || line.trim_start().starts_with('-')
                || line.trim_start().starts_with("Merge:")
        })
        .take(250)
        .collect::<Vec<_>>()
        .join("\n")
}

fn reduce_grep_like(input: &str) -> String {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(400)
        .collect::<Vec<_>>()
        .join("\n")
}

fn reduce_test_output(input: &str) -> String {
    input
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("fail")
                || lower.contains("error")
                || lower.contains("panic")
                || lower.contains("warning")
                || lower.contains("passed")
                || lower.contains("test result")
                || lower.contains("running ")
        })
        .take(400)
        .collect::<Vec<_>>()
        .join("\n")
}

fn reduce_tree_or_ls(input: &str) -> String {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(300)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn native_reduce_output(cmd: &[String], input: &str) -> String {
    let profile = reduce_profile_from_env();
    let cmd0 = cmd.first().map(String::as_str).unwrap_or("");
    let cmd1 = cmd.get(1).map(String::as_str).unwrap_or("");
    let reduced = match (cmd0, cmd1, profile) {
        ("git", "status", _) => reduce_git_status(input),
        ("git", "diff", _) | ("diff", _, _) => reduce_diff_like(input),
        ("git", "log", _) | ("log", _, _) => reduce_git_log(input),
        ("grep", _, _) => reduce_grep_like(input),
        ("tree", _, _) | ("ls", _, _) => reduce_tree_or_ls(input),
        ("test", _, _) => reduce_test_output(input),
        (_, _, ReduceProfile::Deep) => reduce_test_output(input),
        _ => input.to_string(),
    };
    normalize_generic(&reduced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduce_git_status_keeps_semantic_lines() {
        let input = "On branch main\n  modified: src/main.rs\nrandom noise\n";
        let out = native_reduce_output(&["git".into(), "status".into()], input);
        assert!(out.contains("On branch main"));
        assert!(out.contains("modified: src/main.rs"));
        assert!(!out.contains("random noise"));
    }

    #[test]
    fn reduce_test_output_surfaces_failures() {
        let input = "line 1\nFAIL test_x\nwarning: foo\nline 2\n";
        let out = native_reduce_output(&["test".into()], input);
        assert!(out.contains("FAIL test_x"));
        assert!(out.contains("warning: foo"));
    }
}

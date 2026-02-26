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

pub fn native_reduce_output(cmd: &[String], input: &str) -> String {
    let cmd0 = cmd.first().map(String::as_str).unwrap_or("");
    let cmd1 = cmd.get(1).map(String::as_str).unwrap_or("");
    let reduced = match (cmd0, cmd1) {
        ("git", "status") => reduce_git_status(input),
        ("git", "diff") | ("diff", _) => reduce_diff_like(input),
        _ => input.to_string(),
    };
    normalize_generic(&reduced)
}

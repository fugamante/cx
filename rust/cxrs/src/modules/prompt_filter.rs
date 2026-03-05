#[derive(Debug, Clone)]
pub struct PromptTransform {
    pub filtered: String,
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            let s = v.trim().to_ascii_lowercase();
            matches!(s.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(default)
}

fn env_usize_opt(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
}

fn compact_prompt(raw: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut blank_count = 0usize;
    for line in raw.lines() {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                out.push(String::new());
            }
        } else {
            blank_count = 0;
            out.push(trimmed_end.to_string());
        }
    }
    out.join("\n").trim().to_string()
}

fn clip_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect::<String>()
}

pub fn process_prompt(raw: &str, schema_enforced: bool) -> PromptTransform {
    let enabled = env_bool("CX_PROMPT_FILTER", true);
    let strict = env_bool("CX_PROMPT_FILTER_STRICT", false);
    if !enabled || (schema_enforced && !strict) {
        return PromptTransform {
            filtered: raw.to_string(),
        };
    }

    let mut filtered = compact_prompt(raw);
    if let Some(max_chars) = env_usize_opt("CX_PROMPT_FILTER_MAX_CHARS")
        && filtered.chars().count() > max_chars
    {
        filtered = clip_chars(&filtered, max_chars);
    }
    PromptTransform { filtered }
}

#[cfg(test)]
mod tests {
    use super::process_prompt;

    #[test]
    fn prompt_filter_compacts_blank_lines_when_enabled() {
        // SAFETY: tests run in-process and intentionally toggle env vars.
        unsafe {
            std::env::set_var("CX_PROMPT_FILTER", "1");
            std::env::set_var("CX_PROMPT_FILTER_STRICT", "1");
        }
        let tx = process_prompt("line1\n\n\nline2   \n", false);
        assert_eq!(tx.filtered, "line1\n\nline2");
    }

    #[test]
    fn prompt_filter_bypasses_schema_prompts_by_default() {
        // SAFETY: tests run in-process and intentionally toggle env vars.
        unsafe {
            std::env::set_var("CX_PROMPT_FILTER", "1");
            std::env::set_var("CX_PROMPT_FILTER_STRICT", "0");
        }
        let raw = "a\n\n\nb  ";
        let tx = process_prompt(raw, true);
        assert_eq!(tx.filtered, raw);
    }
}

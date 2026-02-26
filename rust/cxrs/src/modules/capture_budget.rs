use crate::config::app_config;
use crate::types::CaptureStats;

#[derive(Debug, Clone)]
pub struct BudgetConfig {
    pub budget_chars: usize,
    pub budget_lines: usize,
    pub clip_mode: String,
    pub clip_footer: bool,
}

pub fn budget_config_from_env() -> BudgetConfig {
    let cfg = app_config();
    BudgetConfig {
        budget_chars: cfg.budget_chars,
        budget_lines: cfg.budget_lines,
        clip_mode: cfg.clip_mode.clone(),
        clip_footer: cfg.clip_footer,
    }
}

pub fn choose_clip_mode(input: &str, configured_mode: &str) -> String {
    match configured_mode {
        "head" => "head".to_string(),
        "tail" => "tail".to_string(),
        _ => {
            let lower = input.to_lowercase();
            if lower.contains("error") || lower.contains("fail") || lower.contains("warning") {
                "tail".to_string()
            } else {
                "head".to_string()
            }
        }
    }
}

fn first_n_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn last_n_chars(s: &str, n: usize) -> String {
    let total = s.chars().count();
    if n >= total {
        return s.to_string();
    }
    s.chars().skip(total - n).collect()
}

pub fn clip_text_with_config(input: &str, cfg: &BudgetConfig) -> (String, CaptureStats) {
    let original_chars = input.chars().count();
    let original_lines = input.lines().count();
    let mode_used = choose_clip_mode(input, &cfg.clip_mode);
    let lines: Vec<&str> = input.lines().collect();
    let line_limited = if lines.len() <= cfg.budget_lines {
        input.to_string()
    } else if mode_used == "tail" {
        lines[lines.len().saturating_sub(cfg.budget_lines)..].join("\n")
    } else {
        lines[..cfg.budget_lines].join("\n")
    };
    let char_limited = if line_limited.chars().count() <= cfg.budget_chars {
        line_limited
    } else if mode_used == "tail" {
        last_n_chars(&line_limited, cfg.budget_chars)
    } else {
        first_n_chars(&line_limited, cfg.budget_chars)
    };
    let kept_chars = char_limited.chars().count();
    let kept_lines = char_limited.lines().count();
    let clipped = kept_chars < original_chars || kept_lines < original_lines;
    let final_text = if clipped && cfg.clip_footer {
        format!(
            "{char_limited}\n[cx] output clipped: original={}/{}, kept={}/{}, mode={}",
            original_chars, original_lines, kept_chars, kept_lines, mode_used
        )
    } else {
        char_limited
    };
    (
        final_text,
        CaptureStats {
            system_output_len_raw: Some(original_chars as u64),
            system_output_len_processed: Some(input.chars().count() as u64),
            system_output_len_clipped: Some(kept_chars as u64),
            system_output_lines_raw: Some(original_lines as u64),
            system_output_lines_processed: Some(input.lines().count() as u64),
            system_output_lines_clipped: Some(kept_lines as u64),
            clipped: Some(clipped),
            budget_chars: Some(cfg.budget_chars as u64),
            budget_lines: Some(cfg.budget_lines as u64),
            clip_mode: Some(mode_used),
            clip_footer: Some(cfg.clip_footer),
            rtk_used: None,
            capture_provider: None,
        },
    )
}

pub fn chunk_text_by_budget(input: &str, chunk_chars: usize) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_chars = 0usize;
    for line in input.lines() {
        let line_chars = line.chars().count() + 1;
        if cur_chars > 0 && cur_chars + line_chars > chunk_chars {
            chunks.push(cur);
            cur = String::new();
            cur_chars = 0;
        }
        cur.push_str(line);
        cur.push('\n');
        cur_chars += line_chars;
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    if chunks.is_empty() {
        vec![String::new()]
    } else {
        chunks
    }
}

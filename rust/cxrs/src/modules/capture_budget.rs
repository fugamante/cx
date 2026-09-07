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
            "{char_limited}\n[XSHELF] output clipped: original={}/{}, kept={}/{}, mode={}",
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
            capture_prompt_profile: None,
            capture_prompt_profile_applied: None,
            capture_prompt_reducer_kind: None,
            capture_prompt_fallback_reason: None,
        },
    )
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SectionPriority {
    Critical,
    High,
    Medium,
    Low,
}

#[allow(dead_code)]
impl SectionPriority {
    fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct PromptSection<'a> {
    pub id: &'a str,
    pub priority: SectionPriority,
    pub text: &'a str,
    pub uncertainty: &'a str,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SectionOmission {
    pub id: String,
    pub priority: &'static str,
    pub chars: usize,
    pub lines: usize,
    pub reason: &'static str,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssemblyResult {
    pub text: String,
    pub omissions: Vec<SectionOmission>,
    pub clipped: bool,
}

#[allow(dead_code)]
fn section_block(section: &PromptSection<'_>) -> String {
    format!(
        "[{}] {}\n{}\n",
        section.priority.as_str(),
        section.id,
        section.text.trim_end()
    )
}

#[allow(dead_code)]
fn block_fits(current: &str, block: &str, cfg: &BudgetConfig) -> bool {
    current.chars().count() + block.chars().count() <= cfg.budget_chars
        && current.lines().count() + block.lines().count() <= cfg.budget_lines
}

#[allow(dead_code)]
fn omission(section: &PromptSection<'_>, reason: &'static str) -> SectionOmission {
    SectionOmission {
        id: section.id.to_string(),
        priority: section.priority.as_str(),
        chars: section.text.chars().count(),
        lines: section.text.lines().count(),
        reason,
    }
}

#[allow(dead_code)]
pub(crate) fn assemble_sections_with_config(
    sections: &[PromptSection<'_>],
    cfg: &BudgetConfig,
) -> AssemblyResult {
    let mut ordered: Vec<(usize, &PromptSection<'_>)> = sections.iter().enumerate().collect();
    ordered.sort_by_key(|(idx, section)| {
        let rank = if section.uncertainty == "high" {
            SectionPriority::Critical.rank()
        } else {
            section.priority.rank()
        };
        (rank, *idx)
    });

    let mut text = String::new();
    let mut omissions = Vec::new();
    for (_, section) in ordered {
        let block = section_block(section);
        if block_fits(&text, &block, cfg) {
            text.push_str(&block);
            continue;
        }
        if text.is_empty()
            && (section.priority == SectionPriority::Critical || section.uncertainty == "high")
        {
            let (clipped, _) = clip_text_with_config(&block, cfg);
            text.push_str(&clipped);
            omissions.push(omission(section, "clipped_section"));
            continue;
        }
        let reason = if section.uncertainty == "high" {
            "omitted_high_uncertainty_over_budget"
        } else {
            "omitted_over_budget"
        };
        omissions.push(omission(section, reason));
    }

    AssemblyResult {
        text,
        clipped: !omissions.is_empty(),
        omissions,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_budget(chars: usize, lines: usize) -> BudgetConfig {
        BudgetConfig {
            budget_chars: chars,
            budget_lines: lines,
            clip_mode: "head".to_string(),
            clip_footer: false,
        }
    }

    #[test]
    fn assembly_omits_low_priority_first() {
        let sections = [
            PromptSection {
                id: "low-noise",
                priority: SectionPriority::Low,
                text: "low context that should drop",
                uncertainty: "low",
            },
            PromptSection {
                id: "critical-error",
                priority: SectionPriority::Critical,
                text: "error: root cause",
                uncertainty: "low",
            },
            PromptSection {
                id: "high-context",
                priority: SectionPriority::High,
                text: "nearby stack frame",
                uncertainty: "low",
            },
        ];
        let result = assemble_sections_with_config(&sections, &test_budget(100, 8));
        assert!(result.text.contains("[critical] critical-error"));
        assert!(result.text.contains("[high] high-context"));
        assert!(!result.text.contains("low-noise"));
        assert_eq!(result.omissions.len(), 1);
        assert_eq!(result.omissions[0].id, "low-noise");
        assert_eq!(result.omissions[0].reason, "omitted_over_budget");
    }

    #[test]
    fn assembly_preserves_same_priority_order() {
        let sections = [
            PromptSection {
                id: "first",
                priority: SectionPriority::High,
                text: "first high",
                uncertainty: "low",
            },
            PromptSection {
                id: "second",
                priority: SectionPriority::High,
                text: "second high",
                uncertainty: "low",
            },
            PromptSection {
                id: "third",
                priority: SectionPriority::Medium,
                text: "third medium",
                uncertainty: "low",
            },
        ];
        let result = assemble_sections_with_config(&sections, &test_budget(300, 20));
        let first = result.text.find("[high] first").expect("first section");
        let second = result.text.find("[high] second").expect("second section");
        let third = result.text.find("[medium] third").expect("third section");
        assert!(first < second);
        assert!(second < third);
        assert!(result.omissions.is_empty());
    }

    #[test]
    fn assembly_promotes_high_uncertainty_section() {
        let sections = [
            PromptSection {
                id: "normal-critical",
                priority: SectionPriority::Critical,
                text: "known critical",
                uncertainty: "low",
            },
            PromptSection {
                id: "uncertain-medium",
                priority: SectionPriority::Medium,
                text: "uncertain reducer fallback evidence",
                uncertainty: "high",
            },
            PromptSection {
                id: "normal-high",
                priority: SectionPriority::High,
                text: "ordinary nearby context",
                uncertainty: "low",
            },
        ];
        let result = assemble_sections_with_config(&sections, &test_budget(120, 8));
        assert!(result.text.contains("normal-critical"));
        assert!(result.text.contains("uncertain-medium"));
        assert!(!result.text.contains("normal-high"));
        assert_eq!(result.omissions[0].id, "normal-high");
    }

    #[test]
    fn assembly_clips_oversized_critical_section() {
        let sections = [PromptSection {
            id: "critical-long",
            priority: SectionPriority::Critical,
            text: "0123456789abcdefghijklmnopqrstuvwxyz",
            uncertainty: "low",
        }];
        let result = assemble_sections_with_config(&sections, &test_budget(24, 10));
        assert!(result.clipped);
        assert!(result.text.chars().count() <= 24);
        assert_eq!(result.omissions.len(), 1);
        assert_eq!(result.omissions[0].reason, "clipped_section");
    }
}

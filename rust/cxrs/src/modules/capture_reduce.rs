use std::collections::HashSet;
use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReduceProfile {
    Fast,
    Balanced,
    Deep,
}

impl ReduceProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Deep => "deep",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReducerKind {
    GenericPassthrough,
    GitStatus,
    GitDiff,
    GitLog,
    Grep,
    TreeLs,
    TestOutput,
    DeepFallback,
}

impl ReducerKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::GenericPassthrough => "generic_passthrough",
            Self::GitStatus => "git_status",
            Self::GitDiff => "git_diff",
            Self::GitLog => "git_log",
            Self::Grep => "grep_like",
            Self::TreeLs => "tree_ls",
            Self::TestOutput => "test_output",
            Self::DeepFallback => "deep_fallback",
        }
    }

    fn lossiness_level(self) -> &'static str {
        match self {
            Self::GenericPassthrough => "lossless",
            Self::DeepFallback => "uncertain_fallback",
            _ => "semantic_extract",
        }
    }

    fn uncertainty(self) -> &'static str {
        match self {
            Self::DeepFallback => "high",
            _ => "low",
        }
    }

    fn critical_sections(self) -> Vec<&'static str> {
        match self {
            Self::GitStatus => vec!["branch_state", "touched_paths"],
            Self::GitDiff => vec!["diff_headers", "hunk_headers", "changed_lines"],
            Self::GitLog => vec!["commit_ids", "authors", "dates"],
            Self::Grep => vec!["matching_lines"],
            Self::TreeLs => vec!["listed_paths"],
            Self::TestOutput | Self::DeepFallback => vec![
                "failing_test_names",
                "panic_error_assertion_warning_snippets",
                "final_summary",
            ],
            Self::GenericPassthrough => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReductionMetadata {
    pub reducer_kind: &'static str,
    pub reducer_version: u32,
    pub profile: &'static str,
    pub lossiness_level: &'static str,
    pub raw_chars: usize,
    pub reduced_chars: usize,
    pub clipped_chars: usize,
    pub omitted_lines: usize,
    pub omitted_chars: usize,
    pub critical_sections_kept: Vec<&'static str>,
    pub uncertainty: &'static str,
    pub replay_pointer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReductionResult {
    pub text: String,
    pub metadata: ReductionMetadata,
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

fn select_reducer(cmd: &[String], profile: ReduceProfile) -> ReducerKind {
    let cmd0 = cmd.first().map(String::as_str).unwrap_or("");
    let cmd1 = cmd.get(1).map(String::as_str).unwrap_or("");
    match (cmd0, cmd1, profile) {
        ("git", "status", _) => ReducerKind::GitStatus,
        ("git", "diff", _) | ("diff", _, _) => ReducerKind::GitDiff,
        ("git", "log", _) | ("log", _, _) => ReducerKind::GitLog,
        ("grep", _, _) => ReducerKind::Grep,
        ("tree", _, _) | ("ls", _, _) => ReducerKind::TreeLs,
        ("test", _, _) => ReducerKind::TestOutput,
        (_, _, ReduceProfile::Deep) => ReducerKind::DeepFallback,
        _ => ReducerKind::GenericPassthrough,
    }
}

fn reduce_by_kind(kind: ReducerKind, input: &str) -> String {
    match kind {
        ReducerKind::GitStatus => reduce_git_status(input),
        ReducerKind::GitDiff => reduce_diff_like(input),
        ReducerKind::GitLog => reduce_git_log(input),
        ReducerKind::Grep => reduce_grep_like(input),
        ReducerKind::TreeLs => reduce_tree_or_ls(input),
        ReducerKind::TestOutput | ReducerKind::DeepFallback => reduce_test_output(input),
        ReducerKind::GenericPassthrough => input.to_string(),
    }
}

fn reduction_metadata(
    kind: ReducerKind,
    profile: ReduceProfile,
    input: &str,
    output: &str,
) -> ReductionMetadata {
    let raw_chars = input.chars().count();
    let reduced_chars = output.chars().count();
    let raw_lines = input.lines().count();
    let reduced_lines = output.lines().count();
    ReductionMetadata {
        reducer_kind: kind.as_str(),
        reducer_version: 1,
        profile: profile.as_str(),
        lossiness_level: kind.lossiness_level(),
        raw_chars,
        reduced_chars,
        clipped_chars: 0,
        omitted_lines: raw_lines.saturating_sub(reduced_lines),
        omitted_chars: raw_chars.saturating_sub(reduced_chars),
        critical_sections_kept: kind.critical_sections(),
        uncertainty: kind.uncertainty(),
        replay_pointer: None,
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
            || line.starts_with("new file mode ")
            || line.starts_with("deleted file mode ")
            || line.starts_with("old mode ")
            || line.starts_with("new mode ")
            || line.starts_with("similarity index ")
            || line.starts_with("dissimilarity index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("@@ ")
            || line.starts_with("Binary files ")
            || line.starts_with("rename from ")
            || line.starts_with("rename to ")
            || line.starts_with("copy from ")
            || line.starts_with("copy to ")
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
    let mut out: Vec<String> = Vec::new();
    let mut seen_warnings: HashSet<String> = HashSet::new();
    let mut context_lines = 0usize;

    for line in input.lines() {
        let lower = line.to_ascii_lowercase();
        let lower_trim = lower.trim_start();
        let actual_panic = lower_trim.starts_with("thread ") && lower.contains("panicked");
        let actual_assertion = lower_trim.starts_with("assertion ");
        let keep = lower.contains("fail")
            || lower.contains("error")
            || actual_panic
            || lower.contains("warning")
            || actual_assertion
            || lower.contains("test result")
            || lower.contains("running ")
            || lower_trim.starts_with("left:")
            || lower_trim.starts_with("right:")
            || lower_trim.starts_with("note:")
            || lower_trim.starts_with("failures:");

        if keep {
            if lower.contains("warning") && !seen_warnings.insert(line.to_string()) {
                continue;
            }
            out.push(line.to_string());
            if actual_panic || actual_assertion {
                context_lines = context_lines.max(3);
            }
        } else if context_lines > 0 {
            out.push(line.to_string());
            context_lines -= 1;
        }

        if out.len() >= 400 {
            break;
        }
    }

    out.join("\n")
}

fn reduce_tree_or_ls(input: &str) -> String {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(300)
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(dead_code)]
pub fn native_reduce_output(cmd: &[String], input: &str) -> String {
    native_reduce_output_with_metadata(cmd, input).text
}

pub fn native_reduce_output_with_metadata(cmd: &[String], input: &str) -> ReductionResult {
    let profile = reduce_profile_from_env();
    let kind = select_reducer(cmd, profile);
    let reduced = reduce_by_kind(kind, input);
    let text = normalize_generic(&reduced);
    let metadata = reduction_metadata(kind, profile, input, &text);
    ReductionResult { text, metadata }
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

    #[test]
    fn reduction_metadata_reports_git_diff_shape() {
        let input = "\
diff --git a/src/main.rs b/src/main.rs
index 111..222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,2 +1,2 @@
-old
+new
 unchanged
";
        let result = native_reduce_output_with_metadata(&["git".into(), "diff".into()], input);
        assert!(
            result
                .text
                .contains("diff --git a/src/main.rs b/src/main.rs")
        );
        assert!(result.text.contains("+new"));
        assert!(!result.text.contains(" unchanged"));
        assert_eq!(result.metadata.reducer_kind, "git_diff");
        assert_eq!(result.metadata.reducer_version, 1);
        assert_eq!(result.metadata.lossiness_level, "semantic_extract");
        assert_eq!(result.metadata.uncertainty, "low");
        assert_eq!(result.metadata.clipped_chars, 0);
        assert!(result.metadata.omitted_lines > 0);
        assert!(
            result
                .metadata
                .critical_sections_kept
                .contains(&"changed_lines")
        );
    }

    #[test]
    fn string_reducer_matches_metadata_text() {
        let input = "On branch main\n  modified: src/main.rs\nrandom noise\n";
        let cmd = ["git".into(), "status".into()];
        let text = native_reduce_output(&cmd, input);
        let result = native_reduce_output_with_metadata(&cmd, input);
        assert_eq!(text, result.text);
        assert_eq!(result.metadata.reducer_kind, "git_status");
        assert_eq!(result.metadata.raw_chars, input.chars().count());
        assert_eq!(result.metadata.reduced_chars, result.text.chars().count());
    }

    #[test]
    fn passthrough_metadata_is_lossless() {
        let input = "plain output\nkept as-is\n";
        let metadata = reduction_metadata(
            ReducerKind::GenericPassthrough,
            ReduceProfile::Balanced,
            input,
            input,
        );
        assert_eq!(metadata.reducer_kind, "generic_passthrough");
        assert_eq!(metadata.profile, "balanced");
        assert_eq!(metadata.lossiness_level, "lossless");
        assert_eq!(metadata.omitted_lines, 0);
        assert_eq!(metadata.omitted_chars, 0);
        assert!(metadata.critical_sections_kept.is_empty());
    }

    #[test]
    fn test_output_fixture_retains_required_spans() {
        let input = include_str!("../../tests/fixtures/phase_x/cargo_test_failure.txt");
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/phase_x/test_output_reducer_manifest.json"
        ))
        .expect("parse fixture manifest");
        let command = manifest
            .get("command")
            .and_then(serde_json::Value::as_array)
            .expect("command")
            .iter()
            .map(|v| v.as_str().expect("command string").to_string())
            .collect::<Vec<String>>();

        let result = native_reduce_output_with_metadata(&command, input);
        assert_eq!(
            result.metadata.reducer_kind,
            manifest
                .get("expected_reducer_kind")
                .and_then(serde_json::Value::as_str)
                .expect("expected reducer kind")
        );
        assert_eq!(
            result.metadata.lossiness_level,
            manifest
                .get("expected_lossiness_level")
                .and_then(serde_json::Value::as_str)
                .expect("expected lossiness")
        );
        assert_eq!(
            result.metadata.uncertainty,
            manifest
                .get("max_uncertainty")
                .and_then(serde_json::Value::as_str)
                .expect("max uncertainty")
        );

        for span in manifest
            .get("required_spans")
            .and_then(serde_json::Value::as_array)
            .expect("required spans")
        {
            let span = span.as_str().expect("span string");
            assert!(result.text.contains(span), "missing required span: {span}");
        }
        for span in manifest
            .get("forbidden_spans")
            .and_then(serde_json::Value::as_array)
            .expect("forbidden spans")
        {
            let span = span.as_str().expect("span string");
            assert!(!result.text.contains(span), "kept forbidden span: {span}");
        }

        assert_eq!(
            result
                .text
                .matches("warning: unused variable: `noise`")
                .count(),
            1
        );
        assert!(result.metadata.reduced_chars < result.metadata.raw_chars);
        let min_reduction_ratio = manifest
            .get("min_reduction_ratio")
            .and_then(serde_json::Value::as_f64)
            .expect("min reduction ratio");
        let actual_reduction_ratio = (result.metadata.raw_chars - result.metadata.reduced_chars)
            as f64
            / result.metadata.raw_chars as f64;
        assert!(
            actual_reduction_ratio >= min_reduction_ratio,
            "reduction ratio {actual_reduction_ratio} below {min_reduction_ratio}"
        );
        assert!(
            result
                .metadata
                .critical_sections_kept
                .contains(&"final_summary")
        );
    }

    #[test]
    fn diff_fixture_retains_structural_spans() {
        let input = include_str!("../../tests/fixtures/phase_x/git_diff_mixed.txt");
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/phase_x/diff_reducer_manifest.json"
        ))
        .expect("parse diff fixture manifest");
        let command = manifest
            .get("command")
            .and_then(serde_json::Value::as_array)
            .expect("command")
            .iter()
            .map(|v| v.as_str().expect("command string").to_string())
            .collect::<Vec<String>>();

        let result = native_reduce_output_with_metadata(&command, input);
        assert_eq!(
            result.metadata.reducer_kind,
            manifest
                .get("expected_reducer_kind")
                .and_then(serde_json::Value::as_str)
                .expect("expected reducer kind")
        );
        assert_eq!(
            result.metadata.lossiness_level,
            manifest
                .get("expected_lossiness_level")
                .and_then(serde_json::Value::as_str)
                .expect("expected lossiness")
        );
        assert_eq!(
            result.metadata.uncertainty,
            manifest
                .get("max_uncertainty")
                .and_then(serde_json::Value::as_str)
                .expect("max uncertainty")
        );

        for span in manifest
            .get("required_spans")
            .and_then(serde_json::Value::as_array)
            .expect("required spans")
        {
            let span = span.as_str().expect("span string");
            assert!(result.text.contains(span), "missing required span: {span}");
        }
        for span in manifest
            .get("forbidden_spans")
            .and_then(serde_json::Value::as_array)
            .expect("forbidden spans")
        {
            let span = span.as_str().expect("span string");
            assert!(!result.text.contains(span), "kept forbidden span: {span}");
        }

        let min_reduction_ratio = manifest
            .get("min_reduction_ratio")
            .and_then(serde_json::Value::as_f64)
            .expect("min reduction ratio");
        let actual_reduction_ratio = (result.metadata.raw_chars - result.metadata.reduced_chars)
            as f64
            / result.metadata.raw_chars as f64;
        assert!(
            actual_reduction_ratio >= min_reduction_ratio,
            "reduction ratio {actual_reduction_ratio} below {min_reduction_ratio}"
        );
        for section in ["diff_headers", "hunk_headers", "changed_lines"] {
            assert!(
                result.metadata.critical_sections_kept.contains(&section),
                "missing critical section: {section}"
            );
        }
    }
}

//! Project instructions auto-load.
//!
//! When a session is started for a project, WebCodex best-effort loads
//! project-local instruction files (e.g. `AGENTS.md`) so GPT Action / MCP /
//! Codex / GLM callers see project-local development rules at session start.
//!
//! These files are project-local guidance only; they never override system,
//! platform, or WebCodex safety policy. Only a fixed candidate whitelist is
//! read; arbitrary caller-supplied paths and secrets are never read. Read
//! failures never cause `start_session` to fail — a candidate that cannot be
//! read is simply skipped.

use serde::Serialize;

/// Conservative total character cap across all loaded instruction files.
pub(crate) const MAX_TOTAL_CHARS: usize = 32 * 1024;
/// Conservative per-file line cap.
pub(crate) const MAX_LINES_PER_FILE: usize = 400;

/// Fixed, ordered candidate instruction file paths tried at session start.
pub(crate) const INSTRUCTION_CANDIDATE_PATHS: &[&str] = &[
    "AGENTS.md",
    "agents.md",
    "CLAUDE.md",
    ".codex/AGENTS.md",
    ".github/copilot-instructions.md",
];

const PROJECT_INSTRUCTIONS_NOTE: &str = "Project instructions are project-local guidance only; they do not override system, platform, or WebCodex safety policy.";

/// Hint for reading the remainder of a truncated instruction file via
/// `read_file` (`path` / `start_line` / `limit`).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReadMoreHint {
    pub(crate) path: String,
    pub(crate) start_line: usize,
    pub(crate) limit: usize,
}

/// One loaded instruction file (full content). Returned by `start_session`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectInstructionFile {
    pub(crate) path: String,
    pub(crate) content: String,
    pub(crate) chars: usize,
    pub(crate) total_lines: usize,
    pub(crate) start_line: usize,
    pub(crate) limit: usize,
    pub(crate) truncated: bool,
    pub(crate) read_more: Option<ReadMoreHint>,
}

/// Summary projection of one instruction file (no content). Returned by
/// `session_summary`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectInstructionFileSummary {
    pub(crate) path: String,
    pub(crate) chars: usize,
    pub(crate) total_lines: usize,
    pub(crate) start_line: usize,
    pub(crate) limit: usize,
    pub(crate) truncated: bool,
    pub(crate) read_more: Option<ReadMoreHint>,
}

/// Full snapshot of loaded project instructions (with content). Stored on the
/// `SessionRecord` and returned in `start_session` output.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectInstructionsSnapshot {
    pub(crate) loaded: bool,
    pub(crate) files: Vec<ProjectInstructionFile>,
    pub(crate) candidate_paths: Vec<String>,
    pub(crate) total_chars: usize,
    pub(crate) max_total_chars: usize,
    pub(crate) truncated: bool,
    pub(crate) note: String,
}

/// Summary-only snapshot (no file content) used by `session_summary` so the
/// summary does not echo large instruction bodies back on every call.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectInstructionsSummarySnapshot {
    pub(crate) loaded: bool,
    pub(crate) files: Vec<ProjectInstructionFileSummary>,
    pub(crate) candidate_paths: Vec<String>,
    pub(crate) total_chars: usize,
    pub(crate) max_total_chars: usize,
    pub(crate) truncated: bool,
    pub(crate) note: String,
}

fn candidate_paths() -> Vec<String> {
    INSTRUCTION_CANDIDATE_PATHS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl ProjectInstructionsSnapshot {
    /// Empty snapshot (`loaded = false`) used when no candidate could be read.
    pub(crate) fn empty() -> Self {
        Self {
            loaded: false,
            files: Vec::new(),
            candidate_paths: candidate_paths(),
            total_chars: 0,
            max_total_chars: MAX_TOTAL_CHARS,
            truncated: false,
            note: PROJECT_INSTRUCTIONS_NOTE.to_string(),
        }
    }

    /// Build a snapshot from a single successfully-read instruction file,
    /// applying the per-file line cap and the total char cap. The first
    /// successful candidate wins, so `files` holds at most one entry.
    pub(crate) fn from_single_file(path: &str, content: String, total_lines: usize) -> Self {
        let file = build_instruction_file(path, content, total_lines);
        let total_chars = file.chars;
        let truncated = file.truncated;
        Self {
            loaded: true,
            files: vec![file],
            candidate_paths: candidate_paths(),
            total_chars,
            max_total_chars: MAX_TOTAL_CHARS,
            truncated,
            note: PROJECT_INSTRUCTIONS_NOTE.to_string(),
        }
    }

    /// Project to a content-less summary for `session_summary`.
    pub(crate) fn to_summary(&self) -> ProjectInstructionsSummarySnapshot {
        ProjectInstructionsSummarySnapshot {
            loaded: self.loaded,
            files: self
                .files
                .iter()
                .map(|f| ProjectInstructionFileSummary {
                    path: f.path.clone(),
                    chars: f.chars,
                    total_lines: f.total_lines,
                    start_line: f.start_line,
                    limit: f.limit,
                    truncated: f.truncated,
                    read_more: f.read_more.clone(),
                })
                .collect(),
            candidate_paths: self.candidate_paths.clone(),
            total_chars: self.total_chars,
            max_total_chars: self.max_total_chars,
            truncated: self.truncated,
            note: self.note.clone(),
        }
    }
}

/// Apply the per-file line cap (`MAX_LINES_PER_FILE`) and the total char cap
/// (`MAX_TOTAL_CHARS`) to a raw instruction file body.
///
/// `content` is the raw text returned by the reader. For agent projects the
/// reader requests `MAX_LINES_PER_FILE + 1` lines so a returned line count
/// strictly greater than `MAX_LINES_PER_FILE` reliably signals line
/// truncation regardless of response format. `total_lines` is the best-known
/// true total line count of the file (exact for local reads and for the
/// `webcodex.file_read_range.v1` JSON format; a lower bound for plain-text
/// agent fallback).
fn build_instruction_file(
    path: &str,
    content: String,
    total_lines: usize,
) -> ProjectInstructionFile {
    let all_lines: Vec<&str> = content.lines().collect();
    let returned_lines = all_lines.len();
    let line_truncated = returned_lines > MAX_LINES_PER_FILE;
    let line_cap = if line_truncated {
        MAX_LINES_PER_FILE
    } else {
        returned_lines
    };

    // Apply the total char cap on top of the line cap, keeping only full lines
    // that fit. `lines_kept` tracks the boundary so `read_more` can point the
    // caller at the next line to read.
    let mut kept = String::new();
    let mut lines_kept = 0usize;
    let mut char_count = 0usize;
    let mut char_truncated = false;
    for (idx, line) in all_lines.iter().take(line_cap).enumerate() {
        let line_chars = line.chars().count();
        let separator = if idx == 0 { 0 } else { 1 };
        if char_count + separator + line_chars > MAX_TOTAL_CHARS {
            char_truncated = true;
            break;
        }
        if idx > 0 {
            kept.push('\n');
        }
        kept.push_str(line);
        char_count += separator + line_chars;
        lines_kept += 1;
    }

    let truncated = line_truncated || char_truncated;
    let chars = kept.chars().count();
    let reported_total = if total_lines >= returned_lines {
        total_lines
    } else {
        returned_lines
    };
    let read_more = if truncated {
        Some(ReadMoreHint {
            path: path.to_string(),
            start_line: lines_kept.saturating_add(1),
            limit: MAX_LINES_PER_FILE,
        })
    } else {
        None
    };

    ProjectInstructionFile {
        path: path.to_string(),
        content: kept,
        chars,
        total_lines: reported_total,
        start_line: 1,
        limit: MAX_LINES_PER_FILE,
        truncated,
        read_more,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_is_not_loaded() {
        let snap = ProjectInstructionsSnapshot::empty();
        assert!(!snap.loaded);
        assert!(snap.files.is_empty());
        assert_eq!(snap.max_total_chars, MAX_TOTAL_CHARS);
        assert_eq!(
            snap.candidate_paths,
            INSTRUCTION_CANDIDATE_PATHS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert!(snap.note.contains("project-local guidance only"));
    }

    #[test]
    fn from_single_file_small_file_is_not_truncated() {
        let body = "line one\nline two\n".to_string();
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, 2);
        assert!(snap.loaded);
        assert_eq!(snap.files.len(), 1);
        let file = &snap.files[0];
        assert_eq!(file.path, "AGENTS.md");
        assert!(!file.truncated);
        assert!(file.read_more.is_none());
        assert_eq!(file.total_lines, 2);
        assert_eq!(file.start_line, 1);
        assert_eq!(file.limit, MAX_LINES_PER_FILE);
        assert!(file.content.contains("line one"));
        assert_eq!(file.chars, file.content.chars().count());
        assert_eq!(snap.total_chars, file.chars);
        assert!(!snap.truncated);
    }

    #[test]
    fn line_truncation_marks_truncated_and_read_more() {
        // 500 lines: reader returns the first MAX_LINES_PER_FILE + 1 lines.
        let body = (0..(MAX_LINES_PER_FILE + 1))
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, 500);
        let file = &snap.files[0];
        assert!(file.truncated);
        let read_more = file.read_more.as_ref().expect("read_more hint");
        assert_eq!(read_more.path, "AGENTS.md");
        assert_eq!(read_more.start_line, MAX_LINES_PER_FILE + 1);
        assert_eq!(read_more.limit, MAX_LINES_PER_FILE);
        // Kept content is exactly MAX_LINES_PER_FILE lines.
        assert_eq!(file.content.lines().count(), MAX_LINES_PER_FILE);
        assert_eq!(file.total_lines, 500);
        assert!(snap.truncated);
    }

    #[test]
    fn line_truncation_plain_text_lower_bound_total_lines() {
        // Simulates the plain-text agent fallback: the reader returns 401 lines
        // and total_lines is only the returned count (a lower bound).
        let body = (0..(MAX_LINES_PER_FILE + 1))
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let returned_lines = body.lines().count();
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, returned_lines);
        let file = &snap.files[0];
        assert!(file.truncated);
        assert_eq!(file.total_lines, returned_lines);
        assert!(file.read_more.is_some());
    }

    #[test]
    fn char_truncation_keeps_full_lines_and_points_read_more_at_next_line() {
        // A short file (few lines) but the first line alone exceeds the char cap.
        let big_line = "x".repeat(MAX_TOTAL_CHARS + 1);
        let body = format!("{big_line}\nsecond line\nthird line\n");
        let snap = ProjectInstructionsSnapshot::from_single_file("CLAUDE.md", body, 3);
        let file = &snap.files[0];
        assert!(file.truncated);
        // The first line alone already exceeds the cap, so zero full lines fit.
        assert_eq!(lines_kept_from_content(&file.content), 0);
        let read_more = file.read_more.as_ref().expect("read_more hint");
        assert_eq!(read_more.start_line, 1);
    }

    #[test]
    fn char_truncation_with_several_fit_lines() {
        // Many small lines whose combined size exceeds the char cap.
        let line = "x".repeat(MAX_TOTAL_CHARS / 10 + 1);
        let body = (0..50).map(|_| line.clone()).collect::<Vec<_>>().join("\n");
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, 50);
        let file = &snap.files[0];
        assert!(file.truncated);
        assert!(file.chars <= MAX_TOTAL_CHARS);
        let read_more = file.read_more.as_ref().expect("read_more hint");
        assert!(read_more.start_line > 1);
        assert!(read_more.start_line <= MAX_LINES_PER_FILE + 1);
    }

    #[test]
    fn exactly_at_line_cap_is_not_truncated() {
        let body = (0..MAX_LINES_PER_FILE)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snap =
            ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, MAX_LINES_PER_FILE);
        let file = &snap.files[0];
        assert!(!file.truncated);
        assert!(file.read_more.is_none());
        assert_eq!(file.total_lines, MAX_LINES_PER_FILE);
    }

    #[test]
    fn summary_projection_omits_content() {
        let body = "alpha\nbeta\n".to_string();
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, 2);
        let summary = snap.to_summary();
        assert!(summary.loaded);
        assert_eq!(summary.files.len(), 1);
        let serialized = serde_json::to_string(&summary.files[0]).unwrap();
        assert!(!serialized.contains("content"));
        assert!(serialized.contains("chars"));
        assert!(serialized.contains("total_lines"));
        assert_eq!(summary.total_chars, snap.total_chars);
        assert_eq!(summary.candidate_paths, snap.candidate_paths);
    }

    fn lines_kept_from_content(content: &str) -> usize {
        if content.is_empty() {
            0
        } else {
            content.lines().count()
        }
    }
}

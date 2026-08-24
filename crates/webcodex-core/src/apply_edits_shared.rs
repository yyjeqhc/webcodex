//! Types, limits, and the sensitive-path guard shared by the `apply_text_edits`
//! host write path (`tool_runtime::files`) and the agent-side wire boundary.
//! Both sides consume this module from `webcodex-core`.
//!
//! It must stay dependency-light: only `serde` and `std`, which both binaries
//! have. Do not add main-crate-only imports here.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Line-ending convention of one existing text file. Mixed LF/CRLF and bare
/// carriage returns are rejected because edit matching must not guess how to
/// rewrite an ambiguous file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyTextLineEnding {
    None,
    Lf,
    Crlf,
}

/// Detect one unambiguous existing-file line-ending convention.
pub fn detect_apply_text_line_ending(text: &str) -> Result<ApplyTextLineEnding, &'static str> {
    let bytes = text.as_bytes();
    let mut saw_lf = false;
    let mut saw_crlf = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                if bytes.get(index + 1) != Some(&b'\n') {
                    return Err("file contains unsupported bare CR line endings");
                }
                saw_crlf = true;
                index += 2;
            }
            b'\n' => {
                saw_lf = true;
                index += 1;
            }
            _ => index += 1,
        }
    }
    match (saw_lf, saw_crlf) {
        (true, true) => Err("file contains mixed LF and CRLF line endings"),
        (true, false) => Ok(ApplyTextLineEnding::Lf),
        (false, true) => Ok(ApplyTextLineEnding::Crlf),
        (false, false) => Ok(ApplyTextLineEnding::None),
    }
}

/// Canonicalize LF/CRLF edit text to LF for exact matching. Bare CR remains a
/// hard error; no other whitespace or textual normalization is performed.
pub fn canonicalize_apply_text_line_endings<'a>(
    text: &'a str,
    line_ending: ApplyTextLineEnding,
) -> Result<Cow<'a, str>, &'static str> {
    let bytes = text.as_bytes();
    if !bytes.contains(&b'\r') {
        return Ok(Cow::Borrowed(text));
    }
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\r' && bytes.get(index + 1) != Some(&b'\n') {
            return Err("edit text contains unsupported bare CR line endings");
        }
    }
    if line_ending == ApplyTextLineEnding::None {
        return Ok(Cow::Borrowed(text));
    }
    Ok(Cow::Owned(text.replace("\r\n", "\n")))
}

/// Restore canonical LF content to the original existing-file convention.
pub fn restore_apply_text_line_endings(text: String, line_ending: ApplyTextLineEnding) -> String {
    match line_ending {
        ApplyTextLineEnding::Crlf => text.replace('\n', "\r\n"),
        ApplyTextLineEnding::None | ApplyTextLineEnding::Lf => text,
    }
}

/// Kind of atomic text edit performed by `apply_text_edits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyTextEditKind {
    ReplaceExact,
    InsertAfter,
    InsertBefore,
    DeleteExact,
}

impl ApplyTextEditKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReplaceExact => "replace_exact",
            Self::InsertAfter => "insert_after",
            Self::InsertBefore => "insert_before",
            Self::DeleteExact => "delete_exact",
        }
    }
}

/// A single atomic text edit against one file. Only the fields relevant to the
/// `kind` are required; the runtime validates presence before dispatch.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApplyTextEditInput {
    pub kind: ApplyTextEditKind,
    #[serde(default)]
    pub old_text: Option<String>,
    #[serde(default)]
    pub new_text: Option<String>,
    #[serde(default)]
    pub anchor_text: Option<String>,
    #[serde(default)]
    pub occurrence: Option<usize>,
}

/// Maximum number of source-order exact-match candidates returned for one
/// recoverable edit conflict. The full match count remains available.
pub const MAX_APPLY_TEXT_CONFLICT_CANDIDATES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplyTextMatchCandidate {
    pub occurrence: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyTextMatchConflictKind {
    MatchNotFound,
    MultipleMatches,
    OccurrenceOutOfRange,
}

impl ApplyTextMatchConflictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MatchNotFound => "match_not_found",
            Self::MultipleMatches => "multiple_matches",
            Self::OccurrenceOutOfRange => "occurrence_out_of_range",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyTextMatchConflict {
    pub kind: ApplyTextMatchConflictKind,
    pub match_count: usize,
    pub requested_occurrence: Option<usize>,
    pub candidate_ranges: Vec<ApplyTextMatchCandidate>,
    pub candidates_truncated: bool,
}

/// Resolve an optional 1-based exact occurrence against canonicalized file
/// content. Matches are deterministic, non-overlapping, and in source order.
pub fn resolve_apply_text_match(
    original: &str,
    needle: &str,
    occurrence: Option<usize>,
) -> Result<(usize, usize), ApplyTextMatchConflict> {
    debug_assert!(!needle.is_empty());
    let needle_newlines = needle.bytes().filter(|byte| *byte == b'\n').count();
    let needle_ends_with_newline = needle.as_bytes().last() == Some(&b'\n');
    let mut candidate_ranges = Vec::with_capacity(MAX_APPLY_TEXT_CONFLICT_CANDIDATES);
    let mut match_count = 0usize;
    let mut selected = None;
    let mut line_cursor = 0usize;
    let mut current_line = 1usize;

    for (start, _) in original.match_indices(needle) {
        match_count = match_count.saturating_add(1);
        if candidate_ranges.len() < MAX_APPLY_TEXT_CONFLICT_CANDIDATES {
            current_line = current_line.saturating_add(
                original.as_bytes()[line_cursor..start]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count(),
            );
            line_cursor = start;
            let end_line = if needle_ends_with_newline {
                current_line.saturating_add(needle_newlines.saturating_sub(1))
            } else {
                current_line.saturating_add(needle_newlines)
            };
            candidate_ranges.push(ApplyTextMatchCandidate {
                occurrence: match_count,
                start_line: current_line,
                end_line: end_line.max(current_line),
            });
        }
        if occurrence == Some(match_count) {
            selected = Some((start, start + needle.len()));
        }
    }

    if match_count == 0 {
        return Err(ApplyTextMatchConflict {
            kind: ApplyTextMatchConflictKind::MatchNotFound,
            match_count,
            requested_occurrence: occurrence,
            candidate_ranges,
            candidates_truncated: false,
        });
    }
    if let Some(requested) = occurrence {
        if requested == 0 || requested > match_count {
            return Err(ApplyTextMatchConflict {
                kind: ApplyTextMatchConflictKind::OccurrenceOutOfRange,
                match_count,
                requested_occurrence: Some(requested),
                candidates_truncated: match_count > MAX_APPLY_TEXT_CONFLICT_CANDIDATES,
                candidate_ranges,
            });
        }
        return Ok(selected.expect("requested exact occurrence counted above"));
    }
    if match_count > 1 {
        return Err(ApplyTextMatchConflict {
            kind: ApplyTextMatchConflictKind::MultipleMatches,
            match_count,
            requested_occurrence: None,
            candidates_truncated: match_count > MAX_APPLY_TEXT_CONFLICT_CANDIDATES,
            candidate_ranges,
        });
    }
    let start = original
        .find(needle)
        .expect("unique exact match counted above");
    Ok((start, start + needle.len()))
}

/// Kind of project-file change performed by one transactional
/// `apply_text_edits` batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyFileChangeKind {
    Edit,
    Create,
    Delete,
    Rename,
}

impl ApplyFileChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Create => "create",
            Self::Delete => "delete",
            Self::Rename => "rename",
        }
    }
}

/// One file change in a transactional edit batch. Runtime validation enforces
/// the fields allowed and required for each `kind` before the owning agent is
/// contacted.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApplyFileChangeInput {
    pub kind: ApplyFileChangeKind,
    pub path: String,
    #[serde(default)]
    pub to_path: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub edits: Vec<ApplyTextEditInput>,
    #[serde(default)]
    pub expected_sha256: Option<String>,
}

/// Maximum number of edits accepted by a single `apply_text_edits` call.
pub const MAX_APPLY_TEXT_EDITS: usize = 20;

/// Maximum files changed by one transactional `apply_text_edits` request.
pub const MAX_APPLY_FILE_CHANGES: usize = 16;

/// Maximum byte size of a single `old_text`/`new_text`/`anchor_text` field in
/// an `apply_text_edits` edit.
pub const MAX_APPLY_TEXT_EDIT_FIELD_BYTES: usize = 512 * 1024; // 512 KiB

/// True if `path` contains a sensitive component for the structured edit tools.
/// Matching is component-wise (split on `/`) so legitimate filenames that
/// merely contain a sensitive substring (e.g. `targeting.md`) are NOT rejected.
/// A component is sensitive if it equals one of the guarded names, starts with
/// `.env` / `agent.toml` / `webcodex.env` (catching backups like `.env.local`
/// or `agent.toml.bak`), or ends with `.env` / `.toml.bak` (catching
/// `service.env` or `config.toml.bak`). This is the single source of truth for
/// both the host write path and the agent wire boundary.
pub fn is_sensitive_edit_path(path: &str) -> bool {
    // Edits are denied for credentials *and* for the bulk trees: writing into
    // `.git`, `target`, or `node_modules` through the tool surface is never
    // intended. Reads use the narrower `is_secret_path`.
    crate::sensitive_paths::is_bulk_skipped_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_text_edits_exact_occurrence_resolution_is_bounded_and_ordered() {
        let source = (1..=10)
            .map(|index| format!("prefix-{index}\nneedle\n"))
            .collect::<String>();
        let conflict = resolve_apply_text_match(&source, "needle\n", None).unwrap_err();
        assert_eq!(conflict.kind, ApplyTextMatchConflictKind::MultipleMatches);
        assert_eq!(conflict.match_count, 10);
        assert_eq!(
            conflict.candidate_ranges.len(),
            MAX_APPLY_TEXT_CONFLICT_CANDIDATES
        );
        assert!(conflict.candidates_truncated);
        assert_eq!(conflict.candidate_ranges[0].occurrence, 1);
        assert_eq!(conflict.candidate_ranges[0].start_line, 2);
        assert_eq!(conflict.candidate_ranges[7].occurrence, 8);
        let second = resolve_apply_text_match(&source, "needle\n", Some(2)).unwrap();
        assert_eq!(&source[second.0..second.1], "needle\n");
        assert!(second.0 > source.find("needle\n").unwrap());
    }

    #[test]
    fn apply_text_edits_multiline_candidate_ranges_are_inclusive_source_lines() {
        let source = "head\na\nb\nmid\na\nb\n";
        let conflict = resolve_apply_text_match(source, "a\nb\n", None).unwrap_err();
        assert_eq!(conflict.candidate_ranges[0].start_line, 2);
        assert_eq!(conflict.candidate_ranges[0].end_line, 3);
        assert_eq!(conflict.candidate_ranges[1].start_line, 5);
        assert_eq!(conflict.candidate_ranges[1].end_line, 6);
    }
}

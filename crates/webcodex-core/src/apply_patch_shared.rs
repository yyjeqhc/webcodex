//! Side-effect-free parsing and text-update semantics for the model-facing
//! Codex-compatible `apply_patch` tool. Filesystem authority stays with the
//! Runner; this module only validates the patch language and derives new UTF-8
//! content from already-read source text.

use crate::apply_edits_shared::{
    canonicalize_apply_text_line_endings, detect_apply_text_line_ending,
    restore_apply_text_line_endings,
};
use std::fmt;

pub const MAX_CODEX_PATCH_BYTES: usize = 256 * 1024;
pub const MAX_CODEX_PATCH_FILE_CHANGES: usize = 64;
pub const MAX_CODEX_PATCH_CHUNKS_PER_FILE: usize = 256;

const BEGIN_PATCH_MARKER: &str = "*** Begin Patch";
const END_PATCH_MARKER: &str = "*** End Patch";
const ADD_FILE_MARKER: &str = "*** Add File:";
const DELETE_FILE_MARKER: &str = "*** Delete File:";
const UPDATE_FILE_MARKER: &str = "*** Update File:";
const MOVE_TO_MARKER: &str = "*** Move to:";
const END_OF_FILE_MARKER: &str = "*** End of File";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPatch {
    pub hunks: Vec<CodexPatchHunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexPatchHunk {
    AddFile {
        path: String,
        contents: String,
    },
    DeleteFile {
        path: String,
    },
    UpdateFile {
        path: String,
        move_path: Option<String>,
        chunks: Vec<CodexPatchChunk>,
    },
}

impl CodexPatchHunk {
    pub fn path(&self) -> &str {
        match self {
            Self::AddFile { path, .. }
            | Self::DeleteFile { path }
            | Self::UpdateFile { path, .. } => path,
        }
    }

    pub fn move_path(&self) -> Option<&str> {
        match self {
            Self::UpdateFile { move_path, .. } => move_path.as_deref(),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::AddFile { .. } => "add",
            Self::DeleteFile { .. } => "delete",
            Self::UpdateFile { .. } => "update",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexPatchChunk {
    pub change_context: Option<String>,
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
    pub is_end_of_file: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CodexPatchMatchMode {
    Exact,
    TrimEnd,
    Trim,
}

impl CodexPatchMatchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::TrimEnd => "trim_end",
            Self::Trim => "trim",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexPatchMatchSource {
    OldLines,
    ChangeContext,
    Append,
}

impl CodexPatchMatchSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OldLines => "old_lines",
            Self::ChangeContext => "change_context",
            Self::Append => "append",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPatchChunkMatch {
    pub chunk_index: usize,
    pub match_mode: Option<CodexPatchMatchMode>,
    pub match_source: CodexPatchMatchSource,
    /// One-based source line where the replacement or insertion starts.
    pub matched_start_line: usize,
    /// Number of candidates at the selected match mode for match_source.
    /// Append operations do not perform text matching and report None.
    pub candidate_count: Option<usize>,
    /// True only when every text match used to position this chunk was exact
    /// and unique. Unanchored append performs no text match and is strict-safe.
    pub strict_match: bool,
}

/// Body-free structural hint for a failed apply_patch text search.
///
/// This intentionally contains no source or patch line text. It is safe to
/// surface to the model so a context mismatch can be refined without echoing
/// file contents or relying on fuzzy auto-write behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPatchMatchDiagnostic {
    pub chunk_index: usize,
    pub match_source: CodexPatchMatchSource,
    /// One-based line where this chunk's search began after prior chunks.
    pub search_start_line: usize,
    pub expected_line_count: usize,
    /// Number of source lines available from search_start_line to EOF.
    pub available_line_count: usize,
    /// One-based start of the structurally closest candidate, when any source
    /// line remains in the search range.
    pub closest_start_line: Option<usize>,
    /// Positional line matches within the closest candidate at each tier.
    pub closest_exact_line_matches: usize,
    pub closest_trim_end_line_matches: usize,
    pub closest_trim_line_matches: usize,
    /// One-based offset inside the expected pattern for the first exact
    /// mismatch (or first unavailable source line).
    pub first_exact_mismatch_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPatchUpdate {
    pub content: String,
    pub chunk_matches: Vec<CodexPatchChunkMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPatchError {
    pub kind: &'static str,
    pub line: Option<usize>,
    pub message: String,
    pub match_diagnostic: Option<CodexPatchMatchDiagnostic>,
}

impl CodexPatchError {
    fn new(kind: &'static str, line: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            kind,
            line,
            message: message.into(),
            match_diagnostic: None,
        }
    }

    fn with_match_diagnostic(mut self, diagnostic: CodexPatchMatchDiagnostic) -> Self {
        self.match_diagnostic = Some(diagnostic);
        self
    }
}

impl fmt::Display for CodexPatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.line {
            write!(f, "{} at patch line {line}: {}", self.kind, self.message)
        } else {
            write!(f, "{}: {}", self.kind, self.message)
        }
    }
}

impl std::error::Error for CodexPatchError {}

fn marker_path(line: &str, marker: &str) -> Option<String> {
    let trimmed = line.trim_end();
    trimmed
        .strip_prefix(marker)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
}

fn is_file_header(line: &str) -> bool {
    marker_path(line, ADD_FILE_MARKER).is_some()
        || marker_path(line, DELETE_FILE_MARKER).is_some()
        || marker_path(line, UPDATE_FILE_MARKER).is_some()
}

fn normalized_patch_lines(patch: &str) -> Result<Vec<String>, CodexPatchError> {
    if patch.is_empty() {
        return Err(CodexPatchError::new(
            "invalid_patch",
            None,
            "patch cannot be empty",
        ));
    }
    if patch.len() > MAX_CODEX_PATCH_BYTES {
        return Err(CodexPatchError::new(
            "patch_too_large",
            None,
            format!("patch exceeds {MAX_CODEX_PATCH_BYTES} UTF-8 bytes"),
        ));
    }
    let normalized = patch.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err(CodexPatchError::new(
            "invalid_patch",
            None,
            "patch contains unsupported bare CR line endings",
        ));
    }
    Ok(normalized.split('\n').map(str::to_string).collect())
}

fn push_chunk(
    chunks: &mut Vec<CodexPatchChunk>,
    chunk: CodexPatchChunk,
    line: usize,
) -> Result<(), CodexPatchError> {
    if chunk.change_context.is_some() && chunk.old_lines.is_empty() && chunk.new_lines.is_empty() {
        return Err(CodexPatchError::new(
            "invalid_hunk",
            Some(line),
            "change context must be followed by at least one patch line",
        ));
    }
    if chunk.change_context.is_none()
        && chunk.old_lines.is_empty()
        && chunk.new_lines.is_empty()
        && !chunk.is_end_of_file
    {
        return Ok(());
    }
    if chunks.len() >= MAX_CODEX_PATCH_CHUNKS_PER_FILE {
        return Err(CodexPatchError::new(
            "patch_too_large",
            Some(line),
            format!("one update may contain at most {MAX_CODEX_PATCH_CHUNKS_PER_FILE} chunks"),
        ));
    }
    chunks.push(chunk);
    Ok(())
}

pub fn parse_codex_patch(patch: &str) -> Result<CodexPatch, CodexPatchError> {
    let lines = normalized_patch_lines(patch)?;
    let mut index = 0usize;
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }
    if lines.get(index).map(|line| line.trim()) != Some(BEGIN_PATCH_MARKER) {
        return Err(CodexPatchError::new(
            "invalid_patch",
            Some(index.saturating_add(1)),
            format!("first marker must be '{BEGIN_PATCH_MARKER}'"),
        ));
    }
    index += 1;

    let mut hunks = Vec::new();
    let mut saw_end = false;
    while index < lines.len() {
        while index < lines.len() && lines[index].trim().is_empty() {
            index += 1;
        }
        let Some(line) = lines.get(index) else {
            break;
        };
        if line.trim_end() == END_PATCH_MARKER {
            saw_end = true;
            index += 1;
            break;
        }
        if hunks.len() >= MAX_CODEX_PATCH_FILE_CHANGES {
            return Err(CodexPatchError::new(
                "patch_too_large",
                Some(index + 1),
                format!("patch may touch at most {MAX_CODEX_PATCH_FILE_CHANGES} files"),
            ));
        }

        if let Some(path) = marker_path(line, ADD_FILE_MARKER) {
            let header_line = index + 1;
            index += 1;
            let mut added = Vec::new();
            while index < lines.len() {
                let current = &lines[index];
                if current.trim_end() == END_PATCH_MARKER || is_file_header(current) {
                    break;
                }
                if current.trim().is_empty()
                    && lines.get(index + 1).is_some_and(|next| {
                        next.trim_end() == END_PATCH_MARKER || is_file_header(next)
                    })
                {
                    index += 1;
                    break;
                }
                let Some(content) = current.strip_prefix('+') else {
                    return Err(CodexPatchError::new(
                        "invalid_hunk",
                        Some(index + 1),
                        "every Add File content line must start with '+'",
                    ));
                };
                added.push(content.to_string());
                index += 1;
            }
            if added.is_empty() {
                return Err(CodexPatchError::new(
                    "invalid_hunk",
                    Some(header_line),
                    "Add File requires at least one '+' content line",
                ));
            }
            let mut contents = added.join("\n");
            contents.push('\n');
            hunks.push(CodexPatchHunk::AddFile { path, contents });
            continue;
        }

        if let Some(path) = marker_path(line, DELETE_FILE_MARKER) {
            hunks.push(CodexPatchHunk::DeleteFile { path });
            index += 1;
            continue;
        }

        if let Some(path) = marker_path(line, UPDATE_FILE_MARKER) {
            let header_line = index + 1;
            index += 1;
            let mut move_path = None;
            if let Some(current) = lines.get(index) {
                if let Some(destination) = marker_path(current, MOVE_TO_MARKER) {
                    move_path = Some(destination);
                    index += 1;
                }
            }

            let mut chunks = Vec::new();
            let mut current_chunk: Option<CodexPatchChunk> = None;
            let mut saw_eof = false;
            while index < lines.len() {
                let current = &lines[index];
                if current.trim_end() == END_PATCH_MARKER || is_file_header(current) {
                    break;
                }
                if current.trim().is_empty()
                    && lines.get(index + 1).is_some_and(|next| {
                        next.trim_end() == END_PATCH_MARKER || is_file_header(next)
                    })
                {
                    index += 1;
                    break;
                }
                if saw_eof {
                    return Err(CodexPatchError::new(
                        "invalid_hunk",
                        Some(index + 1),
                        "no update lines may follow '*** End of File'",
                    ));
                }
                let trimmed = current.trim_end();
                if trimmed == "@@" || trimmed.starts_with("@@ ") {
                    if let Some(chunk) = current_chunk.take() {
                        push_chunk(&mut chunks, chunk, index + 1)?;
                    }
                    let change_context = trimmed
                        .strip_prefix("@@ ")
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string);
                    current_chunk = Some(CodexPatchChunk {
                        change_context,
                        ..Default::default()
                    });
                    index += 1;
                    continue;
                }
                if trimmed == END_OF_FILE_MARKER {
                    let Some(chunk) = current_chunk.as_mut() else {
                        return Err(CodexPatchError::new(
                            "invalid_hunk",
                            Some(index + 1),
                            "'*** End of File' requires a preceding update chunk",
                        ));
                    };
                    chunk.is_end_of_file = true;
                    saw_eof = true;
                    index += 1;
                    continue;
                }
                let chunk = current_chunk.get_or_insert_with(CodexPatchChunk::default);
                let mut chars = current.chars();
                let Some(prefix) = chars.next() else {
                    return Err(CodexPatchError::new(
                        "invalid_hunk",
                        Some(index + 1),
                        "update lines must start with '+', '-', or one context space",
                    ));
                };
                let content = chars.as_str().to_string();
                match prefix {
                    '+' => chunk.new_lines.push(content),
                    '-' => chunk.old_lines.push(content),
                    ' ' => {
                        chunk.old_lines.push(content.clone());
                        chunk.new_lines.push(content);
                    }
                    _ => {
                        return Err(CodexPatchError::new(
                            "invalid_hunk",
                            Some(index + 1),
                            "update lines must start with '+', '-', or one context space",
                        ))
                    }
                }
                index += 1;
            }
            if let Some(chunk) = current_chunk.take() {
                push_chunk(&mut chunks, chunk, index.saturating_add(1))?;
            }
            if move_path.is_none() && chunks.is_empty() {
                return Err(CodexPatchError::new(
                    "invalid_hunk",
                    Some(header_line),
                    "Update File requires a Move to destination or at least one change chunk",
                ));
            }
            hunks.push(CodexPatchHunk::UpdateFile {
                path,
                move_path,
                chunks,
            });
            continue;
        }

        return Err(CodexPatchError::new(
            "invalid_patch",
            Some(index + 1),
            "expected Add File, Delete File, Update File, or End Patch marker",
        ));
    }

    if !saw_end {
        return Err(CodexPatchError::new(
            "invalid_patch",
            None,
            format!("patch must end with '{END_PATCH_MARKER}'"),
        ));
    }
    if hunks.is_empty() {
        return Err(CodexPatchError::new(
            "invalid_patch",
            None,
            "patch must contain at least one file operation",
        ));
    }
    if lines[index..].iter().any(|line| !line.trim().is_empty()) {
        return Err(CodexPatchError::new(
            "invalid_patch",
            Some(index + 1),
            "non-whitespace content follows End Patch",
        ));
    }
    Ok(CodexPatch { hunks })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SequenceMatch {
    index: usize,
    mode: CodexPatchMatchMode,
    candidate_count: usize,
}

impl SequenceMatch {
    fn is_exact_unique(self) -> bool {
        self.mode == CodexPatchMatchMode::Exact && self.candidate_count == 1
    }
}

fn seek_sequence(
    lines: &[String],
    pattern: &[String],
    start: usize,
    eof: bool,
) -> Option<SequenceMatch> {
    if pattern.is_empty() {
        return Some(SequenceMatch {
            index: start.min(lines.len()),
            mode: CodexPatchMatchMode::Exact,
            candidate_count: 1,
        });
    }
    if pattern.len() > lines.len() {
        return None;
    }
    let last_start = lines.len() - pattern.len();
    let start = start.min(last_start);

    let matches_at = |index: usize, mode: CodexPatchMatchMode| {
        lines[index..index + pattern.len()]
            .iter()
            .zip(pattern)
            .all(|(candidate, expected)| match mode {
                CodexPatchMatchMode::Exact => candidate == expected,
                CodexPatchMatchMode::TrimEnd => candidate.trim_end() == expected.trim_end(),
                CodexPatchMatchMode::Trim => candidate.trim() == expected.trim(),
            })
    };

    for mode in [
        CodexPatchMatchMode::Exact,
        CodexPatchMatchMode::TrimEnd,
        CodexPatchMatchMode::Trim,
    ] {
        let mut selected_index = (eof && matches_at(last_start, mode)).then_some(last_start);
        let mut candidate_count = 0usize;
        for index in start..=last_start {
            if matches_at(index, mode) {
                candidate_count = candidate_count.saturating_add(1);
                if selected_index.is_none() {
                    selected_index = Some(index);
                }
            }
        }
        if let Some(index) = selected_index {
            return Some(SequenceMatch {
                index,
                mode,
                candidate_count,
            });
        }
    }
    None
}

fn diagnose_sequence_miss(
    lines: &[String],
    pattern: &[String],
    start: usize,
    chunk_index: usize,
    match_source: CodexPatchMatchSource,
) -> CodexPatchMatchDiagnostic {
    let effective_start = start.min(lines.len());
    let available_line_count = lines.len().saturating_sub(effective_start);
    let mut best: Option<(usize, usize, usize, usize)> = None;
    // Prefer candidates that have enough remaining source lines to represent the
    // complete expected pattern. Only fall back to EOF-partial candidates when
    // the search range itself is shorter than the pattern. Within that candidate
    // set, broad whitespace-normalized positional coverage is more useful for
    // recovery than one isolated exact line.
    let has_complete_candidate = available_line_count >= pattern.len();

    for index in effective_start..lines.len() {
        let compared = pattern.len().min(lines.len() - index);
        if has_complete_candidate && compared < pattern.len() {
            break;
        }
        let mut exact = 0usize;
        let mut trim_end = 0usize;
        let mut trim = 0usize;
        for offset in 0..compared {
            let candidate = &lines[index + offset];
            let expected = &pattern[offset];
            exact += usize::from(candidate == expected);
            trim_end += usize::from(candidate.trim_end() == expected.trim_end());
            trim += usize::from(candidate.trim() == expected.trim());
        }
        let score = (trim, trim_end, exact);
        let replace = best
            .map(|(_, best_exact, best_trim_end, best_trim)| {
                score > (best_trim, best_trim_end, best_exact)
            })
            .unwrap_or(true);
        if replace {
            best = Some((index, exact, trim_end, trim));
        }
    }

    let (closest_start_line, closest_exact, closest_trim_end, closest_trim, first_mismatch) =
        if let Some((index, exact, trim_end, trim)) = best {
            let first_mismatch = (0..pattern.len())
                .find(|offset| {
                    lines
                        .get(index + *offset)
                        .is_none_or(|candidate| candidate != &pattern[*offset])
                })
                .map(|offset| offset + 1);
            (Some(index + 1), exact, trim_end, trim, first_mismatch)
        } else {
            (None, 0, 0, 0, (!pattern.is_empty()).then_some(1))
        };

    CodexPatchMatchDiagnostic {
        chunk_index,
        match_source,
        search_start_line: effective_start + 1,
        expected_line_count: pattern.len(),
        available_line_count,
        closest_start_line,
        closest_exact_line_matches: closest_exact,
        closest_trim_end_line_matches: closest_trim_end,
        closest_trim_line_matches: closest_trim,
        first_exact_mismatch_offset: first_mismatch,
    }
}

pub fn derive_codex_patch_update(
    original: &str,
    path: &str,
    chunks: &[CodexPatchChunk],
) -> Result<String, CodexPatchError> {
    derive_codex_patch_update_with_matches(original, path, chunks).map(|update| update.content)
}

pub fn derive_codex_patch_update_with_matches(
    original: &str,
    path: &str,
    chunks: &[CodexPatchChunk],
) -> Result<CodexPatchUpdate, CodexPatchError> {
    let line_ending = detect_apply_text_line_ending(original).map_err(|message| {
        CodexPatchError::new("unsupported_file", None, format!("{path}: {message}"))
    })?;
    let canonical =
        canonicalize_apply_text_line_endings(original, line_ending).map_err(|message| {
            CodexPatchError::new("unsupported_file", None, format!("{path}: {message}"))
        })?;
    let mut original_lines = canonical
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if original_lines.last().is_some_and(String::is_empty) {
        original_lines.pop();
    }

    let mut replacements: Vec<(usize, usize, Vec<String>)> = Vec::new();
    let mut chunk_matches = Vec::with_capacity(chunks.len());
    let mut line_index = 0usize;
    for (chunk_index, chunk) in chunks.iter().enumerate() {
        let mut context_match = None;
        if let Some(context) = chunk.change_context.as_ref() {
            let Some(matched_context) = seek_sequence(
                &original_lines,
                std::slice::from_ref(context),
                line_index,
                false,
            ) else {
                return Err(CodexPatchError::new(
                    "context_mismatch",
                    None,
                    format!("{path}: chunk {chunk_index} could not find its change context"),
                )
                .with_match_diagnostic(diagnose_sequence_miss(
                    &original_lines,
                    std::slice::from_ref(context),
                    line_index,
                    chunk_index,
                    CodexPatchMatchSource::ChangeContext,
                )));
            };
            context_match = Some(matched_context);
            line_index = matched_context.index + 1;
        }

        if chunk.old_lines.is_empty() {
            let insertion_index = if chunk.change_context.is_some() {
                line_index
            } else {
                original_lines.len()
            };
            replacements.push((insertion_index, 0, chunk.new_lines.clone()));
            chunk_matches.push(CodexPatchChunkMatch {
                chunk_index,
                match_mode: context_match.map(|matched| matched.mode),
                match_source: if chunk.change_context.is_some() {
                    CodexPatchMatchSource::ChangeContext
                } else {
                    CodexPatchMatchSource::Append
                },
                matched_start_line: insertion_index + 1,
                candidate_count: context_match.map(|matched| matched.candidate_count),
                strict_match: context_match.is_none_or(SequenceMatch::is_exact_unique),
            });
            continue;
        }

        let mut pattern = chunk.old_lines.as_slice();
        let mut replacement = chunk.new_lines.as_slice();
        let mut found = seek_sequence(&original_lines, pattern, line_index, chunk.is_end_of_file);
        if found.is_none() && pattern.last().is_some_and(String::is_empty) {
            pattern = &pattern[..pattern.len() - 1];
            if replacement.last().is_some_and(String::is_empty) {
                replacement = &replacement[..replacement.len() - 1];
            }
            found = seek_sequence(&original_lines, pattern, line_index, chunk.is_end_of_file);
        }
        let Some(found) = found else {
            return Err(CodexPatchError::new(
                "context_mismatch",
                None,
                format!("{path}: chunk {chunk_index} could not find its expected old lines"),
            )
            .with_match_diagnostic(diagnose_sequence_miss(
                &original_lines,
                pattern,
                line_index,
                chunk_index,
                CodexPatchMatchSource::OldLines,
            )));
        };
        let start = found.index;
        replacements.push((start, pattern.len(), replacement.to_vec()));
        chunk_matches.push(CodexPatchChunkMatch {
            chunk_index,
            match_mode: Some(
                context_match
                    .map(|matched_context| matched_context.mode.max(found.mode))
                    .unwrap_or(found.mode),
            ),
            match_source: CodexPatchMatchSource::OldLines,
            matched_start_line: start + 1,
            candidate_count: Some(found.candidate_count),
            strict_match: found.is_exact_unique()
                && context_match.is_none_or(SequenceMatch::is_exact_unique),
        });
        line_index = start + pattern.len();
    }

    replacements.sort_by_key(|(start, _, _)| *start);
    let mut new_lines = original_lines;
    for (start, old_len, replacement) in replacements.into_iter().rev() {
        new_lines.splice(start..start + old_len, replacement);
    }
    new_lines.push(String::new());
    Ok(CodexPatchUpdate {
        content: restore_apply_text_line_endings(new_lines.join("\n"), line_ending),
        chunk_matches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_add_delete_update_move_and_context() {
        let patch = parse_codex_patch(
            "*** Begin Patch\n*** Add File: add.txt\n+hello\n*** Delete File: old.txt\n*** Update File: src.txt\n*** Move to: dst.txt\n@@ fn demo()\n-old\n+new\n*** End Patch",
        )
        .unwrap();
        assert_eq!(patch.hunks.len(), 3);
        assert!(matches!(
            &patch.hunks[0],
            CodexPatchHunk::AddFile { path, contents }
                if path == "add.txt" && contents == "hello\n"
        ));
        assert!(matches!(
            &patch.hunks[2],
            CodexPatchHunk::UpdateFile { path, move_path: Some(to), chunks }
                if path == "src.txt" && to == "dst.txt" && chunks.len() == 1
        ));
    }

    #[test]
    fn update_allows_first_chunk_without_explicit_context_marker() {
        let patch = parse_codex_patch(
            "*** Begin Patch\n*** Update File: file.txt\n old\n+new\n*** End Patch",
        )
        .unwrap();
        let CodexPatchHunk::UpdateFile { chunks, .. } = &patch.hunks[0] else {
            panic!("expected update")
        };
        assert_eq!(chunks[0].old_lines, vec!["old"]);
        assert_eq!(chunks[0].new_lines, vec!["old", "new"]);
    }

    #[test]
    fn update_matching_falls_back_to_trimmed_whitespace_and_preserves_crlf() {
        let chunks = vec![CodexPatchChunk {
            old_lines: vec!["alpha".into(), "beta".into()],
            new_lines: vec!["alpha".into(), "changed".into()],
            ..Default::default()
        }];
        let updated =
            derive_codex_patch_update_with_matches("  alpha  \r\n beta\t\r\n", "file.txt", &chunks)
                .unwrap();
        assert_eq!(updated.content, "alpha\r\nchanged\r\n");
        assert_eq!(
            updated.chunk_matches[0].match_mode,
            Some(CodexPatchMatchMode::Trim)
        );
        assert_eq!(
            updated.chunk_matches[0].match_source,
            CodexPatchMatchSource::OldLines
        );
        assert_eq!(updated.chunk_matches[0].matched_start_line, 1);
        assert_eq!(updated.chunk_matches[0].candidate_count, Some(1));
        assert!(!updated.chunk_matches[0].strict_match);
    }

    #[test]
    fn update_matching_reports_exact_and_trim_end_modes() {
        let exact = vec![CodexPatchChunk {
            old_lines: vec!["alpha".into()],
            new_lines: vec!["ALPHA".into()],
            ..Default::default()
        }];
        let exact_update =
            derive_codex_patch_update_with_matches("alpha\n", "file.txt", &exact).unwrap();
        assert_eq!(
            exact_update.chunk_matches[0].match_mode,
            Some(CodexPatchMatchMode::Exact)
        );

        let trim_end = vec![CodexPatchChunk {
            old_lines: vec!["beta".into()],
            new_lines: vec!["BETA".into()],
            ..Default::default()
        }];
        let trim_end_update =
            derive_codex_patch_update_with_matches("beta  \n", "file.txt", &trim_end).unwrap();
        assert_eq!(
            trim_end_update.chunk_matches[0].match_mode,
            Some(CodexPatchMatchMode::TrimEnd)
        );
    }

    #[test]
    fn update_matching_reports_widest_mode_used_by_change_context_and_old_lines() {
        let chunks = vec![CodexPatchChunk {
            change_context: Some("section".into()),
            old_lines: vec!["old".into()],
            new_lines: vec!["new".into()],
            ..Default::default()
        }];
        let update =
            derive_codex_patch_update_with_matches("  section  \nold\n", "file.txt", &chunks)
                .unwrap();
        assert_eq!(update.content, "  section  \nnew\n");
        assert_eq!(
            update.chunk_matches[0].match_mode,
            Some(CodexPatchMatchMode::Trim)
        );
        assert_eq!(
            update.chunk_matches[0].match_source,
            CodexPatchMatchSource::OldLines
        );
        assert_eq!(update.chunk_matches[0].matched_start_line, 2);
        assert!(!update.chunk_matches[0].strict_match);
    }

    #[test]
    fn anchored_pure_addition_inserts_after_change_context() {
        let patch = parse_codex_patch(
            "*** Begin Patch\n*** Update File: file.py\n@@ class Alpha:\n+    inserted = True\n*** End Patch",
        )
        .unwrap();
        let CodexPatchHunk::UpdateFile { chunks, .. } = &patch.hunks[0] else {
            panic!("expected update")
        };
        let updated = derive_codex_patch_update_with_matches(
            "class Alpha:\n    existing = True\n\nclass Beta:\n    existing = True\n",
            "file.py",
            chunks,
        )
        .unwrap();
        assert_eq!(
            updated.content,
            "class Alpha:\n    inserted = True\n    existing = True\n\nclass Beta:\n    existing = True\n"
        );
        assert_eq!(
            updated.chunk_matches[0].match_source,
            CodexPatchMatchSource::ChangeContext
        );
        assert_eq!(
            updated.chunk_matches[0].match_mode,
            Some(CodexPatchMatchMode::Exact)
        );
        assert_eq!(updated.chunk_matches[0].matched_start_line, 2);
        assert_eq!(updated.chunk_matches[0].candidate_count, Some(1));
        assert!(updated.chunk_matches[0].strict_match);
    }

    #[test]
    fn anchorless_pure_addition_appends_at_end_of_file() {
        let patch =
            parse_codex_patch("*** Begin Patch\n*** Update File: file.txt\n+tail\n*** End Patch")
                .unwrap();
        let CodexPatchHunk::UpdateFile { chunks, .. } = &patch.hunks[0] else {
            panic!("expected update")
        };
        let updated =
            derive_codex_patch_update_with_matches("alpha\nbeta\n", "file.txt", chunks).unwrap();
        assert_eq!(updated.content, "alpha\nbeta\ntail\n");
        assert_eq!(
            updated.chunk_matches[0].match_source,
            CodexPatchMatchSource::Append
        );
        assert_eq!(updated.chunk_matches[0].match_mode, None);
        assert_eq!(updated.chunk_matches[0].matched_start_line, 3);
        assert_eq!(updated.chunk_matches[0].candidate_count, None);
        assert!(updated.chunk_matches[0].strict_match);
    }

    #[test]
    fn end_of_file_prefers_the_trailing_match() {
        let chunks = vec![CodexPatchChunk {
            old_lines: vec!["same".into()],
            new_lines: vec!["last".into()],
            is_end_of_file: true,
            ..Default::default()
        }];
        let updated =
            derive_codex_patch_update_with_matches("same\nmid\nsame\n", "file.txt", &chunks)
                .unwrap();
        assert_eq!(updated.content, "same\nmid\nlast\n");
        assert_eq!(updated.chunk_matches[0].matched_start_line, 3);
        assert_eq!(updated.chunk_matches[0].candidate_count, Some(2));
        assert!(!updated.chunk_matches[0].strict_match);
    }

    #[test]
    fn repeated_match_reports_candidate_count_without_changing_first_match_selection() {
        let chunks = vec![CodexPatchChunk {
            old_lines: vec!["same".into()],
            new_lines: vec!["first".into()],
            ..Default::default()
        }];
        let updated =
            derive_codex_patch_update_with_matches("same\nmid\nsame\n", "file.txt", &chunks)
                .unwrap();
        assert_eq!(updated.content, "first\nmid\nsame\n");
        assert_eq!(updated.chunk_matches[0].matched_start_line, 1);
        assert_eq!(updated.chunk_matches[0].candidate_count, Some(2));
        assert!(!updated.chunk_matches[0].strict_match);
    }

    #[test]
    fn exact_tier_still_beats_earlier_fuzzy_candidate() {
        let chunks = vec![CodexPatchChunk {
            old_lines: vec!["target".into()],
            new_lines: vec!["changed".into()],
            ..Default::default()
        }];
        let updated =
            derive_codex_patch_update_with_matches(" target \ntarget\n", "file.txt", &chunks)
                .unwrap();
        assert_eq!(updated.content, " target \nchanged\n");
        assert_eq!(
            updated.chunk_matches[0].match_mode,
            Some(CodexPatchMatchMode::Exact)
        );
        assert_eq!(updated.chunk_matches[0].matched_start_line, 2);
        assert_eq!(updated.chunk_matches[0].candidate_count, Some(1));
        assert!(updated.chunk_matches[0].strict_match);
    }

    #[test]
    fn context_mismatch_reports_body_free_nearest_match_diagnostic() {
        let chunks = vec![CodexPatchChunk {
            old_lines: vec!["alpha".into(), "expected-secret".into(), "omega".into()],
            new_lines: vec!["changed".into()],
            ..Default::default()
        }];
        let error = derive_codex_patch_update_with_matches(
            "alpha\nactual-secret\nomega\n",
            "file.txt",
            &chunks,
        )
        .unwrap_err();

        assert_eq!(error.kind, "context_mismatch");
        assert!(!error.message.contains("expected-secret"));
        assert!(!error.message.contains("actual-secret"));
        let diagnostic = error.match_diagnostic.expect("match diagnostic");
        assert_eq!(diagnostic.chunk_index, 0);
        assert_eq!(diagnostic.match_source, CodexPatchMatchSource::OldLines);
        assert_eq!(diagnostic.search_start_line, 1);
        assert_eq!(diagnostic.expected_line_count, 3);
        assert_eq!(diagnostic.available_line_count, 3);
        assert_eq!(diagnostic.closest_start_line, Some(1));
        assert_eq!(diagnostic.closest_exact_line_matches, 2);
        assert_eq!(diagnostic.closest_trim_end_line_matches, 2);
        assert_eq!(diagnostic.closest_trim_line_matches, 2);
        assert_eq!(diagnostic.first_exact_mismatch_offset, Some(2));
    }

    #[test]
    fn context_mismatch_prefers_full_high_coverage_candidate_over_partial_eof_exact_line() {
        let chunks = vec![CodexPatchChunk {
            old_lines: vec!["alpha".into(), "beta".into(), "gamma".into()],
            new_lines: vec!["changed".into()],
            ..Default::default()
        }];
        let error = derive_codex_patch_update_with_matches(
            " alpha \n beta \nchanged\nnoise\nalpha\n",
            "file.txt",
            &chunks,
        )
        .unwrap_err();

        let diagnostic = error.match_diagnostic.expect("match diagnostic");
        assert_eq!(diagnostic.closest_start_line, Some(1));
        assert_eq!(diagnostic.closest_exact_line_matches, 0);
        assert_eq!(diagnostic.closest_trim_line_matches, 2);
        assert_eq!(diagnostic.first_exact_mismatch_offset, Some(1));
    }

    #[test]
    fn change_context_mismatch_does_not_echo_context_text() {
        let chunks = vec![CodexPatchChunk {
            change_context: Some("private-context-token".into()),
            old_lines: vec!["old".into()],
            new_lines: vec!["new".into()],
            ..Default::default()
        }];
        let error = derive_codex_patch_update_with_matches("unrelated\nold\n", "file.txt", &chunks)
            .unwrap_err();
        assert_eq!(error.kind, "context_mismatch");
        assert!(!error.message.contains("private-context-token"));
        let diagnostic = error.match_diagnostic.expect("match diagnostic");
        assert_eq!(
            diagnostic.match_source,
            CodexPatchMatchSource::ChangeContext
        );
        assert_eq!(diagnostic.expected_line_count, 1);
    }

    #[test]
    fn update_content_only_api_keeps_string_return_contract() {
        let chunks = vec![CodexPatchChunk {
            old_lines: vec!["old".into()],
            new_lines: vec!["new".into()],
            ..Default::default()
        }];
        assert_eq!(
            derive_codex_patch_update("old\n", "file.txt", &chunks).unwrap(),
            "new\n"
        );
    }

    #[test]
    fn parser_rejects_non_prefixed_add_lines() {
        let error = parse_codex_patch(
            "*** Begin Patch\n*** Add File: bad.txt\nnot-prefixed\n*** End Patch",
        )
        .unwrap_err();
        assert_eq!(error.kind, "invalid_hunk");
    }
}

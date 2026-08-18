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

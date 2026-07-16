//! Types, limits, and the sensitive-path guard shared by the `apply_text_edits`
//! host write path (`tool_runtime::files`) and the agent-side wire boundary
//! (`bin/webcodex_agent/patches`). This file is compiled into the main binary
//! as `crate::apply_edits_shared` and `#[path]`-included by the agent binary,
//! so both sides share one definition instead of maintaining parallel copies.
//!
//! It must stay dependency-light: only `serde` and `std`, which both binaries
//! have. Do not add main-crate-only imports here.

use serde::{Deserialize, Serialize};

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
    for comp in path.to_lowercase().split('/') {
        if matches!(
            comp,
            ".git" | "target" | "node_modules" | "projects.d" | "secrets"
        ) {
            return true;
        }
        if comp.starts_with(".env")
            || comp.starts_with("agent.toml")
            || comp.starts_with("webcodex.env")
        {
            return true;
        }
        if comp.ends_with(".env") || comp.ends_with(".toml.bak") {
            return true;
        }
    }
    false
}

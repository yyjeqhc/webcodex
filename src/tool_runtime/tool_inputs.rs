//! Shared input types used by runtime tool calls.

use serde::{Deserialize, Serialize};

/// Serde default helper: `true`. Used by `ToolCall` variants whose `allow_patch`
/// field defaults to true (matching the agent-side project TOML parser).
pub fn default_true() -> bool {
    true
}

pub(crate) const CHECKPOINT_KIND_VALUES: &[&str] = &[
    "snapshot",
    "baseline",
    "before_refactor",
    "after_refactor",
    "last_known_good",
    "rollback_candidate",
];

pub(crate) const CHECKPOINT_VALIDATION_STATUS_VALUES: &[&str] =
    &["unknown", "not_run", "passed", "failed"];

pub(crate) fn is_checkpoint_kind(value: &str) -> bool {
    CHECKPOINT_KIND_VALUES.contains(&value)
}

pub(crate) fn is_checkpoint_validation_status(value: &str) -> bool {
    CHECKPOINT_VALIDATION_STATUS_VALUES.contains(&value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    Normal,
    ReadOnly,
}

impl Default for SessionMode {
    fn default() -> Self {
        Self::Normal
    }
}

impl SessionMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::ReadOnly => "read_only",
        }
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckpointValidationInput {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListToolsOptions {
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default)]
    pub summary_only: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

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

// The `apply_text_edits` wire types are shared verbatim with the agent binary,
// so they live in `crate::apply_edits_shared` and are re-exported here to keep
// existing `tool_inputs::Apply*` import paths working.
pub use crate::apply_edits_shared::{
    ApplyFileChangeInput, ApplyFileChangeKind, ApplyTextEditInput, ApplyTextEditKind,
};

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

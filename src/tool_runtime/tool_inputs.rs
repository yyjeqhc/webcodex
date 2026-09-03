//! Shared input types used by runtime tool calls.

use serde::{Deserialize, Serialize};

pub use webcodex_core::workflow_session_contract::{ExecutionShell, SessionMode};

/// Serde default helper: `true`. Used by `ToolCall` variants whose `allow_patch`
/// field defaults to true (matching the Runner-side Project TOML parser).
pub fn default_true() -> bool {
    true
}

/// Declared intent for a shell/job execution. This is evidence metadata, not
/// an authorization or command-selection policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPurpose {
    Validation,
    Test,
    Build,
    Format,
    Release,
    Diagnostic,
    Operation,
    #[default]
    Other,
}

impl ExecutionPurpose {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Test => "test",
            Self::Build => "build",
            Self::Format => "format",
            Self::Release => "release",
            Self::Diagnostic => "diagnostic",
            Self::Operation => "operation",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupDetail {
    Minimal,
    #[default]
    Standard,
    Full,
}

impl StartupDetail {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Standard => "standard",
            Self::Full => "full",
        }
    }
}

pub(crate) use webcodex_core::runtime_contract::{
    CHECKPOINT_KIND_VALUES, CHECKPOINT_VALIDATION_STATUS_VALUES,
};

pub(crate) fn is_checkpoint_kind(value: &str) -> bool {
    CHECKPOINT_KIND_VALUES.contains(&value)
}

pub(crate) fn is_checkpoint_validation_status(value: &str) -> bool {
    CHECKPOINT_VALIDATION_STATUS_VALUES.contains(&value)
}

// The `apply_text_edits` wire types are shared verbatim with the Runner binary,
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

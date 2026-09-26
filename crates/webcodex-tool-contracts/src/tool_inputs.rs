//! Shared input types used by runtime tool calls.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

pub use webcodex_core::workflow_session_contract::{ExecutionPurpose, ExecutionShell, SessionMode};

/// Serde default helper: `true`. Used by `ToolCall` variants whose `allow_patch`
/// field defaults to true (matching the Runner-side Project TOML parser).
pub fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StartupDetail {
    Minimal,
    #[default]
    Standard,
    Full,
}

/// Request-local model guidance selection, never execution or Session state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CodingGuidanceProfile {
    #[default]
    Direct,
    HostCodeMode,
    #[cfg(feature = "experimental-code-mode")]
    CodeMode,
}

/// Preserve omission for transport-aware effective guidance selection while
/// rejecting explicit JSON null, which is not a valid profile request.
pub fn deserialize_optional_coding_guidance_profile<'de, D>(
    deserializer: D,
) -> Result<Option<CodingGuidanceProfile>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<CodingGuidanceProfile>::deserialize(deserializer)?
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom("guidance_profile must not be null"))
}

impl StartupDetail {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Standard => "standard",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkOnProjectMode {
    #[default]
    Checkout,
    Worktree,
}

impl WorkOnProjectMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Checkout => "checkout",
            Self::Worktree => "worktree",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GoalLifecycleInput {
    Active,
    Completed,
    Cancelled,
}

impl GoalLifecycleInput {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[cfg(feature = "workspace-checkpoints")]
pub use webcodex_core::runtime_contract::{
    CHECKPOINT_KIND_VALUES, CHECKPOINT_VALIDATION_STATUS_VALUES,
};

#[cfg(feature = "workspace-checkpoints")]
pub fn is_checkpoint_kind(value: &str) -> bool {
    CHECKPOINT_KIND_VALUES.contains(&value)
}

#[cfg(feature = "workspace-checkpoints")]
pub fn is_checkpoint_validation_status(value: &str) -> bool {
    CHECKPOINT_VALIDATION_STATUS_VALUES.contains(&value)
}

// Exact edit primitives and kinds remain shared with the Runner wire contract,
// but the model-facing file-change DTO intentionally differs: models carry a
// short read revision while ToolRuntime translates it back to the wire SHA.
pub use webcodex_core::apply_edits_shared::{
    ApplyFileChangeKind, ApplyTextEditInput, ApplyTextEditKind, ApplyTextLineScope,
};

/// Canonical file change. For occurrence, line_scope, expected_match_count, or
/// multiple edits use kind=edit with edits[]; selectors belong inside each edit.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ApplyFileChangeCanonicalInput {
    kind: ApplyFileChangeKind,
    #[schemars(length(min = 1))]
    path: String,
    #[schemars(length(min = 1))]
    #[serde(default)]
    to_path: Option<String>,
    #[serde(default)]
    content: Option<String>,
    /// Independent edits to this file, all resolved against the same original source snapshot.
    /// Use ONE change per file, not repeated changes. Put occurrence/line_scope on these entries.
    #[schemars(length(max = 20))]
    #[serde(default)]
    edits: Vec<ApplyTextEditInput>,
    #[serde(default)]
    #[schemars(range(min = 1, max = 9007199254740991u64))]
    expected_read_revision: Option<u64>,
}

/// Shorthand for ONE simple exact replacement only. For occurrence, line_scope, or
/// multiple edits use canonical kind=edit with edits[] instead; do not mix the forms.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ApplyFileChangeExactReplaceInput {
    #[schemars(length(min = 1))]
    path: String,
    #[schemars(length(min = 1))]
    old_text: String,
    new_text: String,
    #[serde(default)]
    #[schemars(range(min = 1, max = 9007199254740991u64))]
    expected_read_revision: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
enum ApplyFileChangeWireInput {
    Canonical(ApplyFileChangeCanonicalInput),
    ExactReplace(ApplyFileChangeExactReplaceInput),
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[schemars(with = "ApplyFileChangeWireInput")]
pub struct ApplyFileChangeInput {
    pub kind: ApplyFileChangeKind,
    pub path: String,
    pub to_path: Option<String>,
    pub content: Option<String>,
    pub edits: Vec<ApplyTextEditInput>,
    pub expected_read_revision: Option<u64>,
}

impl<'de> Deserialize<'de> for ApplyFileChangeInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match ApplyFileChangeWireInput::deserialize(deserializer)? {
            ApplyFileChangeWireInput::Canonical(input) => Self {
                kind: input.kind,
                path: input.path,
                to_path: input.to_path,
                content: input.content,
                edits: input.edits,
                expected_read_revision: input.expected_read_revision,
            },
            ApplyFileChangeWireInput::ExactReplace(input) => Self {
                kind: ApplyFileChangeKind::Edit,
                path: input.path,
                to_path: None,
                content: None,
                edits: vec![ApplyTextEditInput {
                    kind: ApplyTextEditKind::ReplaceExact,
                    old_text: Some(input.old_text),
                    new_text: Some(input.new_text),
                    anchor_text: None,
                    occurrence: None,
                    expected_match_count: None,
                    line_scope: None,
                }],
                expected_read_revision: input.expected_read_revision,
            },
        })
    }
}

#[cfg(feature = "workspace-checkpoints")]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CheckpointValidationInput {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
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

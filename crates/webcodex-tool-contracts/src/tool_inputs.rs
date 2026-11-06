//! Shared input types used by runtime tool calls.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

mod trace;
pub use trace::ToolTraceQuery;

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

/// Explicit lifecycle filter for read-only Workflow Session discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycleInput {
    Active,
    Closed,
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

fn default_text_edit_kind() -> ApplyTextEditKind {
    ApplyTextEditKind::ReplaceExact
}
fn default_file_change_kind() -> ApplyFileChangeKind {
    ApplyFileChangeKind::Edit
}

/// Exact edits against one original file. kind defaults to replace_exact.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("allOf" = [
    {"if":{"properties":{"kind":{"const":"replace_exact"}}},"then":{"required":["old_text","new_text"],"properties":{"anchor_text":false,"start_line":false,"end_line":false}}},
    {"if":{"properties":{"kind":{"const":"delete_exact"}},"required":["kind"]},"then":{"required":["old_text"],"properties":{"new_text":false,"anchor_text":false,"start_line":false,"end_line":false}}},
    {"if":{"properties":{"kind":{"enum":["insert_before","insert_after"]}},"required":["kind"]},"then":{"required":["anchor_text","new_text"],"properties":{"old_text":false,"start_line":false,"end_line":false}}},
    {"if":{"properties":{"kind":{"const":"replace_range"}},"required":["kind"]},"then":{"required":["start_line","end_line","new_text"],"properties":{"old_text":false,"anchor_text":false,"line_scope":false,"occurrence":false}}},
    {"if":{"required":["expected_match_count"]},"then":{"properties":{"kind":{"const":"replace_exact"},"occurrence":false}}}
]))]
struct ProjectTextEditInput {
    #[serde(default = "default_text_edit_kind")]
    kind: ApplyTextEditKind,
    #[serde(default)]
    #[schemars(length(min = 1, max = 524288))]
    old_text: Option<String>,
    #[serde(default)]
    #[schemars(length(max = 524288))]
    new_text: Option<String>,
    #[serde(default)]
    #[schemars(length(min = 1, max = 524288))]
    anchor_text: Option<String>,
    /// Positional/multiple-match selectors require expected_read_revision.
    #[serde(default)]
    #[schemars(range(min = 1))]
    occurrence: Option<usize>,
    #[serde(default)]
    line_scope: Option<ApplyTextLineScope>,
    #[serde(default)]
    #[schemars(range(min = 1, max = 1024))]
    expected_match_count: Option<usize>,
    /// replace_range only: 1-based inclusive original lines; EOF has no synthetic line.
    #[serde(default)]
    #[schemars(range(min = 1))]
    start_line: Option<usize>,
    #[serde(default)]
    #[schemars(range(min = 1))]
    end_line: Option<usize>,
}

impl ProjectTextEditInput {
    fn validate(&self) -> Result<(), &'static str> {
        if self.occurrence == Some(0) {
            return Err("occurrence must be at least 1");
        }
        if let Some(scope) = self.line_scope {
            scope.validate()?;
        }
        if self
            .expected_match_count
            .is_some_and(|n| !(1..=1024).contains(&n))
        {
            return Err("expected_match_count must be between 1 and 1024");
        }
        if self.expected_match_count.is_some()
            && (self.kind != ApplyTextEditKind::ReplaceExact || self.occurrence.is_some())
        {
            return Err("expected_match_count requires replace_exact without occurrence");
        }
        if self.kind != ApplyTextEditKind::ReplaceRange
            && (self.start_line.is_some() || self.end_line.is_some())
        {
            return Err("start_line/end_line require replace_range");
        }
        match self.kind {
            ApplyTextEditKind::ReplaceExact | ApplyTextEditKind::DeleteExact => {
                if self.old_text.as_deref().is_none_or(str::is_empty) {
                    return Err("old_text must be non-empty");
                }
                if self.anchor_text.is_some() {
                    return Err("anchor_text is not allowed for an exact edit");
                }
                if self.kind == ApplyTextEditKind::ReplaceExact && self.new_text.is_none() {
                    return Err("new_text is required");
                }
                if self.kind == ApplyTextEditKind::DeleteExact && self.new_text.is_some() {
                    return Err("delete_exact does not accept new_text");
                }
            }
            ApplyTextEditKind::InsertBefore | ApplyTextEditKind::InsertAfter => {
                if self.anchor_text.as_deref().is_none_or(str::is_empty) {
                    return Err("anchor_text must be non-empty");
                }
                if self.new_text.is_none() || self.old_text.is_some() {
                    return Err("insertion requires new_text and forbids old_text");
                }
            }
            ApplyTextEditKind::ReplaceRange => {
                let scope = ApplyTextLineScope {
                    start_line: self.start_line.ok_or("start_line is required")?,
                    end_line: self.end_line.ok_or("end_line is required")?,
                };
                scope.validate()?;
                if self.new_text.is_none()
                    || self.old_text.is_some()
                    || self.anchor_text.is_some()
                    || self.line_scope.is_some()
                    || self.occurrence.is_some()
                {
                    return Err("replace_range accepts only start_line, end_line and new_text");
                }
            }
        }
        Ok(())
    }

    fn into_runtime(self) -> ApplyTextEditInput {
        ApplyTextEditInput {
            kind: self.kind,
            old_text: self.old_text,
            new_text: self.new_text,
            anchor_text: self.anchor_text,
            occurrence: self.occurrence,
            expected_match_count: self.expected_match_count,
            line_scope: if self.kind == ApplyTextEditKind::ReplaceRange {
                Some(ApplyTextLineScope {
                    start_line: self.start_line.unwrap(),
                    end_line: self.end_line.unwrap(),
                })
            } else {
                self.line_scope
            },
        }
    }
}

fn serialize_project_text_edits<S: serde::Serializer>(
    edits: &[ApplyTextEditInput],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let values: Vec<serde_json::Value> = edits.iter().map(|edit| {
        if edit.kind == ApplyTextEditKind::ReplaceRange {
            let range = edit.line_scope.expect("replace_range always carries a line scope");
            serde_json::json!({"kind":"replace_range", "start_line":range.start_line, "end_line":range.end_line, "new_text":edit.new_text})
        } else {
            let mut value = serde_json::to_value(edit).expect("serializable text edit");
            value.as_object_mut().unwrap().retain(|_, value| !value.is_null());
            value
        }
    }).collect();
    values.serialize(serializer)
}

/// One change per file. edit defaults to exact replacement. write overwrites an
/// existing file; write/delete/rename and positional edits require a read revision.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("allOf" = [
    {"if":{"properties":{"kind":{"const":"edit"}}},"then":{"required":["edits"],"properties":{"content":false,"to_path":false}}},
    {"if":{"properties":{"kind":{"enum":["create","write"]}},"required":["kind"]},"then":{"required":["content"],"properties":{"edits":false,"to_path":false}}},
    {"if":{"properties":{"kind":{"const":"create"}},"required":["kind"]},"then":{"properties":{"expected_read_revision":false}}},
    {"if":{"properties":{"kind":{"enum":["write","delete","rename"]}},"required":["kind"]},"then":{"required":["expected_read_revision"]}},
    {"if":{"properties":{"kind":{"enum":["delete","rename"]}},"required":["kind"]},"then":{"properties":{"content":false,"edits":false}}},
    {"if":{"properties":{"kind":{"const":"rename"}},"required":["kind"]},"then":{"required":["to_path"]}},
    {"if":{"properties":{"kind":{"const":"delete"}},"required":["kind"]},"then":{"properties":{"to_path":false}}},
    {"if":{"not":{"required":["expected_read_revision"]}},"then":{"properties":{"edits":{"items":{"properties":{"kind":{"enum":["replace_exact","delete_exact"]},"occurrence":false,"line_scope":false,"expected_match_count":false}}}}}}
]))]
struct ApplyFileChangeWireInput {
    #[serde(default = "default_file_change_kind")]
    kind: ApplyFileChangeKind,
    #[schemars(length(min = 1))]
    path: String,
    #[serde(default)]
    to_path: Option<String>,
    #[serde(default)]
    content: Option<String>,
    /// Each edit resolves against the same original content; no fuzzy matching.
    #[serde(default)]
    #[schemars(length(min = 1, max = 20))]
    edits: Vec<ProjectTextEditInput>,
    /// From read_files. Optional only for globally unique exact replacement/deletion;
    /// provided revisions always reject stale content, paths, Projects or Runner instances.
    #[serde(
        default,
        deserialize_with = "crate::read_revision::deserialize_optional"
    )]
    #[schemars(range(min = 1, max = 9007199254740991u64))]
    expected_read_revision: Option<u64>,
}

// Runtime plan data. The model DTO above is the only accepted invocation shape;
// the Runner wire guard remains a separate internal representation.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[schemars(with = "ApplyFileChangeWireInput")]
pub struct ApplyFileChangeInput {
    pub kind: ApplyFileChangeKind,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "serialize_project_text_edits"
    )]
    pub edits: Vec<ApplyTextEditInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_read_revision: Option<u64>,
}

impl<'de> Deserialize<'de> for ApplyFileChangeInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ApplyFileChangeWireInput::deserialize(deserializer)?;
        use serde::de::Error;
        let revision_required = match wire.kind {
            ApplyFileChangeKind::Edit => {
                if wire.edits.is_empty() {
                    return Err(D::Error::missing_field("edits"));
                }
                if wire.to_path.is_some() || wire.content.is_some() {
                    return Err(D::Error::custom(
                        "edit accepts only path, edits and expected_read_revision",
                    ));
                }
                wire.edits.iter().any(|edit| {
                    !matches!(
                        edit.kind,
                        ApplyTextEditKind::ReplaceExact | ApplyTextEditKind::DeleteExact
                    ) || edit.occurrence.is_some()
                        || edit.line_scope.is_some()
                        || edit.expected_match_count.is_some()
                })
            }
            ApplyFileChangeKind::Create | ApplyFileChangeKind::Write => {
                if wire.content.is_none() {
                    return Err(D::Error::missing_field("content"));
                }
                if wire.to_path.is_some()
                    || !wire.edits.is_empty()
                    || (wire.kind == ApplyFileChangeKind::Create
                        && wire.expected_read_revision.is_some())
                {
                    return Err(D::Error::custom("invalid fields for whole-file content"));
                }
                wire.kind == ApplyFileChangeKind::Write
            }
            ApplyFileChangeKind::Delete | ApplyFileChangeKind::Rename => {
                if wire.content.is_some()
                    || !wire.edits.is_empty()
                    || (wire.kind == ApplyFileChangeKind::Delete && wire.to_path.is_some())
                {
                    return Err(D::Error::custom("invalid fields for delete/rename"));
                }
                if wire.kind == ApplyFileChangeKind::Rename && wire.to_path.is_none() {
                    return Err(D::Error::missing_field("to_path"));
                }
                true
            }
        };
        if revision_required && wire.expected_read_revision.is_none() {
            return Err(D::Error::missing_field("expected_read_revision"));
        }
        let edits = wire
            .edits
            .into_iter()
            .map(|edit| {
                edit.validate().map_err(D::Error::custom)?;
                Ok(edit.into_runtime())
            })
            .collect::<Result<Vec<_>, D::Error>>()?;
        Ok(Self {
            kind: wire.kind,
            path: wire.path,
            to_path: wire.to_path,
            content: wire.content,
            edits,
            expected_read_revision: wire.expected_read_revision,
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

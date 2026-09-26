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

/// Text edits resolve against the same revision-fenced original source. No fuzzy matching.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ProjectTextEditInput {
    ReplaceExact {
        #[schemars(length(min = 1, max = 524288))]
        old_text: String,
        #[schemars(length(max = 524288))]
        new_text: String,
        #[serde(default)]
        #[schemars(range(min = 1))]
        occurrence: Option<usize>,
        #[serde(default)]
        line_scope: Option<ApplyTextLineScope>,
        #[serde(default)]
        #[schemars(range(min = 1, max = 1024))]
        expected_match_count: Option<usize>,
    },
    InsertBefore {
        #[schemars(length(min = 1, max = 524288))]
        anchor_text: String,
        #[schemars(length(max = 524288))]
        new_text: String,
        #[serde(default)]
        #[schemars(range(min = 1))]
        occurrence: Option<usize>,
        #[serde(default)]
        line_scope: Option<ApplyTextLineScope>,
    },
    InsertAfter {
        #[schemars(length(min = 1, max = 524288))]
        anchor_text: String,
        #[schemars(length(max = 524288))]
        new_text: String,
        #[serde(default)]
        #[schemars(range(min = 1))]
        occurrence: Option<usize>,
        #[serde(default)]
        line_scope: Option<ApplyTextLineScope>,
    },
    DeleteExact {
        #[schemars(length(min = 1, max = 524288))]
        old_text: String,
        #[serde(default)]
        #[schemars(range(min = 1))]
        occurrence: Option<usize>,
        #[serde(default)]
        line_scope: Option<ApplyTextLineScope>,
    },
    /// Replace complete 1-based inclusive original lines, including their newline when present.
    /// EOF has no synthetic empty line. new_text is literal (no automatic newline); empty deletes.
    /// LF/CRLF follows the existing file convention. Stale revisions reject the entire batch.
    ReplaceRange {
        #[schemars(range(min = 1))]
        start_line: usize,
        #[schemars(range(min = 1))]
        end_line: usize,
        #[schemars(length(max = 524288))]
        new_text: String,
    },
}

impl ProjectTextEditInput {
    fn validate(&self) -> Result<(), &'static str> {
        let validate_selector =
            |occurrence: Option<usize>, line_scope: Option<ApplyTextLineScope>| {
                if occurrence == Some(0) {
                    return Err("occurrence must be at least 1");
                }
                if let Some(line_scope) = line_scope {
                    line_scope.validate()?;
                }
                Ok(())
            };
        match self {
            Self::ReplaceExact {
                old_text,
                occurrence,
                line_scope,
                expected_match_count,
                ..
            } => {
                if old_text.is_empty() {
                    return Err("replace_exact.old_text must be non-empty");
                }
                validate_selector(*occurrence, *line_scope)?;
                if expected_match_count.is_some_and(|count| !(1..=1024).contains(&count)) {
                    return Err("expected_match_count must be between 1 and 1024");
                }
                if occurrence.is_some() && expected_match_count.is_some() {
                    return Err("occurrence and expected_match_count are mutually exclusive");
                }
                Ok(())
            }
            Self::InsertBefore {
                anchor_text,
                occurrence,
                line_scope,
                ..
            }
            | Self::InsertAfter {
                anchor_text,
                occurrence,
                line_scope,
                ..
            } => {
                if anchor_text.is_empty() {
                    return Err("insert anchor_text must be non-empty");
                }
                validate_selector(*occurrence, *line_scope)
            }
            Self::DeleteExact {
                old_text,
                occurrence,
                line_scope,
                ..
            } => {
                if old_text.is_empty() {
                    return Err("delete_exact.old_text must be non-empty");
                }
                validate_selector(*occurrence, *line_scope)
            }
            Self::ReplaceRange {
                start_line,
                end_line,
                ..
            } => ApplyTextLineScope {
                start_line: *start_line,
                end_line: *end_line,
            }
            .validate(),
        }
    }

    fn into_runtime(self) -> ApplyTextEditInput {
        match self {
            Self::ReplaceExact {
                old_text,
                new_text,
                occurrence,
                line_scope,
                expected_match_count,
            } => ApplyTextEditInput {
                kind: ApplyTextEditKind::ReplaceExact,
                old_text: Some(old_text),
                new_text: Some(new_text),
                anchor_text: None,
                occurrence,
                line_scope,
                expected_match_count,
            },
            Self::InsertBefore {
                anchor_text,
                new_text,
                occurrence,
                line_scope,
            } => ApplyTextEditInput {
                kind: ApplyTextEditKind::InsertBefore,
                old_text: None,
                new_text: Some(new_text),
                anchor_text: Some(anchor_text),
                occurrence,
                line_scope,
                expected_match_count: None,
            },
            Self::InsertAfter {
                anchor_text,
                new_text,
                occurrence,
                line_scope,
            } => ApplyTextEditInput {
                kind: ApplyTextEditKind::InsertAfter,
                old_text: None,
                new_text: Some(new_text),
                anchor_text: Some(anchor_text),
                occurrence,
                line_scope,
                expected_match_count: None,
            },
            Self::DeleteExact {
                old_text,
                occurrence,
                line_scope,
            } => ApplyTextEditInput {
                kind: ApplyTextEditKind::DeleteExact,
                old_text: Some(old_text),
                new_text: None,
                anchor_text: None,
                occurrence,
                line_scope,
                expected_match_count: None,
            },
            Self::ReplaceRange {
                start_line,
                end_line,
                new_text,
            } => ApplyTextEditInput {
                kind: ApplyTextEditKind::ReplaceRange,
                new_text: Some(new_text),
                old_text: None,
                anchor_text: None,
                occurrence: None,
                expected_match_count: None,
                line_scope: Some(ApplyTextLineScope {
                    start_line,
                    end_line,
                }),
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

/// One closed file-change variant. Existing sources require the revision returned by read_files.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ApplyFileChangeWireInput {
    Edit {
        #[schemars(length(min = 1))]
        path: String,
        #[schemars(range(min = 1, max = 9007199254740991u64))]
        expected_read_revision: u64,
        /// All edits resolve against the original read snapshot; use ONE change per file.
        #[schemars(length(min = 1, max = 20))]
        edits: Vec<ProjectTextEditInput>,
    },
    Create {
        #[schemars(length(min = 1))]
        path: String,
        content: String,
    },
    Delete {
        #[schemars(length(min = 1))]
        path: String,
        #[schemars(range(min = 1, max = 9007199254740991u64))]
        expected_read_revision: u64,
    },
    Rename {
        #[schemars(length(min = 1))]
        path: String,
        #[schemars(length(min = 1))]
        to_path: String,
        #[schemars(range(min = 1, max = 9007199254740991u64))]
        expected_read_revision: u64,
    },
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
        let (kind, path, to_path, content, edits, revision) =
            match ApplyFileChangeWireInput::deserialize(deserializer)? {
                ApplyFileChangeWireInput::Edit {
                    path,
                    expected_read_revision,
                    edits,
                } => (
                    ApplyFileChangeKind::Edit,
                    path,
                    None,
                    None,
                    edits
                        .into_iter()
                        .map(|edit| {
                            edit.validate().map_err(serde::de::Error::custom)?;
                            Ok(edit.into_runtime())
                        })
                        .collect::<Result<Vec<_>, D::Error>>()?,
                    Some(expected_read_revision),
                ),
                ApplyFileChangeWireInput::Create { path, content } => (
                    ApplyFileChangeKind::Create,
                    path,
                    None,
                    Some(content),
                    vec![],
                    None,
                ),
                ApplyFileChangeWireInput::Delete {
                    path,
                    expected_read_revision,
                } => (
                    ApplyFileChangeKind::Delete,
                    path,
                    None,
                    None,
                    vec![],
                    Some(expected_read_revision),
                ),
                ApplyFileChangeWireInput::Rename {
                    path,
                    to_path,
                    expected_read_revision,
                } => (
                    ApplyFileChangeKind::Rename,
                    path,
                    Some(to_path),
                    None,
                    vec![],
                    Some(expected_read_revision),
                ),
            };
        if revision.is_some_and(|r| !(1..=9_007_199_254_740_991).contains(&r)) {
            return Err(serde::de::Error::custom(
                "expected_read_revision must be a positive JSON-safe integer",
            ));
        }
        Ok(Self {
            kind,
            path,
            to_path,
            content,
            edits,
            expected_read_revision: revision,
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

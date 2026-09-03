//! Wire-format input models for the connector capability surface.
//!
//! These `#[derive(Deserialize)]` structs are the typed argument shapes for
//! the runtime's capability methods (`task_start`, `files_*`, `edits_apply`,
//! `checks_run`, `commands_run`, `task_*`) plus the transport-field sanitiser
//! that scrubs executor identities out of results before they cross the wire.
//! `TaskReviewInput` and `TaskCancelInput` remain `pub(crate)` because the
//! host/http layers reference them directly; the rest are `pub(super)` for the
//! runtime module's own dispatch.

use crate::lsp_bridge::{
    CallHierarchyDirection, DEFAULT_CALL_HIERARCHY_DEPTH, DEFAULT_CALL_HIERARCHY_LIMIT,
};
use crate::tool_runtime::{ApplyFileChangeInput, SearchResultMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use webcodex_connector_runtime::{
    ConnectorRecipeId as RecipeId, ConnectorSemanticCheck as SemanticCheck,
};

pub(super) fn sanitize_value(
    value: &mut Value,
    executor_project: &str,
    logical_project: &str,
    executor_root: &str,
) {
    match value {
        Value::String(string) => {
            if string.contains(executor_project) {
                *string = string.replace(executor_project, logical_project);
            }
            if string.contains(executor_root) {
                *string = string.replace(executor_root, ".");
            }
        }
        Value::Array(items) => {
            for item in items {
                sanitize_value(item, executor_project, logical_project, executor_root);
            }
        }
        Value::Object(object) => {
            for transport_field in [
                "client_id",
                "agent_instance_id",
                "executor",
                "executor_id",
                "execution_executor_ref",
                "request_id",
                "runtime_project_id",
            ] {
                object.remove(transport_field);
            }
            for item in object.values_mut() {
                sanitize_value(item, executor_project, logical_project, executor_root);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskStartInput {
    pub(super) goal: String,
    #[serde(default)]
    pub(super) mode: ConnectorTaskMode,
    /// Optional project-relative directory whose nested repository rules apply
    /// to the current instruction.
    #[serde(default)]
    pub(super) target_path: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ConnectorTaskMode {
    #[default]
    Normal,
    ReadOnly,
}

impl ConnectorTaskMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::ReadOnly => "read_only",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FilesReadInput {
    pub(super) task_id: String,
    pub(super) files: Vec<FileReadInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileReadInput {
    pub(super) path: String,
    #[serde(default)]
    pub(super) start_line: Option<usize>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
    #[serde(default)]
    pub(super) with_line_numbers: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FilesListInput {
    pub(super) task_id: String,
    #[serde(default)]
    pub(super) path: Option<String>,
    #[serde(default)]
    pub(super) globs: Vec<String>,
    #[serde(default)]
    pub(super) depth: Option<usize>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
    #[serde(default)]
    pub(super) offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FilesSearchInput {
    pub(super) task_id: String,
    pub(super) pattern: String,
    #[serde(default)]
    pub(super) path: Option<String>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
    #[serde(default)]
    pub(super) context_before: Option<usize>,
    #[serde(default)]
    pub(super) context_after: Option<usize>,
    #[serde(default)]
    pub(super) include_globs: Vec<String>,
    #[serde(default)]
    pub(super) exclude_globs: Vec<String>,
    #[serde(default)]
    pub(super) result_mode: Option<SearchResultMode>,
    #[serde(default)]
    pub(super) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodeNavigateInput {
    pub(super) task_id: String,
    pub(super) operation: CodeNavigateOperation,
    #[serde(default)]
    pub(super) path: Option<String>,
    #[serde(default)]
    pub(super) query: Option<String>,
    #[serde(default)]
    pub(super) line: Option<usize>,
    #[serde(default)]
    pub(super) column: Option<usize>,
    #[serde(default)]
    pub(super) include_declaration: Option<bool>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CodeNavigateOperation {
    Status,
    DocumentSymbols,
    WorkspaceSymbols,
    Definition,
    References,
    Diagnostics,
    Hover,
}

impl CodeNavigateOperation {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::DocumentSymbols => "document_symbols",
            Self::WorkspaceSymbols => "workspace_symbols",
            Self::Definition => "definition",
            Self::References => "references",
            Self::Diagnostics => "diagnostics",
            Self::Hover => "hover",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodeImpactInput {
    pub(super) task_id: String,
    pub(super) path: String,
    pub(super) line: usize,
    pub(super) column: usize,
    #[serde(default)]
    pub(super) direction: CallHierarchyDirection,
    #[serde(default = "default_code_impact_depth")]
    pub(super) depth: usize,
    #[serde(default = "default_code_impact_limit")]
    pub(super) limit: usize,
}

fn default_code_impact_depth() -> usize {
    DEFAULT_CALL_HIERARCHY_DEPTH
}

fn default_code_impact_limit() -> usize {
    DEFAULT_CALL_HIERARCHY_LIMIT
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EditsApplyInput {
    pub(super) task_id: String,
    pub(super) operation_id: String,
    pub(super) changes: Vec<ApplyFileChangeInput>,
    #[serde(default)]
    pub(super) dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChecksRunInput {
    pub(super) task_id: String,
    pub(super) operation_id: String,
    pub(super) checks: Vec<SemanticCheck>,
    #[serde(default)]
    pub(super) recipe: Option<RecipeId>,
    #[serde(default)]
    pub(super) cwd: Option<String>,
    #[serde(default)]
    pub(super) test_filter: Option<String>,
    #[serde(default)]
    pub(super) timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandsRunInput {
    pub(super) task_id: String,
    pub(super) operation_id: String,
    pub(super) command: String,
    #[serde(default)]
    pub(super) cwd: Option<String>,
    #[serde(default)]
    pub(super) timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskListInput {
    #[serde(default)]
    pub(super) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskResumeInput {
    pub(super) task_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskReviewInput {
    pub(crate) task_id: String,
    #[serde(default)]
    pub(crate) include_diff: Option<bool>,
    #[serde(default)]
    pub(crate) after_cursor: Option<i64>,
    #[serde(default)]
    pub(crate) wait_ms: Option<u64>,
    #[serde(default)]
    pub(crate) max_events: Option<usize>,
    #[serde(default)]
    pub(crate) include_output_tail: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskCancelInput {
    pub(crate) task_id: String,
    #[serde(default)]
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskFinishInput {
    pub(super) task_id: String,
    pub(super) summary: String,
}

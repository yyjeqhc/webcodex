//! Canonical execution bindings and prestart-denial projections.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CanonicalProjectOutput {
    Canonical,
    Requested,
}

/// Select only inner adapters that still perform an auth-less legacy Project
/// lookup. The outer dispatcher has already resolved and authorized the caller's
/// selector under the current principal; these adapters need that canonical id
/// for execution. The output mode is decided in the same match so adding another
/// legacy adapter cannot silently change whether its `project` field echoes the
/// caller selector or the canonical execution target.
pub(super) fn canonical_execution_project_binding(
    call: &mut ToolCall,
) -> Option<(&mut String, CanonicalProjectOutput)> {
    match call {
        ToolCall::GitDiffHunks { project, .. }
        | ToolCall::GitReviewSummary { project, .. }
        | ToolCall::ReviewChanges { project, .. }
        | ToolCall::ShowChanges { project, .. }
        | ToolCall::WorkspaceHygieneCheck { project, .. } => {
            Some((project, CanonicalProjectOutput::Requested))
        }
        #[cfg(feature = "workspace-checkpoints")]
        ToolCall::WorkspaceCheckpointCreate { project, .. }
        | ToolCall::WorkspaceCheckpointList { project, .. }
        | ToolCall::WorkspaceCheckpointShow { project, .. }
        | ToolCall::WorkspaceCheckpointRestore { project, .. }
        | ToolCall::WorkspaceCheckpointDelete { project, .. } => {
            Some((project, CanonicalProjectOutput::Requested))
        }
        ToolCall::RunProcess { project, .. }
        | ToolCall::RunDetachedProcess { project, .. }
        | ToolCall::StartAgentTaskCodingRun { project, .. }
        | ToolCall::RunScript { project, .. }
        | ToolCall::RunShell { project, .. }
        | ToolCall::OpenSessionShell { project, .. }
        | ToolCall::SessionShellExec { project, .. }
        | ToolCall::SessionShellStatus { project, .. }
        | ToolCall::CloseSessionShell { project, .. }
        | ToolCall::ApplyPatch { project, .. }
        | ToolCall::ApplyUnifiedDiff { project, .. }
        | ToolCall::DeleteProjectFiles { project, .. }
        | ToolCall::GitRestorePaths { project, .. }
        | ToolCall::DiscardUntracked { project, .. }
        | ToolCall::GitCommitPaths { project, .. }
        | ToolCall::GitStatus { project, .. }
        | ToolCall::GitLog { project, .. }
        | ToolCall::CargoFmt { project, .. }
        | ToolCall::CargoCheck { project, .. }
        | ToolCall::CargoTest { project, .. }
        | ToolCall::ProjectBuild { project, .. }
        | ToolCall::ProjectValidate { project, .. }
        | ToolCall::GoTest { project, .. }
        | ToolCall::ListProjectFiles { project, .. }
        | ToolCall::ListProjectTrackedFiles { project, .. }
        | ToolCall::ProjectOverview { project, .. }
        | ToolCall::WriteProjectFile { project, .. }
        | ToolCall::SaveProjectArtifact { project, .. }
        | ToolCall::PresentSpreadsheet { project, .. }
        | ToolCall::ProjectArtifact { project, .. }
        | ToolCall::ReadProjectArtifactMetadata { project, .. }
        | ToolCall::ReadProjectArtifact { project, .. }
        | ToolCall::ArtifactUploadBegin { project, .. }
        | ToolCall::ArtifactUploadChunk { project, .. }
        | ToolCall::ArtifactUploadFinish { project, .. }
        | ToolCall::ArtifactUploadAbort { project, .. }
        | ToolCall::ApplyTextEdits { project, .. }
        | ToolCall::LspStatus { project, .. }
        | ToolCall::DocumentSymbols { project, .. }
        | ToolCall::DocumentDiagnostics { project, .. }
        | ToolCall::Hover { project, .. }
        | ToolCall::WorkspaceSymbols { project, .. }
        | ToolCall::GotoDefinition { project, .. }
        | ToolCall::FindReferences { project, .. }
        | ToolCall::CallHierarchy { project, .. } => {
            Some((project, CanonicalProjectOutput::Canonical))
        }
        _ => None,
    }
}

/// Add the Phase A lifecycle tuple to a definite pre-execution structured
/// execution denial without changing generic denial helpers used by unrelated
/// tools.
pub(in crate::tool_runtime) fn decorate_structured_execution_prestart_denial(
    tool_name: &str,
    result: &mut ToolResult,
    fallback_failure_kind: &'static str,
) {
    let structured_execution = matches!(
        tool_name,
        "run_process" | "run_detached_process" | "run_script" | "run_skill_resource"
    );
    let structured_mutation = tool_name == "edit_project_files";
    if !structured_execution && !structured_mutation {
        return;
    }
    let mut output = match std::mem::take(&mut result.output) {
        Value::Object(output) => output,
        other => {
            let mut output = serde_json::Map::new();
            output.insert("value".to_string(), other);
            output
        }
    };
    let failure_kind = output
        .get("failure_kind")
        .and_then(Value::as_str)
        .or_else(|| output.get("error_kind").and_then(Value::as_str))
        .or_else(|| output.get("code").and_then(Value::as_str))
        .unwrap_or(fallback_failure_kind)
        .to_string();
    output.insert(
        "execution_state".to_string(),
        Value::String("not_started".to_string()),
    );
    output.insert("failure_kind".to_string(), Value::String(failure_kind));
    output.insert("tool_failure".to_string(), Value::Bool(true));
    if structured_mutation {
        // These Runtime gates precede business mutation dispatch, so the
        // canonical mutation result can authoritatively prove no state changed.
        output.insert("state_changed".to_string(), Value::Bool(false));
    } else {
        output.insert("command_started".to_string(), Value::Bool(false));
        output.insert("command_completed".to_string(), Value::Bool(false));
        output.insert("command_ok".to_string(), Value::Bool(false));
        output.insert("exit_code".to_string(), Value::Null);
    }
    result.output = Value::Object(output);
}

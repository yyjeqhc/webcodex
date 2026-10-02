//! Typed file-operation routing after Runner policy and target validation.
use super::artifacts::handle_artifact_file_operation_with_store;
#[cfg(feature = "workspace-checkpoints")]
use super::checkpoints::handle_checkpoint_file_request;
#[cfg(all(test, feature = "workspace-checkpoints"))]
use super::checkpoints::is_checkpoint_request_kind;
use super::config::RunnerPolicy;
use super::files::{handle_basic_file_request, resolve_requested_path};
use super::output::CommandResult;
use super::patches::{
    handle_apply_patch_file_request, handle_apply_text_edits_file_request,
    handle_write_project_file_request, validate_structured_edit_runner_path,
};
#[cfg(test)]
use super::{
    is_artifact_request_kind, is_basic_file_request_kind, is_structured_edit_request_kind,
};
use std::path::Path;
use std::time::Instant;
use webcodex_core::runner_operation::RunnerFileOperation;
#[cfg(test)]
use webcodex_core::{runner_operation::RunnerOperation, runner_protocol::RunnerRequest};

#[cfg(test)]
pub(crate) fn is_file_request_kind(kind: &str) -> bool {
    #[cfg(feature = "workspace-checkpoints")]
    if is_checkpoint_request_kind(kind) {
        return true;
    }
    is_basic_file_request_kind(kind)
        || is_structured_edit_request_kind(kind)
        || is_artifact_request_kind(kind)
}

#[cfg(test)]
pub(crate) fn handle_file_operation(
    policy: &RunnerPolicy,
    operation: &RunnerFileOperation,
) -> CommandResult {
    handle_file_operation_with_artifact_store(policy, operation, None)
}

pub(crate) fn handle_file_operation_with_artifact_store(
    policy: &RunnerPolicy,
    operation: &RunnerFileOperation,
    artifact_store_root: Option<&Path>,
) -> CommandResult {
    let request = operation.payload();
    let path = request.path.as_str();
    let start = Instant::now();
    if matches!(
        operation,
        RunnerFileOperation::WriteProjectFile(_)
            | RunnerFileOperation::ApplyTextEdits(_)
            | RunnerFileOperation::ApplyPatch(_)
    ) {
        if let Err(e) = validate_structured_edit_runner_path(path) {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(e),
            };
        }
    }
    let resolved = match resolve_requested_path(policy, request.cwd.as_deref(), path) {
        Ok(path) => path,
        Err(e) => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(e),
            }
        }
    };
    match operation {
        RunnerFileOperation::WriteProjectFile(_) => {
            handle_write_project_file_request(request, &resolved, start)
        }
        RunnerFileOperation::ApplyTextEdits(_) => {
            handle_apply_text_edits_file_request(policy, request, start)
        }
        RunnerFileOperation::ApplyPatch(_) => {
            handle_apply_patch_file_request(policy, request, start)
        }
        RunnerFileOperation::SaveProjectArtifact(_)
        | RunnerFileOperation::ReadProjectArtifactMetadata(_)
        | RunnerFileOperation::ReadProjectArtifact(_)
        | RunnerFileOperation::ReadProjectArtifactExportChunk(_)
        | RunnerFileOperation::ArtifactUploadBegin(_)
        | RunnerFileOperation::ArtifactUploadChunk(_)
        | RunnerFileOperation::ArtifactUploadFinish(_)
        | RunnerFileOperation::ArtifactUploadAbort(_) => handle_artifact_file_operation_with_store(
            operation,
            &resolved,
            start,
            artifact_store_root,
        ),
        #[cfg(feature = "workspace-checkpoints")]
        RunnerFileOperation::CheckpointCreate(_) | RunnerFileOperation::CheckpointRestore(_) => {
            handle_checkpoint_file_request(operation, &resolved, start)
        }
        #[cfg(not(feature = "workspace-checkpoints"))]
        RunnerFileOperation::CheckpointCreate(_) | RunnerFileOperation::CheckpointRestore(_) => {
            CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(
                    "workspace checkpoints are unsupported in this Runner build".to_string(),
                ),
            }
        }
        RunnerFileOperation::Read(_)
        | RunnerFileOperation::Write(_)
        | RunnerFileOperation::List(_)
        | RunnerFileOperation::ProjectOverview(_)
        | RunnerFileOperation::DeleteProjectFiles(_)
        | RunnerFileOperation::SkillListPackages(_)
        | RunnerFileOperation::SkillReadFile(_) => {
            handle_basic_file_request(policy, operation, &resolved, start)
        }
    }
}

#[cfg(test)]
pub(crate) fn handle_file_request(policy: &RunnerPolicy, request: &RunnerRequest) -> CommandResult {
    match request.decode_operation() {
        Ok(RunnerOperation::File(operation)) => handle_file_operation(policy, &operation),
        _ => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("invalid file request".to_string()),
        },
    }
}

//! Runtime dispatch adapters for file, artifact, and text-edit tool calls.

use super::{ToolCall, ToolResult, ToolRuntime};

impl ToolRuntime {
    pub(crate) async fn dispatch_file_tool(&self, call: ToolCall) -> ToolResult {
        match call {
            ToolCall::DeleteProjectFiles {
                project,
                paths,
                session_id: _,
            } => self.delete_project_files(project, paths).await,
            ToolCall::ReadFile {
                project,
                path,
                session_id: _,
                start_line,
                limit,
                with_line_numbers,
            } => {
                self.read_file(project, path, start_line, limit, with_line_numbers)
                    .await
            }
            ToolCall::ListProjectFiles {
                project,
                session_id: _,
                path,
                limit,
            } => self.list_project_files(project, path, limit).await,
            ToolCall::SearchProjectText {
                project,
                pattern,
                session_id: _,
                path,
                limit,
                context_before,
                context_after,
                include_globs,
                exclude_globs,
                result_mode,
                timeout_secs,
            } => {
                self.search_project_text(
                    project,
                    pattern,
                    path,
                    limit,
                    context_before,
                    context_after,
                    include_globs,
                    exclude_globs,
                    result_mode,
                    timeout_secs,
                )
                .await
            }
            ToolCall::ReplaceInFile {
                project,
                path,
                old,
                new,
                session_id: _,
                expected_replacements,
                allow_multiple,
            } => {
                self.replace_in_file(
                    project,
                    path,
                    old,
                    new,
                    expected_replacements,
                    allow_multiple,
                )
                .await
            }
            ToolCall::ReplaceExactBlock {
                project,
                path,
                old_text,
                new_text,
                session_id: _,
                expected_old_sha256,
            } => {
                self.replace_exact_block(project, path, old_text, new_text, expected_old_sha256)
                    .await
            }
            ToolCall::InsertBeforePattern {
                project,
                path,
                pattern,
                text,
                session_id: _,
            } => {
                self.insert_around_pattern(project, path, pattern, text, "insert_before_pattern")
                    .await
            }
            ToolCall::InsertAfterPattern {
                project,
                path,
                pattern,
                text,
                session_id: _,
            } => {
                self.insert_around_pattern(project, path, pattern, text, "insert_after_pattern")
                    .await
            }
            ToolCall::WriteProjectFile {
                project,
                path,
                content,
                session_id: _,
                overwrite,
                expected_sha256,
                expected_content_prefix,
            } => {
                self.write_project_file(
                    project,
                    path,
                    content,
                    overwrite,
                    expected_sha256,
                    expected_content_prefix,
                )
                .await
            }
            ToolCall::SaveProjectArtifact {
                project,
                path,
                content_base64,
                session_id: _,
                mime_type,
                overwrite,
            } => {
                self.save_project_artifact(project, path, content_base64, mime_type, overwrite)
                    .await
            }
            ToolCall::ReadProjectArtifactMetadata {
                project,
                path,
                session_id: _,
                allow_missing,
            } => {
                self.read_project_artifact_metadata(project, path, allow_missing)
                    .await
            }
            ToolCall::ReadProjectArtifact {
                project,
                path,
                session_id: _,
                encoding,
                offset,
                length,
                max_bytes,
            } => {
                self.read_project_artifact(project, path, encoding, offset, length, max_bytes)
                    .await
            }
            ToolCall::ArtifactUploadBegin {
                project,
                path,
                session_id: _,
                expected_bytes,
                expected_sha256,
                mime_type,
                overwrite,
            } => {
                self.artifact_upload_begin(
                    project,
                    path,
                    expected_bytes,
                    expected_sha256,
                    mime_type,
                    overwrite,
                )
                .await
            }
            ToolCall::ArtifactUploadChunk {
                project,
                path,
                upload_id,
                offset,
                content_base64,
                session_id: _,
            } => {
                self.artifact_upload_chunk(project, path, upload_id, offset, content_base64)
                    .await
            }
            ToolCall::ArtifactUploadFinish {
                project,
                path,
                upload_id,
                session_id: _,
            } => self.artifact_upload_finish(project, path, upload_id).await,
            ToolCall::ArtifactUploadAbort {
                project,
                path,
                upload_id,
                session_id: _,
            } => self.artifact_upload_abort(project, path, upload_id).await,
            ToolCall::ReplaceLineRange {
                project,
                path,
                start_line,
                end_line,
                new_text,
                session_id: _,
                expected_old_sha256,
                expected_old_prefix,
            } => {
                self.replace_line_range(
                    project,
                    path,
                    start_line,
                    end_line,
                    new_text,
                    expected_old_sha256,
                    expected_old_prefix,
                )
                .await
            }
            ToolCall::InsertAtLine {
                project,
                path,
                line,
                text,
                session_id: _,
                expected_anchor_sha256,
                expected_anchor_prefix,
            } => {
                self.insert_at_line(
                    project,
                    path,
                    line,
                    text,
                    expected_anchor_sha256,
                    expected_anchor_prefix,
                )
                .await
            }
            ToolCall::DeleteLineRange {
                project,
                path,
                start_line,
                end_line,
                session_id: _,
                expected_old_sha256,
                expected_old_prefix,
            } => {
                self.delete_line_range(
                    project,
                    path,
                    start_line,
                    end_line,
                    expected_old_sha256,
                    expected_old_prefix,
                )
                .await
            }
            ToolCall::ApplyTextEdits {
                project,
                path,
                edits,
                dry_run,
                expected_file_sha256,
                session_id: _,
            } => {
                self.apply_text_edits(project, path, edits, dry_run, expected_file_sha256)
                    .await
            }
            _ => unreachable!("non-file tool routed to file dispatcher"),
        }
    }
}

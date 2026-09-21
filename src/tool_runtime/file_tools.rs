//! Runtime dispatch adapters for file, artifact, and text-edit tool calls.

use super::project_resolution::{ProjectResolverError, ResolvedProject};
use super::{sessions::SessionTransport, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_file_tool(
        &self,
        call: ToolCall,
        transport: SessionTransport,
        project_resolution: Option<Result<ResolvedProject, ProjectResolverError>>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::DeleteProjectFiles {
                project,
                paths,
                session_id: _,
            } => self.delete_project_files(project, paths).await,
            ToolCall::ReadFiles {
                project,
                items,
                session_id: _,
                with_line_numbers,
                max_result_bytes: _,
            } => match project_resolution {
                Some(Ok(resolved)) => {
                    self.read_files_resolved(&resolved, items, with_line_numbers)
                        .await
                }
                Some(Err(error)) => error.into_tool_result(),
                None => self.read_files(project, items, with_line_numbers).await,
            },
            ToolCall::ListProjectFiles {
                project,
                session_id: _,
                path,
                limit,
                offset,
            } => self.list_project_files(project, path, limit, offset).await,
            ToolCall::ListProjectTrackedFiles {
                project,
                session_id: _,
                path,
                globs,
                depth,
                limit,
                offset,
            } => {
                self.list_project_tracked_files(project, path, globs, depth, limit, offset)
                    .await
            }
            ToolCall::ProjectOverview {
                project,
                session_id: _,
                path,
                max_depth,
                limit,
            } => self.project_overview(project, path, max_depth, limit).await,
            ToolCall::SearchProjectTexts {
                project,
                queries,
                session_id: _,
                max_result_bytes: _,
            } => match project_resolution {
                Some(Ok(resolved)) => self.search_project_texts_resolved(&resolved, queries).await,
                Some(Err(error)) => error.into_tool_result(),
                None => self.search_project_texts(project, queries).await,
            },
            ToolCall::SearchAndRead {
                project,
                query,
                session_id,
                read_before,
                read_after,
                max_reads,
                with_line_numbers,
            } => match project_resolution {
                Some(Ok(resolved)) => {
                    self.search_and_read_resolved(
                        &resolved,
                        query,
                        session_id,
                        read_before,
                        read_after,
                        max_reads,
                        with_line_numbers,
                    )
                    .await
                }
                Some(Err(error)) => error.into_tool_result(),
                None => {
                    self.search_and_read(
                        project,
                        query,
                        session_id,
                        read_before,
                        read_after,
                        max_reads,
                        with_line_numbers,
                    )
                    .await
                }
            },
            ToolCall::WriteProjectFile {
                project,
                path,
                content,
                session_id: _,
                overwrite,
                expected_read_revision,
            } => {
                self.write_project_file(project, path, content, overwrite, expected_read_revision)
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
            ToolCall::ProjectArtifact {
                project,
                path,
                action,
                session_id,
                allow_missing,
                offset,
                length,
                expected_sha256,
            } => match action {
                super::ProjectArtifactAction::Metadata => {
                    self.read_project_artifact_metadata(project, path, allow_missing)
                        .await
                }
                super::ProjectArtifactAction::Inspect => {
                    let mut result = self
                        .read_project_artifact(
                            project,
                            path,
                            None,
                            offset,
                            length,
                            expected_sha256,
                            session_id,
                            None,
                        )
                        .await;
                    if let Some(suggested_call) = result
                        .output
                        .get_mut("suggested_call")
                        .and_then(serde_json::Value::as_object_mut)
                    {
                        if suggested_call
                            .get("tool")
                            .and_then(serde_json::Value::as_str)
                            == Some("read_project_artifact")
                        {
                            suggested_call
                                .insert("tool".to_string(), serde_json::json!("project_artifact"));
                            if let Some(arguments) = suggested_call
                                .get_mut("arguments")
                                .and_then(serde_json::Value::as_object_mut)
                            {
                                arguments.remove("encoding");
                                arguments
                                    .insert("action".to_string(), serde_json::json!("inspect"));
                            }
                        }
                    }
                    result
                }
                super::ProjectArtifactAction::Image => {
                    if !matches!(transport, SessionTransport::Mcp) {
                        ToolResult::err_with_output(
                            "project_artifact action=image requires MCP native-image transport",
                            serde_json::json!({
                                "error_kind": "unsupported_transport",
                                "action": "image",
                                "required_transport": "mcp",
                            }),
                        )
                    } else {
                        self.read_project_artifact(
                            project,
                            path,
                            None,
                            None,
                            None,
                            None,
                            session_id,
                            Some(true),
                        )
                        .await
                    }
                }
                super::ProjectArtifactAction::Export => {
                    if !matches!(transport, SessionTransport::Mcp) {
                        ToolResult::err_with_output(
                            "project_artifact action=export requires Stateless MCP 2026 ResourceLink transport",
                            serde_json::json!({
                                "error_kind": "unsupported_transport",
                                "action": "export",
                                "required_transport": "mcp",
                            }),
                        )
                    } else {
                        match project_resolution {
                            Some(Ok(resolved)) => {
                                self.export_project_artifact_metadata_resolved(&resolved, path, auth)
                                    .await
                            }
                            Some(Err(error)) => error.into_tool_result(),
                            None => ToolResult::err(
                                "project_artifact action=export requires an exact resolved Runner project",
                            ),
                        }
                    }
                }
            },
            ToolCall::ExportProjectArtifact {
                project: _,
                path,
                session_id: _,
            } => {
                if !matches!(transport, SessionTransport::Mcp) {
                    ToolResult::err(
                        "export_project_artifact is MCP-only; use read_project_artifact for bounded inspection outside MCP",
                    )
                } else {
                    match project_resolution {
                        Some(Ok(resolved)) => {
                            self.export_project_artifact_metadata_resolved(&resolved, path, auth)
                                .await
                        }
                        Some(Err(error)) => error.into_tool_result(),
                        None => ToolResult::err(
                            "export_project_artifact requires an exact resolved Runner project",
                        ),
                    }
                }
            }
            ToolCall::ProjectArtifactDownloadLink {
                project,
                path,
                session_id: _,
            } => {
                crate::artifact_download_http::issue_download_link(self, project, path, auth).await
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
                session_id,
                encoding,
                offset,
                length,
                expected_sha256,
                as_image,
            } => {
                if as_image == Some(true) && !matches!(transport, SessionTransport::Mcp) {
                    ToolResult::err(
                        "as_image is only supported over MCP; omit it to use the existing chunked artifact response",
                    )
                } else {
                    self.read_project_artifact(
                        project,
                        path,
                        encoding,
                        offset,
                        length,
                        expected_sha256,
                        session_id,
                        as_image,
                    )
                    .await
                }
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
            ToolCall::ApplyTextEdits {
                project,
                changes,
                dry_run,
                session_id: _,
            } => self.apply_text_edits(project, changes, dry_run).await,
            _ => unreachable!("non-file tool routed to file dispatcher"),
        }
    }
}

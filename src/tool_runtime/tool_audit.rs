//! Audit-safe argument summaries for runtime tool calls.

use super::tool_call::ToolCall;
use super::tool_inputs::{is_checkpoint_kind, is_checkpoint_validation_status};
use serde_json::Value;

pub(crate) fn session_log_arguments_for_tool_request(tool_name: &str, arguments: &Value) -> Value {
    let Some(obj) = arguments.as_object() else {
        return Value::Null;
    };
    let mut out = serde_json::Map::new();
    if let Some(project) = obj.get("project").cloned() {
        out.insert("project".to_string(), project);
    }
    match tool_name {
        "run_shell" | "run_job" => {
            out.insert(
                "command_present".to_string(),
                Value::Bool(obj.contains_key("command")),
            );
            copy_keys(obj, &mut out, &["timeout_secs", "cwd"]);
        }
        "write_project_file" => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "path",
                    "overwrite",
                    "expected_sha256",
                    "expected_content_prefix",
                ],
            );
            out.insert(
                "content_present".to_string(),
                Value::Bool(obj.contains_key("content")),
            );
        }
        "save_project_artifact" => {
            copy_keys(obj, &mut out, &["path", "mime_type", "overwrite"]);
            out.insert(
                "content_base64_present".to_string(),
                Value::Bool(obj.contains_key("content_base64")),
            );
        }
        "artifact_upload_begin" => {
            copy_keys(
                obj,
                &mut out,
                &["path", "expected_bytes", "mime_type", "overwrite"],
            );
            out.insert(
                "expected_sha256_present".to_string(),
                Value::Bool(obj.contains_key("expected_sha256")),
            );
        }
        "artifact_upload_chunk" => {
            copy_keys(obj, &mut out, &["path", "upload_id", "offset"]);
            out.insert(
                "content_base64_present".to_string(),
                Value::Bool(obj.contains_key("content_base64")),
            );
        }
        "artifact_upload_finish" | "artifact_upload_abort" => {
            copy_keys(obj, &mut out, &["path", "upload_id"]);
        }
        "apply_patch" | "apply_patch_checked" | "validate_patch" => {
            out.insert(
                "patch_present".to_string(),
                Value::Bool(obj.contains_key("patch")),
            );
            copy_keys(obj, &mut out, &["deny_sensitive_paths"]);
        }
        "replace_in_file" => {
            copy_keys(
                obj,
                &mut out,
                &["path", "expected_replacements", "allow_multiple"],
            );
            out.insert(
                "old_present".to_string(),
                Value::Bool(obj.contains_key("old")),
            );
            out.insert(
                "new_present".to_string(),
                Value::Bool(obj.contains_key("new")),
            );
        }
        "replace_exact_block" => {
            copy_keys(obj, &mut out, &["path", "expected_old_sha256"]);
            out.insert(
                "old_text_present".to_string(),
                Value::Bool(obj.contains_key("old_text")),
            );
            out.insert(
                "new_text_present".to_string(),
                Value::Bool(obj.contains_key("new_text")),
            );
        }
        "insert_before_pattern" | "insert_after_pattern" => {
            copy_keys(obj, &mut out, &["path"]);
            out.insert(
                "pattern_present".to_string(),
                Value::Bool(obj.contains_key("pattern")),
            );
            out.insert(
                "text_present".to_string(),
                Value::Bool(obj.contains_key("text")),
            );
        }
        "replace_line_range" => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "path",
                    "start_line",
                    "end_line",
                    "expected_old_sha256",
                    "expected_old_prefix",
                ],
            );
            out.insert(
                "new_text_present".to_string(),
                Value::Bool(obj.contains_key("new_text")),
            );
        }
        "insert_at_line" => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "path",
                    "line",
                    "expected_anchor_sha256",
                    "expected_anchor_prefix",
                ],
            );
            out.insert(
                "text_present".to_string(),
                Value::Bool(obj.contains_key("text")),
            );
        }
        "delete_line_range" => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "path",
                    "start_line",
                    "end_line",
                    "expected_old_sha256",
                    "expected_old_prefix",
                ],
            );
        }
        "delete_project_files" | "git_restore_paths" | "discard_untracked" => {
            copy_keys(obj, &mut out, &["paths"]);
        }
        "workspace_checkpoint_create" => {
            copy_keys(obj, &mut out, &["title", "include_untracked"]);
            out.insert(
                "note_present".to_string(),
                Value::Bool(obj.contains_key("note")),
            );
            let kind = obj
                .get("kind")
                .and_then(Value::as_str)
                .filter(|value| is_checkpoint_kind(value))
                .unwrap_or(if obj.get("kind").is_some() {
                    "invalid"
                } else {
                    "snapshot"
                });
            out.insert("kind".to_string(), Value::String(kind.to_string()));
            let label_count = obj
                .get("labels")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            out.insert("label_count".to_string(), Value::from(label_count));
            let validation_status = obj
                .get("validation")
                .and_then(Value::as_object)
                .and_then(|validation| validation.get("status"))
                .and_then(Value::as_str)
                .filter(|value| is_checkpoint_validation_status(value))
                .unwrap_or(
                    if obj
                        .get("validation")
                        .and_then(Value::as_object)
                        .and_then(|validation| validation.get("status"))
                        .is_some()
                    {
                        "invalid"
                    } else {
                        "unknown"
                    },
                );
            out.insert(
                "validation_status".to_string(),
                Value::String(validation_status.to_string()),
            );
        }
        "workspace_checkpoint_list" => {
            copy_keys(obj, &mut out, &["limit"]);
        }
        "workspace_checkpoint_show" => {
            copy_keys(obj, &mut out, &["checkpoint_id", "include_diff_stat"]);
        }
        "workspace_checkpoint_restore" | "workspace_checkpoint_delete" => {
            copy_keys(obj, &mut out, &["checkpoint_id", "confirm"]);
        }
        _ => return arguments.clone(),
    }
    Value::Object(out)
}

fn copy_keys(
    obj: &serde_json::Map<String, Value>,
    out: &mut serde_json::Map<String, Value>,
    keys: &[&str],
) {
    for key in keys {
        if let Some(value) = obj.get(*key).cloned() {
            out.insert((*key).to_string(), value);
        }
    }
}

impl ToolCall {
    pub(crate) fn session_log_arguments(&self) -> Value {
        match self {
            Self::RunShell {
                project,
                timeout_secs,
                cwd,
                ..
            } => serde_json::json!({
                "project": project,
                "command_present": true,
                "timeout_secs": timeout_secs,
                "cwd": cwd,
            }),
            Self::RunJob {
                project,
                timeout_secs,
                cwd,
                ..
            } => serde_json::json!({
                "project": project,
                "command_present": true,
                "timeout_secs": timeout_secs,
                "cwd": cwd,
            }),
            Self::StopJob {
                project,
                job_id,
                confirm,
                ..
            } => serde_json::json!({
                "project": project,
                "job_id": job_id,
                "confirm": confirm,
            }),
            Self::ApplyPatch { project, .. } => serde_json::json!({
                "project": project,
                "patch_present": true,
            }),
            Self::ApplyPatchChecked {
                project,
                deny_sensitive_paths,
                ..
            }
            | Self::ValidatePatch {
                project,
                deny_sensitive_paths,
                ..
            } => serde_json::json!({
                "project": project,
                "patch_present": true,
                "deny_sensitive_paths": deny_sensitive_paths,
            }),
            Self::DeleteProjectFiles { project, paths, .. }
            | Self::GitRestorePaths { project, paths, .. }
            | Self::DiscardUntracked { project, paths, .. } => serde_json::json!({
                "project": project,
                "paths": paths,
            }),
            Self::GitStatus { project, .. } | Self::GitDiffSummary { project, .. } => {
                serde_json::json!({
                    "project": project,
                })
            }
            Self::GitLog {
                project,
                limit,
                skip,
                ..
            } => serde_json::json!({
                "project": project,
                "limit": limit,
                "skip": skip,
            }),
            Self::GitDiff { project, args, .. } => serde_json::json!({
                "project": project,
                "args_count": args.as_ref().map(Vec::len),
            }),
            Self::GitDiffHunks {
                project,
                paths,
                max_hunks,
                max_hunk_lines,
                cached,
                ..
            } => serde_json::json!({
                "project": project,
                "paths": paths,
                "max_hunks": max_hunks,
                "max_hunk_lines": max_hunk_lines,
                "cached": cached,
            }),
            Self::CargoFmt {
                project,
                cwd,
                check,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "check": check,
                "timeout_secs": timeout_secs,
            }),
            Self::CargoCheck {
                project,
                cwd,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "all_targets": all_targets,
                "all_features": all_features,
                "no_default_features": no_default_features,
                "features_present": features.as_ref().is_some_and(|v| !v.is_empty()),
                "package": package,
                "timeout_secs": timeout_secs,
            }),
            Self::CargoTest {
                project,
                cwd,
                filter,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                no_run,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "filter_present": filter.as_ref().is_some_and(|v| !v.is_empty()),
                "all_targets": all_targets,
                "all_features": all_features,
                "no_default_features": no_default_features,
                "features_present": features.as_ref().is_some_and(|v| !v.is_empty()),
                "package": package,
                "no_run": no_run,
                "timeout_secs": timeout_secs,
            }),
            Self::ReadFile {
                project,
                path,
                start_line,
                limit,
                with_line_numbers,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "start_line": start_line,
                "limit": limit,
                "with_line_numbers": with_line_numbers,
            }),
            Self::ListProjectFiles {
                project,
                path,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "limit": limit,
            }),
            Self::SearchProjectText {
                project,
                path,
                limit,
                context_before,
                context_after,
                ..
            } => serde_json::json!({
                "project": project,
                "pattern_present": true,
                "path": path,
                "limit": limit,
                "context_before": context_before,
                "context_after": context_after,
            }),
            Self::ShowChanges {
                project,
                include_diff,
                max_hunks,
                max_hunk_lines,
                session_event_limit,
                ..
            } => serde_json::json!({
                "project": project,
                "include_diff": include_diff,
                "max_hunks": max_hunks,
                "max_hunk_lines": max_hunk_lines,
                "session_event_limit": session_event_limit,
            }),
            Self::ReplaceInFile {
                project,
                path,
                expected_replacements,
                allow_multiple,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "old_present": true,
                "new_present": true,
                "expected_replacements": expected_replacements,
                "allow_multiple": allow_multiple,
            }),
            Self::ReplaceExactBlock {
                project,
                path,
                expected_old_sha256,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "old_text_present": true,
                "new_text_present": true,
                "expected_old_sha256_present": expected_old_sha256.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::InsertBeforePattern { project, path, .. }
            | Self::InsertAfterPattern { project, path, .. } => serde_json::json!({
                "project": project,
                "path": path,
                "pattern_present": true,
                "text_present": true,
            }),
            Self::WriteProjectFile {
                project,
                path,
                overwrite,
                expected_sha256,
                expected_content_prefix,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "content_present": true,
                "overwrite": overwrite,
                "expected_sha256_present": expected_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "expected_content_prefix_present": expected_content_prefix.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::SaveProjectArtifact {
                project,
                path,
                mime_type,
                overwrite,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "content_base64_present": true,
                "mime_type": mime_type,
                "overwrite": overwrite,
            }),
            Self::ReadProjectArtifactMetadata {
                project,
                path,
                allow_missing,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "allow_missing": allow_missing,
            }),
            Self::ReadProjectArtifact {
                project,
                path,
                encoding,
                offset,
                length,
                max_bytes,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "encoding": encoding,
                "offset": offset,
                "length": length,
                "max_bytes": max_bytes,
            }),
            Self::ArtifactUploadBegin {
                project,
                path,
                expected_bytes,
                expected_sha256,
                mime_type,
                overwrite,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "expected_bytes": expected_bytes,
                "expected_sha256_present": expected_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "mime_type": mime_type,
                "overwrite": overwrite,
            }),
            Self::ArtifactUploadChunk {
                project,
                path,
                upload_id,
                offset,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "upload_id": upload_id,
                "offset": offset,
                "content_base64_present": true,
            }),
            Self::ArtifactUploadFinish {
                project,
                path,
                upload_id,
                ..
            }
            | Self::ArtifactUploadAbort {
                project,
                path,
                upload_id,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "upload_id": upload_id,
            }),
            Self::ReplaceLineRange {
                project,
                path,
                start_line,
                end_line,
                expected_old_sha256,
                expected_old_prefix,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "start_line": start_line,
                "end_line": end_line,
                "new_text_present": true,
                "expected_old_sha256_present": expected_old_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "expected_old_prefix_present": expected_old_prefix.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::InsertAtLine {
                project,
                path,
                line,
                expected_anchor_sha256,
                expected_anchor_prefix,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "text_present": true,
                "expected_anchor_sha256_present": expected_anchor_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "expected_anchor_prefix_present": expected_anchor_prefix.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::DeleteLineRange {
                project,
                path,
                start_line,
                end_line,
                expected_old_sha256,
                expected_old_prefix,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "start_line": start_line,
                "end_line": end_line,
                "expected_old_sha256_present": expected_old_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "expected_old_prefix_present": expected_old_prefix.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::ApplyTextEdits {
                project,
                path,
                edits,
                dry_run,
                expected_file_sha256,
                ..
            } => {
                let kind_list: Vec<&str> = edits.iter().map(|e| e.kind.as_str()).collect();
                serde_json::json!({
                    "project": project,
                    "path": path,
                    "edit_count": edits.len(),
                    "kinds": kind_list,
                    "old_text_present": edits.iter().any(|e| e.old_text.as_ref().is_some_and(|v| !v.is_empty())),
                    "new_text_present": edits.iter().any(|e| e.new_text.as_ref().is_some_and(|v| !v.is_empty())),
                    "anchor_text_present": edits.iter().any(|e| e.anchor_text.as_ref().is_some_and(|v| !v.is_empty())),
                    "dry_run": dry_run,
                    "expected_file_sha256_present": expected_file_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                })
            }
            Self::WorkspaceCheckpointCreate {
                project,
                title,
                note,
                include_untracked,
                kind,
                labels,
                validation,
                ..
            } => {
                let kind = kind
                    .as_deref()
                    .filter(|value| is_checkpoint_kind(value))
                    .unwrap_or(if kind.is_some() {
                        "invalid"
                    } else {
                        "snapshot"
                    });
                let validation_status = validation
                    .as_ref()
                    .and_then(|value| value.status.as_deref())
                    .filter(|value| is_checkpoint_validation_status(value))
                    .unwrap_or(
                        if validation
                            .as_ref()
                            .and_then(|value| value.status.as_deref())
                            .is_some()
                        {
                            "invalid"
                        } else {
                            "unknown"
                        },
                    );
                serde_json::json!({
                    "project": project,
                    "title": title,
                    "note_present": note.as_ref().is_some_and(|v| !v.is_empty()),
                    "include_untracked": include_untracked,
                    "kind": kind,
                    "label_count": labels.len(),
                    "validation_status": validation_status,
                })
            }
            Self::WorkspaceCheckpointList { project, limit, .. } => serde_json::json!({
                "project": project,
                "limit": limit,
            }),
            Self::WorkspaceCheckpointShow {
                project,
                checkpoint_id,
                include_diff_stat,
                ..
            } => serde_json::json!({
                "project": project,
                "checkpoint_id": checkpoint_id,
                "include_diff_stat": include_diff_stat,
            }),
            Self::WorkspaceCheckpointRestore {
                project,
                checkpoint_id,
                confirm,
                ..
            } => serde_json::json!({
                "project": project,
                "checkpoint_id": checkpoint_id,
                "confirm": confirm,
            }),
            Self::WorkspaceCheckpointDelete {
                project,
                checkpoint_id,
                confirm,
                ..
            } => serde_json::json!({
                "project": project,
                "checkpoint_id": checkpoint_id,
                "confirm": confirm,
            }),
            Self::PostSessionMessage {
                session_id,
                kind,
                tags,
                reply_to,
                priority,
                ..
            } => serde_json::json!({
                "session_id": session_id,
                "kind": kind,
                "message_present": true,
                "tags_count": tags.len(),
                "reply_to": reply_to,
                "priority": priority,
            }),
            Self::ListSessionMessages {
                session_id,
                kind,
                status,
                limit,
            } => serde_json::json!({
                "session_id": session_id,
                "kind": kind,
                "status": status,
                "limit": limit,
            }),
            Self::ResolveSessionMessage {
                session_id,
                message_id,
                resolution,
            } => serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
                "resolution_present": resolution.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::SessionDiscussionSummary { session_id, limit } => serde_json::json!({
                "session_id": session_id,
                "limit": limit,
            }),
            Self::SessionHandoffSummary {
                session_id,
                project,
                include_workspace,
                include_checkpoints,
                include_validation,
                summary_only,
                limit,
            } => serde_json::json!({
                "session_id": session_id,
                "project": project,
                "include_workspace": include_workspace,
                "include_checkpoints": include_checkpoints,
                "include_validation": include_validation,
                "summary_only": summary_only,
                "limit": limit,
            }),
            Self::StartCodingTask {
                project,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
                include_runtime_status,
                compact_startup,
                include_git,
                include_recent_commits,
                include_rules,
                include_tool_manifest,
                tool_manifest_categories,
                tool_manifest_limit,
                bind_current,
            } => serde_json::json!({
                "project": project,
                "title": title,
                "mode": mode,
                "deny_write_tools": deny_write_tools,
                "deny_shell_tools": deny_shell_tools,
                "include_runtime_status": include_runtime_status,
                "compact_startup": compact_startup,
                "include_git": include_git,
                "include_recent_commits": include_recent_commits,
                "include_rules": include_rules,
                "include_tool_manifest": include_tool_manifest,
                "tool_manifest_categories": tool_manifest_categories,
                "tool_manifest_limit": tool_manifest_limit,
                "bind_current": bind_current,
            }),
            Self::FinishCodingTask {
                project,
                session_id,
                summary_only,
                include_diff,
                include_workspace,
                include_hygiene,
                include_handoff,
                include_validation_summary,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "summary_only": summary_only,
                "include_diff": include_diff,
                "include_workspace": include_workspace,
                "include_hygiene": include_hygiene,
                "include_handoff": include_handoff,
                "include_validation_summary": include_validation_summary,
            }),
            Self::ToolManifest {
                category,
                include_recommended_flows,
                include_risk_summary,
            } => serde_json::json!({
                "category": category,
                "include_recommended_flows": include_recommended_flows,
                "include_risk_summary": include_risk_summary,
            }),
            Self::ListTools {
                category,
                features,
                summary_only,
                limit,
            } => serde_json::json!({
                "category": category,
                "features": features,
                "summary_only": summary_only,
                "limit": limit,
            }),
            Self::RuntimeStatus {
                compact,
                summary_only,
            } => serde_json::json!({
                "compact": compact,
                "summary_only": summary_only,
            }),
            Self::WorkspaceHygieneCheck {
                project,
                max_findings,
                include_tracked,
                ..
            } => serde_json::json!({
                "project": project,
                "max_findings": max_findings,
                "include_tracked": include_tracked,
            }),
            _ => serde_json::json!({}),
        }
    }
}

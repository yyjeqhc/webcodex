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
        "run_process" => {
            out.insert(
                "executable_present".to_string(),
                Value::Bool(obj.contains_key("executable")),
            );
            out.insert(
                "stdin_present".to_string(),
                Value::Bool(obj.contains_key("stdin")),
            );
            let args = obj.get("args").and_then(Value::as_array);
            out.insert(
                "arg_count".to_string(),
                Value::from(args.map(Vec::len).unwrap_or_default()),
            );
            copy_keys(obj, &mut out, &["timeout_secs", "cwd", "purpose"]);
            if let Some(executable) = obj.get("executable").and_then(Value::as_str) {
                out.insert(
                    "process_summary".to_string(),
                    Value::String(crate::shell_client::process_preview(
                        executable,
                        args.into_iter().flatten().filter_map(Value::as_str),
                    )),
                );
            }
        }
        "run_script" => {
            if let Some(language) = obj.get("language").cloned() {
                out.insert("language".to_string(), language);
            }
            out.insert(
                "script_bytes".to_string(),
                Value::from(
                    obj.get("script")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "stdin_present".to_string(),
                Value::Bool(obj.get("stdin").is_some_and(|value| !value.is_null())),
            );
            out.insert(
                "arg_count".to_string(),
                Value::from(
                    obj.get("args")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or_default(),
                ),
            );
            copy_keys(obj, &mut out, &["timeout_secs", "cwd", "purpose"]);
        }
        "run_shell" | "run_job" | "session_shell_exec" => {
            out.insert(
                "command_present".to_string(),
                Value::Bool(obj.contains_key("command")),
            );
            copy_keys(obj, &mut out, &["timeout_secs", "cwd", "purpose", "shell"]);
            if tool_name == "session_shell_exec" {
                copy_keys(obj, &mut out, &["session_id", "shell_id"]);
            }
            if let Some(command) = obj.get("command").and_then(Value::as_str) {
                out.insert(
                    "command_summary".to_string(),
                    Value::String(crate::shell_client::command_preview(command)),
                );
            }
        }
        "open_session_shell" => {
            copy_keys(obj, &mut out, &["session_id", "cwd", "shell"]);
        }
        "session_shell_status" | "close_session_shell" => {
            copy_keys(obj, &mut out, &["session_id", "shell_id"]);
        }
        "computer_list_windows" => {
            copy_keys(obj, &mut out, &["client_id", "limit"]);
        }
        "computer_snapshot" => {
            copy_keys(obj, &mut out, &["client_id", "surface_id"]);
        }
        "observe_jobs" => {
            let items = obj.get("items").and_then(Value::as_array);
            out.insert(
                "item_count".to_string(),
                Value::from(items.map(Vec::len).unwrap_or_default()),
            );
            out.insert(
                "token_count".to_string(),
                Value::from(
                    items
                        .into_iter()
                        .flatten()
                        .filter(|item| item.get("after_observation_token").is_some())
                        .count(),
                ),
            );
            out.insert(
                "job_ids".to_string(),
                Value::Array(
                    items
                        .into_iter()
                        .flatten()
                        .filter_map(|item| item.get("job_id").and_then(Value::as_str))
                        .map(|job_id| Value::String(job_id.to_string()))
                        .collect(),
                ),
            );
            copy_keys(obj, &mut out, &["tail_lines", "wait_secs"]);
        }
        "start_session" | "start_coding_task" | "update_session_context" => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "project",
                    "client_id",
                    "title",
                    "mode",
                    "deny_write_tools",
                    "deny_shell_tools",
                    "detail",
                    "resume_session_id",
                    "bind_current",
                    "new_session",
                    "session_id",
                ],
            );
            if tool_name == "start_coding_task" {
                out.insert(
                    "path_source_requested".to_string(),
                    Value::Bool(obj.contains_key("path")),
                );
            }
            let context = obj
                .get("execution_context")
                .cloned()
                .and_then(|value| {
                    serde_json::from_value::<super::sessions::SessionExecutionContext>(value).ok()
                })
                .map(|context| context.audit_summary())
                .unwrap_or(Value::Null);
            out.insert("execution_context".to_string(), context);
        }
        "work_on_project" => {
            copy_keys(obj, &mut out, &["project", "client_id", "session_id"]);
            out.insert(
                "path_source_requested".to_string(),
                Value::Bool(obj.contains_key("path")),
            );
            if let Some(instruction) = obj.get("instruction").and_then(Value::as_str) {
                out.insert(
                    "instruction_summary".to_string(),
                    Value::String(crate::shell_client::command_preview(instruction)),
                );
                out.insert("instruction_present".to_string(), Value::Bool(true));
            }
        }
        "search_project_text" => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "path",
                    "limit",
                    "context_before",
                    "context_after",
                    "result_mode",
                    "timeout_secs",
                ],
            );
            out.insert(
                "pattern_present".to_string(),
                Value::Bool(obj.contains_key("pattern")),
            );
            for (field, summary_field) in [
                ("include_globs", "include_glob_count"),
                ("exclude_globs", "exclude_glob_count"),
            ] {
                let count = obj
                    .get(field)
                    .and_then(Value::as_array)
                    .map(|items| items.len())
                    .unwrap_or(0);
                out.insert(summary_field.to_string(), serde_json::json!(count));
            }
        }
        "search_project_texts" => {
            out.insert(
                "query_count".to_string(),
                serde_json::json!(obj
                    .get("queries")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0)),
            );
            out.insert(
                "patterns_present".to_string(),
                Value::Bool(
                    obj.get("queries")
                        .and_then(Value::as_array)
                        .is_some_and(|queries| {
                            queries.iter().any(|query| query.get("pattern").is_some())
                        }),
                ),
            );
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
        "delete_project_files" | "git_restore_paths" | "discard_untracked" => {
            copy_keys(obj, &mut out, &["paths"]);
        }
        "git_diff_hunks" => {
            copy_keys(
                obj,
                &mut out,
                &["paths", "max_hunks", "max_hunk_lines", "cached"],
            );
            out.insert(
                "continuation_present".to_string(),
                Value::Bool(
                    obj.get("continuation")
                        .is_some_and(|value| !value.is_null()),
                ),
            );
        }
        "go_test" => {
            copy_keys(obj, &mut out, &["cwd", "timeout_secs"]);
            let packages = obj.get("packages").and_then(Value::as_array);
            out.insert(
                "packages_present".to_string(),
                Value::Bool(obj.get("packages").is_some_and(|value| !value.is_null())),
            );
            out.insert(
                "package_count".to_string(),
                Value::from(packages.map(Vec::len).unwrap_or_default()),
            );
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

pub(crate) fn session_log_result_for_tool(tool_name: &str, output: &Value) -> Value {
    match tool_name {
        "computer_list_windows" => serde_json::json!({
            "count": output.get("count").cloned().unwrap_or(Value::Null),
            "truncated": output.get("truncated").cloned().unwrap_or(Value::Null),
        }),
        "computer_snapshot" => serde_json::json!({
            "surface_id": output.pointer("/surface/surface_id").cloned().unwrap_or(Value::Null),
            "width": output.get("width").cloned().unwrap_or(Value::Null),
            "height": output.get("height").cloned().unwrap_or(Value::Null),
            "mime_type": output.get("mime_type").cloned().unwrap_or(Value::Null),
            "file_bytes": output.get("file_bytes").cloned().unwrap_or(Value::Null),
        }),
        _ => output.clone(),
    }
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

#[cfg(test)]
mod computer_privacy_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn computer_list_ledger_result_omits_window_content() {
        let output = json!({
            "windows": [{
                "surface_id": "surface_secret",
                "application": "Private App",
                "title": "Confidential Window Title",
                "width": 1200,
                "height": 800,
                "focused": true,
                "active": true
            }],
            "count": 1,
            "truncated": false
        });
        let summary = session_log_result_for_tool("computer_list_windows", &output);
        let serialized = serde_json::to_string(&summary).unwrap();
        assert_eq!(summary, json!({"count": 1, "truncated": false}));
        assert!(!serialized.contains("Confidential"));
        assert!(!serialized.contains("Private App"));
        assert!(!serialized.contains("surface_secret"));
    }

    #[test]
    fn computer_snapshot_ledger_result_omits_image_and_titles() {
        let output = json!({
            "surface": {
                "surface_id": "surface_safe",
                "application": "Private App",
                "title": "Confidential Window Title",
                "width": 1200,
                "height": 800,
                "focused": null,
                "active": null
            },
            "width": 900,
            "height": 600,
            "mime_type": "image/jpeg",
            "file_bytes": 12345,
            "content_base64": "SUPER_SECRET_SCREENSHOT_BYTES"
        });
        let summary = session_log_result_for_tool("computer_snapshot", &output);
        let serialized = serde_json::to_string(&summary).unwrap();
        assert_eq!(summary["surface_id"], "surface_safe");
        assert_eq!(summary["width"], 900);
        assert_eq!(summary["height"], 600);
        assert_eq!(summary["file_bytes"], 12345);
        assert!(!serialized.contains("SUPER_SECRET"));
        assert!(!serialized.contains("Confidential"));
        assert!(!serialized.contains("Private App"));
    }
}

impl ToolCall {
    pub(crate) fn session_log_arguments(&self) -> Value {
        match self {
            Self::RunProcess {
                project,
                executable,
                args,
                stdin,
                timeout_secs,
                cwd,
                purpose,
                ..
            } => serde_json::json!({
                "project": project,
                "executable_present": true,
                "arg_count": args.len(),
                "stdin_present": stdin.is_some(),
                "process_summary": crate::shell_client::process_preview(
                    executable,
                    args.iter().map(String::as_str),
                ),
                "timeout_secs": timeout_secs,
                "cwd": cwd,
                "purpose": purpose,
            }),
            Self::RunScript {
                project,
                language,
                script,
                args,
                stdin,
                timeout_secs,
                cwd,
                purpose,
                ..
            } => serde_json::json!({
                "project": project,
                "language": language,
                "script_bytes": script.len(),
                "arg_count": args.len(),
                "stdin_present": stdin.is_some(),
                "timeout_secs": timeout_secs,
                "cwd": cwd,
                "purpose": purpose,
            }),
            Self::RunShell {
                project,
                command,
                timeout_secs,
                cwd,
                purpose,
                shell,
                ..
            } => serde_json::json!({
                "project": project,
                "command_present": true,
                "command_summary": crate::shell_client::command_preview(command),
                "timeout_secs": timeout_secs,
                "cwd": cwd,
                "purpose": purpose,
                "shell": shell,
            }),
            Self::RunJob {
                project,
                command,
                timeout_secs,
                cwd,
                purpose,
                shell,
                ..
            } => serde_json::json!({
                "project": project,
                "command_present": true,
                "command_summary": crate::shell_client::command_preview(command),
                "timeout_secs": timeout_secs,
                "cwd": cwd,
                "purpose": purpose,
                "shell": shell,
            }),
            Self::OpenSessionShell {
                project,
                session_id,
                cwd,
                shell,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "cwd": cwd,
                "shell": shell,
            }),
            Self::SessionShellExec {
                project,
                session_id,
                shell_id,
                command,
                timeout_secs,
                purpose,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "shell_id": shell_id,
                "command_present": true,
                "command_summary": crate::shell_client::command_preview(command),
                "timeout_secs": timeout_secs,
                "purpose": purpose,
            }),
            Self::SessionShellStatus {
                project,
                session_id,
                shell_id,
            }
            | Self::CloseSessionShell {
                project,
                session_id,
                shell_id,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "shell_id": shell_id,
            }),
            Self::ComputerListWindows { client_id, limit } => serde_json::json!({
                "client_id": client_id,
                "limit": limit,
            }),
            Self::ComputerSnapshot {
                client_id,
                surface_id,
            } => serde_json::json!({
                "client_id": client_id,
                "surface_id": surface_id,
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
            Self::ObserveJobs {
                items,
                tail_lines,
                wait_secs,
            } => serde_json::json!({
                "item_count": items.len(),
                "token_count": items
                    .iter()
                    .filter(|item| item.after_observation_token.is_some())
                    .count(),
                "job_ids": items
                    .iter()
                    .map(|item| item.job_id.as_str())
                    .collect::<Vec<_>>(),
                "tail_lines": tail_lines,
                "wait_secs": wait_secs,
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
            Self::GoTest {
                project,
                cwd,
                packages,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "packages_present": packages.is_some(),
                "package_count": packages.as_ref().map(Vec::len).unwrap_or_default(),
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
            Self::ReadFiles {
                project,
                items,
                with_line_numbers,
                ..
            } => serde_json::json!({
                "project": project,
                "items": items,
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
            Self::ProjectOverview {
                project,
                path,
                max_depth,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "max_depth": max_depth,
                "limit": limit,
            }),
            Self::SearchProjectText {
                project,
                path,
                limit,
                context_before,
                context_after,
                include_globs,
                exclude_globs,
                result_mode,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "pattern_present": true,
                "path": path,
                "limit": limit,
                "context_before": context_before,
                "context_after": context_after,
                "include_glob_count": include_globs.as_ref().map(Vec::len).unwrap_or(0),
                "exclude_glob_count": exclude_globs.as_ref().map(Vec::len).unwrap_or(0),
                "result_mode": result_mode,
                "timeout_secs": timeout_secs,
            }),
            Self::SearchProjectTexts {
                project, queries, ..
            } => serde_json::json!({
                "project": project,
                "query_count": queries.len(),
                "patterns_present": !queries.is_empty(),
            }),
            Self::LspStatus { project, .. } => serde_json::json!({
                "project": project,
            }),
            Self::DocumentSymbols {
                project,
                path,
                limit,
                ..
            }
            | Self::DocumentDiagnostics {
                project,
                path,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "limit": limit,
            }),
            Self::Hover {
                project,
                path,
                line,
                column,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "column": column,
            }),
            Self::WorkspaceSymbols { project, limit, .. } => serde_json::json!({
                "project": project,
                "query_present": true,
                "limit": limit,
            }),
            Self::GotoDefinition {
                project,
                path,
                line,
                column,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "column": column,
                "limit": limit,
            }),
            Self::FindReferences {
                project,
                path,
                line,
                column,
                include_declaration,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "column": column,
                "include_declaration": include_declaration,
                "limit": limit,
            }),
            Self::CallHierarchy {
                project,
                path,
                line,
                column,
                direction,
                depth,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "column": column,
                "direction": direction,
                "depth": depth,
                "limit": limit,
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
                as_image,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "encoding": encoding,
                "offset": offset,
                "length": length,
                "max_bytes": max_bytes,
                "as_image": as_image,
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
            Self::ApplyTextEdits {
                project,
                changes,
                dry_run,
                ..
            } => {
                let kind_list: Vec<&str> =
                    changes.iter().map(|change| change.kind.as_str()).collect();
                serde_json::json!({
                    "project": project,
                    "change_count": changes.len(),
                    "kinds": kind_list,
                    "paths": changes.iter().map(|change| change.path.as_str()).collect::<Vec<_>>(),
                    "destination_paths": changes.iter().filter_map(|change| change.to_path.as_deref()).collect::<Vec<_>>(),
                    "expected_sha256_count": changes.iter().filter(|change| change.expected_sha256.is_some()).count(),
                    "dry_run": dry_run,
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
            Self::StartSession {
                project,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
                execution_context,
            } => serde_json::json!({
                "project": project,
                "title": title,
                "mode": mode,
                "deny_write_tools": deny_write_tools,
                "deny_shell_tools": deny_shell_tools,
                "execution_context": execution_context
                    .as_ref()
                    .map(super::sessions::SessionExecutionContext::audit_summary),
            }),
            Self::StartCodingTask {
                project,
                client_id,
                path,
                temporary_project_name,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
                detail,
                resume_session_id,
                bind_current,
                new_session,
                execution_context,
            } => serde_json::json!({
                "project": project,
                "client_id": client_id,
                "path_source_requested": path.is_some(),
                "temporary_project_name": temporary_project_name,
                "title": title,
                "mode": mode,
                "deny_write_tools": deny_write_tools,
                "deny_shell_tools": deny_shell_tools,
                "detail": detail,
                "resume_session_id": resume_session_id,
                "bind_current": bind_current,
                "new_session": new_session,
                "execution_context": execution_context
                    .as_ref()
                    .map(super::sessions::SessionExecutionContext::audit_summary),
            }),
            Self::WorkOnProject {
                project,
                client_id,
                path,
                instruction,
                session_id,
            } => serde_json::json!({
                "project": project,
                "client_id": client_id,
                "path_source_requested": path.is_some(),
                "instruction_present": true,
                "instruction_summary": crate::shell_client::command_preview(instruction),
                "session_id": session_id,
            }),
            Self::UpdateSessionContext {
                project,
                session_id,
                execution_context,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "execution_context": execution_context.audit_summary(),
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
                intent,
                include_recommended_flows,
                include_risk_summary,
            } => serde_json::json!({
                "category": category,
                "intent": intent,
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

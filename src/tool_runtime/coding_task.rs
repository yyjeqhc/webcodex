//! Deterministic coding-task workflow aggregates.
//!
//! These tools reduce repetitive startup/finish calls for model-facing coding
//! loops. They only aggregate existing runtime state and never call an LLM,
//! generate prose summaries, parse validation output, or hide underlying tool
//! payloads.

use serde_json::{json, Value};

use super::project_instructions::{ProjectInstructionFile, ProjectInstructionsSnapshot};
use super::project_resolution::ResolvedProject;
use super::sessions::{self, SessionTransport};
use super::types::{SessionMode, ToolResult};
use super::validation_events::{skipped_validation_summary, validation_summary_for_session};
use super::ToolRuntime;
use super::{current_session_key, unknown_session_result};
use crate::auth::AuthContext;

const RULES_MAX_HEADINGS: usize = 8;
const RULES_MAX_FIRST_LINES: usize = 5;
const RULES_MAX_LINE_CHARS: usize = 180;
const FINISH_SESSION_EVENT_LIMIT: usize = 200;

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_coding_task(
        &self,
        project: String,
        title: Option<String>,
        mode: SessionMode,
        deny_write_tools: bool,
        deny_shell_tools: bool,
        include_runtime_status: Option<bool>,
        include_git: Option<bool>,
        include_recent_commits: Option<bool>,
        include_rules: Option<bool>,
        include_tool_manifest: Option<bool>,
        bind_current: bool,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> ToolResult {
        let include_runtime_status = include_runtime_status.unwrap_or(true);
        let include_git = include_git.unwrap_or(true);
        let include_recent_commits = include_recent_commits.unwrap_or(true);
        let include_rules = include_rules.unwrap_or(true);
        let include_tool_manifest = include_tool_manifest.unwrap_or(true);

        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let project_instructions = if include_rules {
            Some(self.load_project_instructions(&resolved.config).await)
        } else {
            None
        };
        let session_summary =
            self.sessions
                .start_session_with_options(sessions::SessionCreateOptions {
                    project: Some(resolved.resolved_id.clone()),
                    title,
                    mode,
                    guards: sessions::SessionGuards::effective(
                        mode,
                        sessions::SessionGuards {
                            deny_write_tools,
                            deny_shell_tools,
                        },
                    ),
                    project_instructions: project_instructions.clone(),
                });

        let mut warnings = Vec::new();
        let current_binding = if bind_current {
            match current_session_key(auth, transport, &resolved.resolved_id) {
                Ok(key) => match self
                    .sessions
                    .bind_current_session(key, &session_summary.session_id)
                {
                    Some(bound) => json!({
                        "bound": true,
                        "session_id": bound.session_id,
                        "process_local_in_memory": true,
                        "transport": transport.as_str(),
                        "resolved_project": resolved.resolved_id.clone(),
                    }),
                    None => {
                        warnings.push(json!({
                            "kind": "current_binding_failed",
                            "message": "new session could not be bound as current",
                        }));
                        json!({
                            "bound": false,
                            "process_local_in_memory": true,
                            "transport": transport.as_str(),
                        })
                    }
                },
                Err(message) => {
                    warnings.push(json!({
                        "kind": "current_binding_unavailable",
                        "message": message,
                    }));
                    json!({
                        "bound": false,
                        "process_local_in_memory": true,
                        "transport": transport.as_str(),
                        "error_kind": "current_session_unavailable",
                    })
                }
            }
        } else {
            json!({
                "bound": false,
                "process_local_in_memory": true,
                "transport": transport.as_str(),
            })
        };

        let runtime_status = if include_runtime_status {
            let result = self.runtime_status(auth).await;
            if !result.success {
                warnings.push(json!({
                    "kind": "runtime_status_unavailable",
                    "message": result.error,
                }));
            }
            result.output
        } else {
            Value::Null
        };

        let git = if include_git || include_recent_commits {
            self.start_coding_task_git_summary(
                &resolved.resolved_id,
                include_git,
                include_recent_commits,
                &mut warnings,
            )
            .await
        } else {
            Value::Null
        };
        let mut output = json!({
            "project": project,
            "resolved_project": resolved_project_payload(&resolved),
            "session": {
                "session_id": session_summary.session_id,
                "mode": session_summary.mode,
                "guards": session_summary.guards,
                "explicit_session_id_recommended": true,
                "explicit_session_id_fields": {
                    "tool_business_input": "session_id",
                    "generic_wrapper_recorder": "recording_session_id"
                },
                "current_binding": current_binding,
            },
            "runtime_status": runtime_status,
            "rules": rules_summary(project_instructions.as_ref()),
            "git": git,
            "recommended_flow": recommended_flow_payload(),
            "deterministic": true,
            "llm_summary": false,
            "warnings": warnings,
        });
        if include_tool_manifest {
            output["tool_manifest"] = self.compact_tool_manifest_payload();
        }
        ToolResult::ok(output)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finish_coding_task(
        &self,
        project: String,
        session_id: String,
        include_diff: Option<bool>,
        include_hygiene: Option<bool>,
        include_handoff: Option<bool>,
        include_validation_summary: Option<bool>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let include_diff = include_diff.unwrap_or(true);
        let include_hygiene = include_hygiene.unwrap_or(true);
        let include_handoff = include_handoff.unwrap_or(true);
        let include_validation_summary = include_validation_summary.unwrap_or(true);

        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let session_summary = match self
            .sessions
            .summary(&session_id, Some(FINISH_SESSION_EVENT_LIMIT))
        {
            Some(summary) => summary,
            None => return unknown_session_result(&session_id),
        };
        if let Some(session_project) = session_summary.project.as_deref() {
            if session_project != resolved.resolved_id {
                return ToolResult::err_with_output(
                    "session_project_mismatch",
                    json!({
                        "error_kind": "session_project_mismatch",
                        "session_id": session_id,
                        "session_project": session_project,
                        "project": project,
                        "resolved_project": resolved.resolved_id.clone(),
                    }),
                );
            }
        }

        let mut final_warnings = Vec::new();
        if session_summary.project.is_none() {
            final_warnings.push(json!({
                "kind": "session_has_no_project",
                "message": "session was not created with a project association",
            }));
        }

        let changes_result = self
            .show_changes(
                resolved.resolved_id.clone(),
                Some(session_id.clone()),
                Some(include_diff),
                None,
                None,
                Some(50),
            )
            .await;
        if !changes_result.success {
            final_warnings.push(json!({
                "kind": "show_changes_failed",
                "message": changes_result.error,
            }));
        }
        let workspace = workspace_payload_from_show_changes(&changes_result.output);
        append_workspace_warnings(&workspace, &mut final_warnings);

        let validation = if include_validation_summary {
            validation_summary_for_session(&session_summary)
        } else {
            skipped_validation_summary()
        };

        let hygiene = if include_hygiene {
            let result = self
                .workspace_hygiene_check(
                    resolved.resolved_id.clone(),
                    None,
                    None,
                    Some(session_id.clone()),
                )
                .await;
            if !result.success {
                final_warnings.push(json!({
                    "kind": "workspace_hygiene_failed",
                    "message": result.error,
                }));
            }
            result.output
        } else {
            Value::Null
        };
        append_hygiene_warnings(&hygiene, &mut final_warnings);

        let handoff = if include_handoff {
            let result = self
                .session_handoff_summary(
                    session_id.clone(),
                    Some(resolved.resolved_id.clone()),
                    Some(true),
                    Some(true),
                    Some(include_validation_summary),
                    Some(20),
                )
                .await;
            if !result.success {
                final_warnings.push(json!({
                    "kind": "session_handoff_failed",
                    "message": result.error,
                }));
            }
            result.output
        } else {
            Value::Null
        };

        ToolResult::ok(json!({
            "project": project,
            "resolved_project": resolved_project_payload(&resolved),
            "session_id": session_id,
            "workspace": workspace,
            "changes": {
                "show_changes": changes_result.output,
                "hunks_truncated": changes_result.output
                    .get("hunks_truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            "validation": validation,
            "hygiene": hygiene,
            "handoff": handoff,
            "deterministic": true,
            "llm_summary": false,
            "final_warnings": final_warnings,
        }))
    }

    async fn start_coding_task_git_summary(
        &self,
        project: &str,
        include_git: bool,
        include_recent_commits: bool,
        warnings: &mut Vec<Value>,
    ) -> Value {
        let mut output = json!({
            "available": false,
            "branch": Value::Null,
            "head": Value::Null,
            "clean": Value::Null,
            "changed_files_count": 0,
            "counts": {},
            "recent_commits": [],
            "warnings": [],
        });

        if include_git {
            let result = self
                .show_changes(project.to_string(), None, Some(false), None, None, None)
                .await;
            if !result.success {
                warnings.push(json!({
                    "kind": "git_status_unavailable",
                    "message": result.error,
                }));
            }
            output["available"] = json!(result
                .output
                .get("git_available")
                .and_then(Value::as_bool)
                .unwrap_or(result.success));
            output["branch"] = result.output.get("branch").cloned().unwrap_or(Value::Null);
            output["head"] = result.output.get("head").cloned().unwrap_or(Value::Null);
            output["clean"] = result.output.get("clean").cloned().unwrap_or(Value::Null);
            output["counts"] = result
                .output
                .get("counts")
                .cloned()
                .unwrap_or_else(|| json!({}));
            output["changed_files_count"] =
                json!(changed_files_count_from_counts(&output["counts"]));
            output["warnings"] = result
                .output
                .get("warnings")
                .cloned()
                .unwrap_or_else(|| json!([]));
            output["show_changes"] = result.output;
        }

        if include_recent_commits {
            let result = self.git_log(project.to_string(), Some(5), None).await;
            if result.success {
                output["recent_commits"] = result
                    .output
                    .get("commits")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                output["recent_commits_truncated"] = result
                    .output
                    .get("truncated")
                    .cloned()
                    .unwrap_or(json!(false));
            } else {
                warnings.push(json!({
                    "kind": "recent_commits_unavailable",
                    "message": result.error,
                }));
                output["recent_commits"] = json!([]);
                output["recent_commits_truncated"] = json!(false);
            }
        }

        output
    }
}

fn resolved_project_payload(resolved: &ResolvedProject) -> Value {
    json!({
        "input": resolved.input.clone(),
        "id": resolved.resolved_id.clone(),
        "path": resolved.config.path.clone(),
        "executor": if resolved.config.is_agent() { "agent" } else { "local" },
        "client_id": resolved.config.client_id.clone(),
        "allow_patch": resolved.config.allow_patch,
    })
}

fn rules_summary(snapshot: Option<&ProjectInstructionsSnapshot>) -> Value {
    let Some(snapshot) = snapshot else {
        return Value::Null;
    };
    let sources: Vec<Value> = snapshot.files.iter().map(rule_source_summary).collect();
    json!({
        "present": snapshot.loaded,
        "loaded": snapshot.loaded,
        "sources": sources,
        "candidate_paths": snapshot.candidate_paths.clone(),
        "total_chars": snapshot.total_chars,
        "max_total_chars": snapshot.max_total_chars,
        "truncated": snapshot.truncated,
        "summary": if snapshot.loaded {
            "deterministic instruction source summary; read listed sources for full content"
        } else {
            "no project instruction source loaded from the fixed candidate list"
        },
        "note": snapshot.note.clone(),
    })
}

fn rule_source_summary(file: &ProjectInstructionFile) -> Value {
    json!({
        "path": file.path.clone(),
        "chars": file.chars,
        "total_lines": file.total_lines,
        "start_line": file.start_line,
        "limit": file.limit,
        "truncated": file.truncated,
        "read_more": file.read_more.clone(),
        "headings": extract_headings(&file.content),
        "first_lines": extract_first_lines(&file.content),
    })
}

fn extract_headings(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .take(RULES_MAX_HEADINGS)
        .map(bound_line)
        .collect()
}

fn extract_first_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(RULES_MAX_FIRST_LINES)
        .map(bound_line)
        .collect()
}

fn bound_line(line: &str) -> String {
    let mut out = String::new();
    for ch in line.chars().take(RULES_MAX_LINE_CHARS) {
        out.push(ch);
    }
    out
}

fn recommended_flow_payload() -> Value {
    json!({
        "inspect": ["read_file", "search_project_text", "show_changes"],
        "edit": [
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
            "apply_text_edits",
            "apply_patch_checked"
        ],
        "validate": ["cargo_check", "cargo_test", "validate_patch"],
        "review": ["show_changes", "git_diff_hunks", "workspace_hygiene_check"],
        "handoff": ["session_summary", "session_handoff_summary"],
    })
}

fn workspace_payload_from_show_changes(show_changes: &Value) -> Value {
    let counts = show_changes
        .get("counts")
        .cloned()
        .unwrap_or_else(|| json!({}));
    json!({
        "clean": show_changes
            .get("clean")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "git_available": show_changes
            .get("git_available")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "non_git_project": show_changes
            .get("non_git_project")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "branch": show_changes.get("branch").cloned().unwrap_or(Value::Null),
        "head": show_changes.get("head").cloned().unwrap_or(Value::Null),
        "changed_files_count": changed_files_count_from_counts(&counts),
        "counts": counts,
        "warnings": show_changes
            .get("warnings")
            .cloned()
            .unwrap_or_else(|| json!([])),
    })
}

fn changed_files_count_from_counts(counts: &Value) -> u64 {
    [
        "modified",
        "added",
        "deleted",
        "renamed",
        "copied",
        "untracked",
    ]
    .iter()
    .map(|key| counts.get(*key).and_then(Value::as_u64).unwrap_or(0))
    .sum()
}

fn append_workspace_warnings(workspace: &Value, warnings: &mut Vec<Value>) {
    if !workspace
        .get("clean")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        warnings.push(json!({
            "kind": "dirty_worktree",
            "changed_files_count": workspace
                .get("changed_files_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "message": "workspace has tracked or untracked changes",
        }));
    }
    if !workspace
        .get("git_available")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        warnings.push(json!({
            "kind": "git_unavailable",
            "message": "git-backed workspace inspection unavailable",
        }));
    }
}

fn append_hygiene_warnings(hygiene: &Value, warnings: &mut Vec<Value>) {
    let finding_count = hygiene
        .get("counts")
        .and_then(|counts| counts.get("findings"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if finding_count > 0 {
        warnings.push(json!({
            "kind": "workspace_hygiene_findings",
            "findings": finding_count,
            "message": "workspace hygiene findings should be reviewed",
        }));
    }
}

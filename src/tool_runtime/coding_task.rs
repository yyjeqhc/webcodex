//! Deterministic coding-task workflow aggregates.
//!
//! These tools reduce repetitive startup/finish calls for model-facing coding
//! loops. They only aggregate existing runtime state and never call an LLM,
//! generate prose summaries, parse validation output, or hide underlying tool
//! payloads.

use serde_json::{json, Value};

use super::handoff::{
    compact_jobs, compact_permissions, compact_review_evidence, compact_tool_failures,
    compact_validation, compact_workflow_verdict, review_evidence_summary_for_session,
};
use super::permissions::{permission_profile_payload, permission_summary_from_events};
use super::project_instructions::{ProjectInstructionFile, ProjectInstructionsSnapshot};
use super::project_resolution::ResolvedProject;
use super::runtime_info::compact_runtime_status;
use super::session_context::{
    session_project_mismatch_warning, SessionProjectMismatch, SESSION_PROJECT_MISMATCH_KIND,
};
use super::sessions::tool_failure_summary_from_events;
use super::sessions::{self, SessionTransport, TOOL_CALL_RECORDING_SESSION_ID_FIELD};
use super::tool_inputs::SessionMode;
use super::tool_result::ToolResult;
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
        compact_startup: bool,
        include_git: Option<bool>,
        include_recent_commits: Option<bool>,
        include_rules: Option<bool>,
        include_tool_manifest: Option<bool>,
        tool_manifest_categories: Option<Vec<String>>,
        tool_manifest_limit: Option<usize>,
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

        let mut runtime_status_call_failed = false;
        let runtime_status = if include_runtime_status {
            let result = self.runtime_status(auth).await;
            if !result.success {
                runtime_status_call_failed = true;
                warnings.push(json!({
                    "kind": "runtime_status_unavailable",
                    "message": result.error,
                }));
            }
            if compact_startup {
                compact_runtime_status(&result.output)
            } else {
                result.output
            }
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
                    "generic_wrapper_recorder": TOOL_CALL_RECORDING_SESSION_ID_FIELD
                },
                "current_binding": current_binding,
            },
            "runtime_status": runtime_status,
            "permissions": permission_profile_payload(),
            "rules": rules_summary(project_instructions.as_ref()),
            "git": git,
            "recommended_flow": recommended_flow_payload(),
            "deterministic": true,
            "llm_summary": false,
            "warnings": warnings,
        });
        if include_tool_manifest {
            output["tool_manifest"] = self.compact_tool_manifest_payload_bounded(
                tool_manifest_categories,
                tool_manifest_limit,
            );
        }
        output["startup_verdict"] = startup_verdict(
            &output,
            include_runtime_status,
            runtime_status_call_failed,
            include_git,
            include_tool_manifest,
        );
        ToolResult::ok(output)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finish_coding_task(
        &self,
        project: String,
        session_id: String,
        summary_only: bool,
        include_diff: Option<bool>,
        include_workspace: Option<bool>,
        include_hygiene: Option<bool>,
        include_handoff: Option<bool>,
        include_validation_summary: Option<bool>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let include_diff = include_diff.unwrap_or(true);
        let include_workspace = include_workspace.unwrap_or(true);
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
        let mut final_warnings = Vec::new();
        let session_project_mismatch =
            session_summary
                .project
                .as_ref()
                .and_then(|session_project| {
                    (session_project != &resolved.resolved_id).then(|| SessionProjectMismatch {
                        session_project: session_project.clone(),
                        request_project: resolved.resolved_id.clone(),
                    })
                });
        if let Some(mismatch) = session_project_mismatch.as_ref() {
            final_warnings.push(session_project_mismatch_warning(mismatch, false));
        }
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
        let permissions = permission_summary_from_events(
            &session_summary.events,
            super::permissions::DEFAULT_PERMISSION_RECENT_LIMIT,
        );

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

        let jobs = self
            .active_jobs_summary(Some(&resolved.resolved_id), auth, 10)
            .await;
        if let Some(warnings) = jobs.get("warnings").and_then(Value::as_array) {
            final_warnings.extend(warnings.iter().cloned());
        }

        let handoff = if include_handoff {
            let result = self
                .session_handoff_summary(
                    session_id.clone(),
                    Some(resolved.resolved_id.clone()),
                    Some(include_workspace),
                    Some(true),
                    Some(include_validation_summary),
                    summary_only,
                    Some(20),
                    auth,
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

        let mut output = json!({
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
            "permissions": permissions,
            "tool_failures": tool_failure_summary_from_events(&session_summary.events, 10),
            "review_evidence": review_evidence_summary_for_session(&session_summary),
            "hygiene": hygiene,
            "handoff": handoff,
            "jobs": jobs,
            "deterministic": true,
            "llm_summary": false,
            "final_warnings": final_warnings,
        });
        if let Some(mismatch) = session_project_mismatch.as_ref() {
            output["warning_kind"] = json!(SESSION_PROJECT_MISMATCH_KIND);
            output["session_project"] = json!(mismatch.session_project);
            output["request_project"] = json!(mismatch.request_project);
            output["allow_cross_project_session_required"] = json!(true);
            output["allow_cross_project_session"] = json!(false);
        }
        output["suggested_next_actions"] = json!(finish_suggested_next_actions(&output));
        if summary_only {
            return ToolResult::ok(compact_finish_output(&output));
        }
        ToolResult::ok(output)
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

fn compact_finish_output(output: &Value) -> Value {
    let hygiene_checked = output
        .get("hygiene")
        .is_some_and(|hygiene| !hygiene.is_null());
    let workspace_clean = output
        .get("workspace")
        .and_then(|workspace| workspace.get("clean"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let hygiene_clean = output
        .get("hygiene")
        .and_then(|hygiene| hygiene.get("clean"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut compact = json!({
        "summary_only": true,
        "project": output.get("project").cloned().unwrap_or(Value::Null),
        "session_id": output.get("session_id").cloned().unwrap_or(Value::Null),
        "workspace_clean": workspace_clean,
        "hygiene_clean": hygiene_clean,
        "jobs": compact_jobs(output.get("jobs").unwrap_or(&Value::Null)),
        "permissions": compact_permissions(output.get("permissions").unwrap_or(&Value::Null)),
        "tool_failures": compact_tool_failures(output.get("tool_failures").unwrap_or(&Value::Null)),
        "validation": compact_validation(output.get("validation").unwrap_or(&Value::Null)),
        "review_evidence": compact_review_evidence(output.get("review_evidence").unwrap_or(&Value::Null)),
        "warnings": output.get("final_warnings").cloned().unwrap_or_else(|| json!([])),
        "suggested_next_actions": output.get("suggested_next_actions").cloned().unwrap_or_else(|| json!([])),
    });
    compact["verdict"] = compact_workflow_verdict(&compact, true, Some(hygiene_checked));
    compact
}

fn startup_verdict(
    output: &Value,
    runtime_status_requested: bool,
    runtime_status_call_failed: bool,
    git_requested: bool,
    tool_manifest_requested: bool,
) -> Value {
    let mut checks = Vec::new();
    let mut actions: Vec<String> = Vec::new();

    push_startup_check(
        &mut checks,
        "runtime_status",
        runtime_status_check(output, runtime_status_requested, runtime_status_call_failed),
    );
    push_startup_check(
        &mut checks,
        "workspace",
        workspace_check(output, git_requested),
    );
    push_startup_check(
        &mut checks,
        "jobs",
        startup_jobs_check(output, runtime_status_requested),
    );
    push_startup_check(
        &mut checks,
        "agent",
        startup_agent_check(output, runtime_status_requested),
    );
    push_startup_check(
        &mut checks,
        "tool_manifest",
        startup_tool_manifest_check(output, tool_manifest_requested),
    );

    for check in &checks {
        match check.get("reason").and_then(Value::as_str) {
            Some("runtime_status_not_requested") => push_unique_action(
                &mut actions,
                "rerun startup with include_runtime_status=true and compact_startup=true for sanity",
            ),
            Some("runtime_status_call_failed") => {
                push_unique_action(&mut actions, "inspect runtime_status directly")
            }
            Some("workspace_not_checked") => {
                push_unique_action(&mut actions, "run show_changes before editing or finishing")
            }
            Some("workspace_dirty") => {
                push_unique_action(&mut actions, "review workspace changes with show_changes")
            }
            Some("active_jobs_present") | Some("blocking_active_jobs") => {
                push_unique_action(&mut actions, "inspect active jobs before proceeding")
            }
            Some("agent_offline") => {
                push_unique_action(&mut actions, "check agent connectivity with list_agents")
            }
            Some("tool_manifest_not_requested") => push_unique_action(
                &mut actions,
                "request tool_manifest if workflow discovery is needed",
            ),
            Some("truncated_by_limit") => push_unique_action(
                &mut actions,
                "continue with the bounded tool_manifest or request a focused category",
            ),
            Some("tool_manifest_unavailable") => {
                push_unique_action(&mut actions, "inspect tool_manifest directly")
            }
            _ => {}
        }
    }

    if actions.is_empty() {
        actions.push("proceed with the coding task using the explicit session_id".to_string());
    }
    let status = aggregate_startup_status(&checks);
    json!({
        "status": status,
        "blocking": status == "fail",
        "checks": checks,
        "suggested_next_actions": actions,
    })
}

fn runtime_status_check(
    output: &Value,
    runtime_status_requested: bool,
    runtime_status_call_failed: bool,
) -> (&'static str, Option<&'static str>) {
    if !runtime_status_requested {
        return ("warn", Some("runtime_status_not_requested"));
    }
    if runtime_status_call_failed {
        return ("fail", Some("runtime_status_call_failed"));
    }
    let runtime_status = output.get("runtime_status").unwrap_or(&Value::Null);
    if !runtime_status.is_object() {
        return ("fail", Some("runtime_status_unavailable"));
    }
    match runtime_status
        .pointer("/tools/count")
        .and_then(Value::as_u64)
    {
        Some(count) if count > 0 => ("pass", None),
        Some(_) => ("fail", Some("tool_count_zero")),
        None => ("warn", Some("tool_count_unknown")),
    }
}

fn workspace_check(output: &Value, git_requested: bool) -> (&'static str, Option<&'static str>) {
    if !git_requested {
        return ("warn", Some("workspace_not_checked"));
    }
    let git = output.get("git").unwrap_or(&Value::Null);
    if git.get("available").and_then(Value::as_bool) == Some(false) {
        return ("warn", Some("git_unavailable"));
    }
    match git.get("clean").and_then(Value::as_bool) {
        Some(true) => ("pass", None),
        Some(false) => ("fail", Some("workspace_dirty")),
        None => ("warn", Some("workspace_unknown")),
    }
}

fn startup_jobs_check(
    output: &Value,
    runtime_status_requested: bool,
) -> (&'static str, Option<&'static str>) {
    if !runtime_status_requested {
        return ("warn", Some("runtime_status_not_requested"));
    }
    let jobs = output
        .pointer("/runtime_status/jobs")
        .unwrap_or(&Value::Null);
    if jobs
        .get("blocking_active_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        return ("fail", Some("blocking_active_jobs"));
    }
    match jobs.get("active_count").and_then(Value::as_u64) {
        Some(0) => ("pass", None),
        Some(_) => ("warn", Some("active_jobs_present")),
        None => ("warn", Some("jobs_unknown")),
    }
}

fn startup_agent_check(
    output: &Value,
    runtime_status_requested: bool,
) -> (&'static str, Option<&'static str>) {
    if !runtime_status_requested {
        return ("warn", Some("runtime_status_not_requested"));
    }
    let executor = output
        .pointer("/resolved_project/executor")
        .and_then(Value::as_str);
    let online = output
        .pointer("/runtime_status/agents/summary/online")
        .or_else(|| output.pointer("/runtime_status/agents/online_count"))
        .and_then(Value::as_u64);
    match (executor, online) {
        (Some("agent"), Some(0)) => ("fail", Some("agent_offline")),
        (Some("agent"), Some(_)) => ("pass", None),
        (Some("local"), _) => ("pass", None),
        (_, Some(_)) => ("pass", None),
        _ => ("warn", Some("agent_health_unknown")),
    }
}

fn startup_tool_manifest_check(
    output: &Value,
    tool_manifest_requested: bool,
) -> (&'static str, Option<&'static str>) {
    if !tool_manifest_requested {
        return ("warn", Some("tool_manifest_not_requested"));
    }
    let Some(manifest) = output.get("tool_manifest") else {
        return ("fail", Some("tool_manifest_unavailable"));
    };
    if !manifest.is_object() {
        return ("fail", Some("tool_manifest_unavailable"));
    }
    if manifest
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        if manifest.get("truncation_reason").and_then(Value::as_str) == Some("limit") {
            return ("warn", Some("truncated_by_limit"));
        }
        return ("warn", Some("tool_manifest_truncated"));
    }
    ("pass", None)
}

fn push_startup_check(
    checks: &mut Vec<Value>,
    name: &'static str,
    (status, reason): (&'static str, Option<&'static str>),
) {
    let mut check = json!({
        "name": name,
        "status": status,
    });
    if let Some(reason) = reason {
        check["reason"] = json!(reason);
    }
    checks.push(check);
}

fn aggregate_startup_status(checks: &[Value]) -> &'static str {
    if checks
        .iter()
        .any(|check| check.get("status").and_then(Value::as_str) == Some("fail"))
    {
        "fail"
    } else if checks
        .iter()
        .any(|check| check.get("status").and_then(Value::as_str) == Some("warn"))
    {
        "warn"
    } else {
        "pass"
    }
}

fn push_unique_action(actions: &mut Vec<String>, action: &str) {
    if !actions.iter().any(|existing| existing == action) {
        actions.push(action.to_string());
    }
}

fn finish_suggested_next_actions(output: &Value) -> Vec<String> {
    let mut actions = Vec::new();
    let push = |actions: &mut Vec<String>, action: &str| {
        if !actions.iter().any(|existing| existing == action) {
            actions.push(action.to_string());
        }
    };
    let tool_failures = output.get("tool_failures").unwrap_or(&Value::Null);
    let unexpected_count = tool_failures
        .get("unexpected_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let expectation_mismatch_count = tool_failures
        .get("expectation_mismatch_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unexpected_success_count = tool_failures
        .get("unexpected_success_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let expected_count = tool_failures
        .get("expected_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    if unexpected_count > 0 {
        push(
            &mut actions,
            "review unexpected failed tool calls before proceeding",
        );
    }
    if expectation_mismatch_count > 0 {
        push(
            &mut actions,
            "review expected failure mismatches before proceeding",
        );
    }
    if unexpected_success_count > 0 {
        push(
            &mut actions,
            "review expected-failure assertions that unexpectedly succeeded",
        );
    }
    if expected_count > 0
        && unexpected_count == 0
        && expectation_mismatch_count == 0
        && unexpected_success_count == 0
    {
        push(&mut actions, "expected failure assertions matched");
    }
    if output
        .get("workspace")
        .and_then(|workspace| workspace.get("clean"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        push(&mut actions, "review workspace changes with show_changes");
    }
    if output
        .get("jobs")
        .and_then(|jobs| jobs.get("blocking_active_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        push(&mut actions, "stop or await blocking active jobs");
    }
    actions
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

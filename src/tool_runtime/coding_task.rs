//! Deterministic coding-task workflow aggregates.
//!
//! These tools reduce repetitive startup/finish calls for model-facing coding
//! loops. They only aggregate existing runtime state and never call an LLM,
//! generate prose summaries, parse validation output, or hide underlying tool
//! payloads.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::continuation_feedback::{
    continuation_feedback_value, not_applicable_continuation_feedback_value,
    ContinuationFeedbackInput,
};
use super::handoff::{
    apply_compact_workflow_outcomes, closeout_work_projection, compact_jobs, compact_permissions,
    compact_review_evidence, compact_tool_failures, compact_validation,
    resolved_unexpected_validation_failure_count, review_evidence_summary_for_session,
    unresolved_unexpected_failure_count, validation_has_cargo_test_zero_tests,
};
use super::handoff_brief::{build_handoff_brief, HandoffBriefInput};
use super::permissions::{
    authority_profile_payload, permission_summary_from_events, PermissionDecision,
};
use super::project_instructions::{ProjectInstructionFile, ProjectInstructionsSnapshot};
use super::project_resolution::ResolvedProject;
use super::runtime_info::compact_runtime_status;
use super::session_context::{
    session_project_mismatch_warning, SessionProjectMismatch, SESSION_PROJECT_MISMATCH_KIND,
};
use super::sessions::tool_failure_summary_from_events;
use super::sessions::{self, SessionTransport, TOOL_CALL_RECORDING_SESSION_ID_FIELD};
use super::startup_brief::{
    build_startup_brief, builtin_coding_workflow_projection, startup_brief_from_output,
    StartupBriefInput, REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON,
};
use super::tool_catalog::TOOL_RECOMMENDED_FLOWS;
use super::tool_inputs::{SessionMode, StartupDetail};
use super::tool_result::ToolResult;
use super::validation_events::skipped_validation_summary;
use super::{current_session_key, unknown_session_result};
use super::{ToolCall, ToolRuntime};
use crate::auth::AuthContext;
use crate::shell_protocol::{
    ShellFileOpRequest, SHELL_CLIENT_CAPABILITY_FILE_READ, SHELL_CLIENT_CAPABILITY_GIT,
    SHELL_CLIENT_CAPABILITY_SHELL,
};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

const RULES_MAX_HEADINGS: usize = 8;
const RULES_MAX_FIRST_LINES: usize = 5;
const RULES_MAX_LINE_CHARS: usize = 180;
const FINISH_SESSION_EVENT_LIMIT: usize = 200;
/// Short startup probe budget for the repository overview, much tighter than
/// the standalone `project_overview` tool's 30s wait. An optional overview
/// failure must not block the coding task, so it fails over quickly.
pub(crate) const DEFAULT_REPOSITORY_OVERVIEW_PROBE_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ProjectResolutionMetadata {
    pub(crate) source: String,
    pub(crate) outcome: String,
    pub(crate) resolved_project: String,
    pub(crate) registered: bool,
    #[serde(skip)]
    pub(crate) permission: Option<PermissionDecision>,
}

enum CodingProjectSource {
    Existing {
        project: String,
    },
    RunnerPath {
        client_id: String,
        path: String,
    },
    ManagedTemporary {
        client_id: String,
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Copy)]
struct CodingStartupOptions {
    detail: StartupDetail,
    include_repository_overview: bool,
}

impl CodingStartupOptions {
    fn start_coding_task(detail: StartupDetail) -> Self {
        Self {
            detail,
            include_repository_overview: true,
        }
    }

    fn work_on_project() -> Self {
        Self {
            detail: StartupDetail::Standard,
            include_repository_overview: false,
        }
    }
}

fn invalid_project_source(message: impl Into<String>, fields: Value) -> ToolResult {
    let mut output = json!({
        "error_kind": "invalid_arguments",
        "failure_kind": "invalid_arguments",
        "constraint": "exactly_one_project_source",
        "state_changed": false,
    });
    if let (Some(output), Some(fields)) = (output.as_object_mut(), fields.as_object()) {
        output.extend(fields.clone());
    }
    ToolResult::err_with_output(message, output)
}

fn non_empty_optional_field(
    field: &'static str,
    value: Option<String>,
) -> Result<Option<String>, ToolResult> {
    match value {
        Some(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                Err(invalid_project_source(
                    format!("{field} must not be empty"),
                    json!({"field": field}),
                ))
            } else {
                Ok(Some(trimmed))
            }
        }
        None => Ok(None),
    }
}

fn resolve_project_source(
    project: String,
    client_id: Option<String>,
    path: Option<String>,
    temporary_project_name: Option<String>,
    managed_temporary_allowed: bool,
) -> Result<CodingProjectSource, ToolResult> {
    let project = project.trim().to_string();
    let client_id = non_empty_optional_field("client_id", client_id)?;
    let path = non_empty_optional_field("path", path)?;
    let temporary_project_name =
        non_empty_optional_field("temporary_project_name", temporary_project_name)?;

    if !project.is_empty() {
        let mut conflicts = Vec::new();
        if client_id.is_some() {
            conflicts.push("client_id");
        }
        if path.is_some() {
            conflicts.push("path");
        }
        if temporary_project_name.is_some() {
            conflicts.push("temporary_project_name");
        }
        if !conflicts.is_empty() {
            let mut fields = vec!["project"];
            fields.extend(conflicts);
            return Err(invalid_project_source(
                "project cannot be combined with client_id, path, or temporary_project_name",
                json!({"conflicting_fields": fields}),
            ));
        }
        return Ok(CodingProjectSource::Existing { project });
    }

    if let Some(path) = path {
        if temporary_project_name.is_some() {
            return Err(invalid_project_source(
                "path cannot be combined with temporary_project_name",
                json!({"conflicting_fields": ["path", "temporary_project_name"]}),
            ));
        }
        if !Path::new(&path).is_absolute() {
            return Err(invalid_project_source(
                "path must be an absolute path",
                json!({"field": "path", "expected": "absolute_path"}),
            ));
        }
        let Some(client_id) = client_id else {
            return Err(invalid_project_source(
                "path requires client_id",
                json!({"field": "client_id", "required_with": "path"}),
            ));
        };
        return Ok(CodingProjectSource::RunnerPath { client_id, path });
    }

    if !managed_temporary_allowed {
        return Err(if client_id.is_some() {
            invalid_project_source(
                "client_id requires path",
                json!({"field": "path", "required_with": "client_id"}),
            )
        } else {
            invalid_project_source(
                "project or client_id + path is required",
                json!({"required_any_of": ["project", "client_id + path"]}),
            )
        });
    }
    let Some(client_id) = client_id else {
        return Err(if temporary_project_name.is_some() {
            invalid_project_source(
                "temporary_project_name requires client_id",
                json!({"field": "client_id", "required_with": "temporary_project_name"}),
            )
        } else {
            invalid_project_source(
                "start_coding_task requires project, client_id + path, or client_id for a managed temporary project",
                json!({"required_any_of": ["project", "client_id + path", "client_id"]}),
            )
        });
    };
    Ok(CodingProjectSource::ManagedTemporary {
        client_id,
        name: temporary_project_name,
    })
}

fn registration_scope_denied(auth: Option<&AuthContext>, operation: &str) -> Option<ToolResult> {
    auth.is_some_and(|auth| {
        auth.is_oauth_token() && !auth.has_scope(crate::auth::SCOPE_PROJECT_WRITE)
    })
    .then(|| {
        ToolResult::err_with_output(
            format!("{operation} requires project:write"),
            json!({
                "error_kind": "insufficient_scope",
                "failure_kind": "insufficient_scope",
                "required_scope": crate::auth::SCOPE_PROJECT_WRITE,
                "state_changed": false,
            }),
        )
    })
}

fn attach_permission(
    mut result: ToolResult,
    permission: Option<&PermissionDecision>,
) -> ToolResult {
    if let Some(permission) = permission {
        super::permissions::add_permission_to_result(&mut result, permission);
    }
    result
}

fn attach_project_resolution(
    mut result: ToolResult,
    resolution: &ProjectResolutionMetadata,
) -> ToolResult {
    // Existing-project aliases are not authoritative until runtime resolution
    // succeeds. Path and managed-temporary sources already carry a Runner-issued
    // full id, so their metadata remains useful on later Session failures.
    if !resolution.resolved_project.is_empty() {
        result.output["project_resolution"] =
            serde_json::to_value(resolution).unwrap_or_else(|_| json!({}));
    }
    if resolution.registered && !result.success {
        result.output["state_changed"] = json!(true);
    }
    attach_permission(result, resolution.permission.as_ref())
}

impl ToolRuntime {
    async fn require_runner_coding_capability(
        &self,
        client_id: &str,
        auth: Option<&AuthContext>,
    ) -> Result<(), ToolResult> {
        let supports_shell = self
            .shell_clients
            .client_supports_for_auth(client_id, SHELL_CLIENT_CAPABILITY_SHELL, auth)
            .await
            .map_err(ToolResult::err)?;
        let supports_git = if supports_shell {
            false
        } else {
            self.shell_clients
                .client_supports_for_auth(client_id, SHELL_CLIENT_CAPABILITY_GIT, auth)
                .await
                .map_err(ToolResult::err)?
        };
        if supports_shell || supports_git {
            Ok(())
        } else {
            Err(ToolResult::err(format!(
                "agent client {client_id} does not support shell or git"
            )))
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_coding_task(
        &self,
        project: String,
        client_id: Option<String>,
        path: Option<String>,
        temporary_project_name: Option<String>,
        title: Option<String>,
        mode: SessionMode,
        deny_write_tools: bool,
        deny_shell_tools: bool,
        detail: StartupDetail,
        resume_session_id: Option<String>,
        bind_current: bool,
        new_session: bool,
        execution_context: Option<sessions::SessionExecutionContext>,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        self.start_coding_task_with_options(
            project,
            client_id,
            path,
            temporary_project_name,
            title,
            mode,
            deny_write_tools,
            deny_shell_tools,
            CodingStartupOptions::start_coding_task(detail),
            resume_session_id,
            bind_current,
            new_session,
            execution_context,
            auth,
            transport,
            window,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_coding_task_with_options(
        &self,
        project: String,
        client_id: Option<String>,
        path: Option<String>,
        temporary_project_name: Option<String>,
        title: Option<String>,
        mode: SessionMode,
        deny_write_tools: bool,
        deny_shell_tools: bool,
        startup: CodingStartupOptions,
        resume_session_id: Option<String>,
        bind_current: bool,
        new_session: bool,
        execution_context: Option<sessions::SessionExecutionContext>,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        let detail = startup.detail;
        let project_source =
            match resolve_project_source(project, client_id, path, temporary_project_name, true) {
                Ok(source) => source,
                Err(result) => return result,
            };
        let resume_requested = resume_session_id.is_some();
        if resume_requested && new_session {
            return ToolResult::err_with_output(
                "resume_session_id and new_session=true are mutually exclusive",
                json!({
                    "error_kind": "invalid_arguments",
                    "failure_kind": "invalid_arguments",
                    "conflicting_fields": ["resume_session_id", "new_session"],
                    "constraint": "resume_session_id_mutually_exclusive_with_new_session",
                    "state_changed": false,
                }),
            );
        }
        let execution_context = match execution_context
            .map(sessions::SessionExecutionContext::validated)
            .transpose()
        {
            Ok(context) => context,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({
                        "error_kind": "invalid_execution_context",
                        "failure_kind": "invalid_arguments",
                        "field": "execution_context",
                        "state_changed": false,
                    }),
                );
            }
        };
        let resume_session_id = match resume_session_id {
            Some(session_id)
                if session_id != session_id.trim()
                    || !sessions::is_valid_session_id(&session_id) =>
            {
                return ToolResult::err_with_output(
                    "resume_session_id must be a valid wc_sess_* Workflow Session id",
                    json!({
                        "error_kind": "invalid_resume_session_id",
                        "failure_kind": "invalid_arguments",
                        "field": "resume_session_id",
                        "expected_format": "wc_sess_*",
                        "state_changed": false,
                    }),
                );
            }
            Some(session_id) => Some(session_id),
            None => None,
        };
        let title = match title {
            Some(title) => {
                let title = title.trim().to_string();
                if title.is_empty()
                    || title.chars().count() > sessions::MAX_CODING_INSTRUCTION_CHARS
                {
                    return ToolResult::err_with_output(
                        format!(
                            "title must contain 1..={} characters",
                            sessions::MAX_CODING_INSTRUCTION_CHARS
                        ),
                        json!({
                            "error_kind": "invalid_coding_instruction",
                            "field": "title",
                            "max_chars": sessions::MAX_CODING_INSTRUCTION_CHARS,
                        }),
                    );
                }
                Some(title)
            }
            None => None,
        };
        let (project, mut project_resolution) = match project_source {
            CodingProjectSource::Existing { project } => {
                let resolution = ProjectResolutionMetadata {
                    source: "project".to_string(),
                    outcome: "resolved_existing_project".to_string(),
                    resolved_project: String::new(),
                    registered: false,
                    permission: None,
                };
                (project, resolution)
            }
            CodingProjectSource::ManagedTemporary { client_id, name } => {
                if resume_session_id.is_some() {
                    return ToolResult::err_with_output(
                        "resume_session_id requires an existing project",
                        json!({
                            "error_kind": "invalid_arguments",
                            "failure_kind": "invalid_arguments",
                            "field": "resume_session_id",
                            "constraint": "managed_temporary_project_cannot_resume",
                            "state_changed": false,
                        }),
                    );
                }
                if let Some(result) =
                    registration_scope_denied(auth, "managed temporary project creation")
                {
                    return result;
                }
                let permission = self.permission_evaluator.evaluate("create_project", None);
                if let Some(decision) = permission.as_ref() {
                    if !decision.allows_execution() {
                        let mut result =
                            super::permissions::permission_execution_denied_result(decision);
                        super::permissions::add_permission_to_result(&mut result, decision);
                        return result;
                    }
                }
                if let Err(result) = self
                    .require_runner_coding_capability(&client_id, auth)
                    .await
                {
                    return attach_permission(result, permission.as_ref());
                }
                let created = self
                    .create_managed_temporary_project(client_id, name, auth)
                    .await;
                if !created.success {
                    return attach_permission(created, permission.as_ref());
                }
                let Some(project) = created
                    .output
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.trim().is_empty())
                    .map(str::to_string)
                else {
                    return attach_permission(
                        ToolResult::err_with_output(
                            "agent returned a managed temporary project without a runtime id",
                            json!({
                                "error_kind": "operation_failed",
                                "failure_kind": "operation_failed",
                                "state_changed": true,
                            }),
                        ),
                        permission.as_ref(),
                    );
                };
                let resolution = ProjectResolutionMetadata {
                    source: "managed_temporary".to_string(),
                    outcome: "managed_temporary_created".to_string(),
                    resolved_project: project.clone(),
                    registered: true,
                    permission,
                };
                (project, resolution)
            }
            CodingProjectSource::RunnerPath { client_id, path } => {
                if let Some(result) = registration_scope_denied(auth, "project path registration") {
                    return result;
                }
                let permission = self.permission_evaluator.evaluate("register_project", None);
                if let Some(decision) = permission.as_ref() {
                    if !decision.allows_execution() {
                        let mut result =
                            super::permissions::permission_execution_denied_result(decision);
                        super::permissions::add_permission_to_result(&mut result, decision);
                        return result;
                    }
                }
                if let Err(result) = self
                    .require_runner_coding_capability(&client_id, auth)
                    .await
                {
                    return attach_permission(result, permission.as_ref());
                }
                let resolved = self
                    .resolve_or_register_project(client_id, path, auth)
                    .await;
                if !resolved.success {
                    return attach_permission(resolved, permission.as_ref());
                }
                let Some(project) = resolved
                    .output
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.trim().is_empty())
                    .map(str::to_string)
                else {
                    return attach_permission(
                        ToolResult::err_with_output(
                            "Runner returned a path resolution without a runtime project id",
                            json!({
                                "error_kind": "operation_failed",
                                "failure_kind": "operation_failed",
                                "state_changed": resolved.output["registered"]
                                    .as_bool()
                                    .unwrap_or(false),
                            }),
                        ),
                        permission.as_ref(),
                    );
                };
                let outcome = resolved
                    .output
                    .get("outcome")
                    .and_then(Value::as_str)
                    .filter(|outcome| {
                        matches!(*outcome, "reused_existing_registration" | "auto_registered")
                    })
                    .map(str::to_string);
                let registered = resolved.output.get("registered").and_then(Value::as_bool);
                let (Some(outcome), Some(registered)) = (outcome, registered) else {
                    return attach_permission(
                        ToolResult::err_with_output(
                            "Runner returned malformed path resolution metadata",
                            json!({
                                "error_kind": "operation_failed",
                                "failure_kind": "operation_failed",
                                "state_changed": resolved.output["registered"]
                                    .as_bool()
                                    .unwrap_or(false),
                            }),
                        ),
                        permission.as_ref(),
                    );
                };
                if registered != (outcome == "auto_registered") {
                    return attach_permission(
                        ToolResult::err_with_output(
                            "Runner returned inconsistent path resolution metadata",
                            json!({
                                "error_kind": "operation_failed",
                                "failure_kind": "operation_failed",
                                "state_changed": registered,
                            }),
                        ),
                        permission.as_ref(),
                    );
                }
                let resolution = ProjectResolutionMetadata {
                    source: "path".to_string(),
                    outcome,
                    resolved_project: project.clone(),
                    registered,
                    permission,
                };
                (project, resolution)
            }
        };
        // `detail` is the single startup projection control: full keeps the
        // complete runtime status, recent commits, rules, and tool manifest;
        // standard/minimal use the compact projections.
        let compact_startup = detail != StartupDetail::Full;
        let include_recent_commits = detail == StartupDetail::Full;
        let include_tool_manifest = detail == StartupDetail::Full;
        let tool_manifest = if include_tool_manifest {
            match self.compact_tool_manifest_payload_bounded(None, None, None) {
                Ok(payload) => Some(payload),
                Err(result) => return attach_project_resolution(result, &project_resolution),
            }
        } else {
            None
        };

        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(err) => {
                return attach_project_resolution(err.into_tool_result(), &project_resolution)
            }
        };
        project_resolution.resolved_project = resolved.resolved_id.clone();
        // Semantic-navigation and fixed project-instruction observation remain
        // mandatory startup probes. The advanced start_coding_task entry also
        // runs the independent repository overview concurrently. The ordinary
        // work_on_project entry deliberately omits that optional scan/request.
        let (semantic_navigation, project_instructions, repository_overview) =
            if startup.include_repository_overview {
                futures_util::future::join3(
                    self.probe_semantic_navigation_for_startup(&resolved),
                    self.load_coding_project_instructions(&resolved.config),
                    self.repository_overview_for_startup(&resolved, auth),
                )
                .await
            } else {
                let (semantic_navigation, project_instructions) = futures_util::future::join(
                    self.probe_semantic_navigation_for_startup(&resolved),
                    self.load_coding_project_instructions(&resolved.config),
                )
                .await;
                (
                    semantic_navigation,
                    project_instructions,
                    repository_overview_not_requested(),
                )
            };
        let semantic_navigation = serde_json::to_value(semantic_navigation).unwrap_or_else(|_| {
            json!({
                "supported": false,
                "available": false,
                "status": "probe_failed",
                "reason_code": "status_probe_failed",
            })
        });
        // Coding startup always observes every fixed repository-rule
        // candidate. The complete bounded body remains only in the in-memory
        // Workflow Session; the ledger persistence path omits it.
        let mut warnings = Vec::new();
        if repository_overview.get("status").and_then(Value::as_str) == Some("unavailable")
            && repository_overview
                .get("reason_code")
                .and_then(Value::as_str)
                != Some(REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON)
        {
            warnings.push(json!({
                "kind": "repository_overview_unavailable",
                "message": "repository structure overview was unavailable during startup",
            }));
        }
        let continuity_key = if bind_current {
            match current_session_key(
                auth,
                transport,
                &resolved.resolved_id,
                &resolved.config.path,
                window,
            ) {
                Ok(key) => Some(key),
                Err(message) => {
                    warnings.push(if resume_requested {
                        json!({
                            "kind": "current_binding_unavailable",
                            "reason_code": "stable_window_identity_unavailable",
                            "message": "explicit Workflow Session resume continued without a current binding because stable chat-window identity was unavailable",
                        })
                    } else {
                        json!({
                            "kind": "current_binding_unavailable",
                            "message": message,
                        })
                    });
                    None
                }
            }
        } else {
            None
        };

        let mut runtime_status_call_failed = false;
        let (runtime_status, runtime_status_for_brief) = {
            let result = self.runtime_status(auth).await;
            if !result.success {
                runtime_status_call_failed = true;
                warnings.push(json!({
                    "kind": "runtime_status_unavailable",
                    "message": "runtime status was unavailable during startup",
                }));
            }
            let raw = result.output;
            let projected = if compact_startup {
                compact_runtime_status(&raw)
            } else {
                raw.clone()
            };
            (projected, raw)
        };
        let owning_runner_available = owning_runner_available(
            &resolved,
            &runtime_status_for_brief,
            runtime_status_call_failed,
        );
        let git = self
            .start_coding_task_git_summary(
                &resolved.resolved_id,
                include_recent_commits,
                &mut warnings,
            )
            .await;
        // Surface dirty/conflict worktree state at top-level so compact Action
        // responses that omit full git payloads still keep the warning reason.
        if !git.is_null() {
            append_workspace_warnings(&workspace_payload_from_git_summary(&git), &mut warnings);
        }
        let binding_available = bind_current && continuity_key.is_some();
        let write_scope_verified = auth.is_none_or(|auth| {
            !auth.is_oauth_token() || auth.has_scope(crate::auth::SCOPE_PROJECT_WRITE)
        });
        let session_outcome = match self.sessions.ensure_coding_session(
            sessions::CodingSessionRequest {
                key: continuity_key.clone(),
                project: resolved.resolved_id.clone(),
                resume_session_id: resume_session_id.clone(),
                instruction: title.clone(),
                mode,
                guards: sessions::SessionGuards {
                    deny_write_tools,
                    deny_shell_tools,
                },
                execution_context,
                project_instructions: Some(project_instructions.clone()),
                transport,
                bind_current: binding_available,
                new_session,
                // Startup always re-reads bounded current Git state and the
                // fixed project-instruction candidates.
                context_refreshed: true,
                write_scope_verified,
            },
        ) {
            Ok(outcome) => outcome,
            Err(sessions::CodingSessionError::InvalidResumeSessionId) => {
                return attach_project_resolution(
                    ToolResult::err_with_output(
                        "resume_session_id must be a valid wc_sess_* Workflow Session id",
                        json!({
                            "error_kind": "invalid_resume_session_id",
                            "failure_kind": "invalid_arguments",
                            "field": "resume_session_id",
                            "expected_format": "wc_sess_*",
                            "state_changed": false,
                        }),
                    ),
                    &project_resolution,
                );
            }
            Err(sessions::CodingSessionError::UnknownResumeSession { session_id }) => {
                return attach_project_resolution(
                    unknown_session_result(&session_id),
                    &project_resolution,
                );
            }
            Err(sessions::CodingSessionError::ResumeSessionNotActive {
                session_id,
                lifecycle,
            }) => {
                let error_kind = match lifecycle {
                    sessions::SessionLifecycle::Closed => "session_closed",
                    sessions::SessionLifecycle::Archived => "session_archived",
                    sessions::SessionLifecycle::Active => "session_lifecycle_denied",
                };
                return attach_project_resolution(
                    ToolResult::err_with_output(
                        format!(
                            "{error_kind}: start_coding_task cannot resume a {} session",
                            lifecycle.as_str()
                        ),
                        json!({
                            "error_kind": error_kind,
                            "failure_kind": error_kind,
                            "session_id": session_id,
                            "lifecycle": lifecycle,
                            "resume_requested": true,
                            "state_changed": false,
                        }),
                    ),
                    &project_resolution,
                );
            }
            Err(sessions::CodingSessionError::ResumeProjectMismatch {
                session_id,
                session_project,
                request_project,
            }) => {
                return attach_project_resolution(
                    ToolResult::err_with_output(
                        "session_project_mismatch: explicit Workflow Session resume requires an exact project match",
                        json!({
                            "error_kind": "session_project_mismatch",
                            "failure_kind": "session_project_mismatch",
                            "session_id": session_id,
                            "session_project": session_project,
                            "request_project": request_project,
                            "resume_requested": true,
                            "state_changed": false,
                        }),
                    ),
                    &project_resolution,
                );
            }
            Err(sessions::CodingSessionError::ResumeNewSessionConflict) => {
                return attach_project_resolution(
                    ToolResult::err_with_output(
                        "resume_session_id and new_session=true are mutually exclusive",
                        json!({
                            "error_kind": "invalid_arguments",
                            "failure_kind": "invalid_arguments",
                            "conflicting_fields": ["resume_session_id", "new_session"],
                            "constraint": "resume_session_id_mutually_exclusive_with_new_session",
                            "state_changed": false,
                        }),
                    ),
                    &project_resolution,
                );
            }
            Err(sessions::CodingSessionError::WriteScopeRequired) => {
                return attach_project_resolution(
                    ToolResult::err_with_output(
                        "session capability upgrade requires project:write",
                        json!({
                            "error_kind": "session_capability_upgrade_denied",
                            "required_scope": crate::auth::SCOPE_PROJECT_WRITE,
                            "mode": mode.as_str(),
                            "state_changed": false,
                        }),
                    ),
                    &project_resolution,
                );
            }
            Err(sessions::CodingSessionError::InvalidExecutionContext(error)) => {
                return attach_project_resolution(
                    ToolResult::err_with_output(
                        error,
                        json!({
                            "error_kind": "invalid_execution_context",
                            "failure_kind": "invalid_arguments",
                            "field": "execution_context",
                            "state_changed": false,
                        }),
                    ),
                    &project_resolution,
                );
            }
            Err(sessions::CodingSessionError::CommitFailed) => {
                return attach_project_resolution(
                    ToolResult::err_with_output(
                        "coding continuity state could not be committed",
                        json!({
                            "error_kind": "coding_continuity_commit_failed",
                            "state_changed": false,
                        }),
                    ),
                    &project_resolution,
                );
            }
        };
        let session_summary = &session_outcome.summary;
        let current_binding = if binding_available {
            json!({
                "bound": true,
                "session_id": session_summary.session_id,
                "process_local_cache": true,
                "durable_exact_binding": true,
                "restored_after_restart": true,
                "transport": transport.as_str(),
                "resolved_project": resolved.resolved_id.clone(),
            })
        } else {
            json!({
                "bound": false,
                "process_local_cache": true,
                "durable_exact_binding": true,
                "restored_after_restart": true,
                "transport": transport.as_str(),
                "reason_code": if bind_current {
                    if resume_requested {
                        "stable_window_identity_unavailable"
                    } else {
                        "window_identity_unavailable"
                    }
                } else {
                    "binding_disabled"
                },
            })
        };
        let mut connection_state = runtime_status
            .get("connection_layers")
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "runner_process": {"status": "not_observed"},
                    "server_transport": {"status": "not_observed"},
                    "server_registration": {"status": "not_observed"},
                    "project_registry": {"status": "resolved", "resolved_project": resolved.resolved_id},
                    "connector_endpoint": {"status": "not_observed"},
                    "session_binding": {"status": "not_observed"},
                    "last_successful_tool_call": {"status": "not_observed"},
                })
            });
        connection_state["project_registry"]["resolved_project"] = json!(resolved.resolved_id);
        connection_state["session_binding"] = json!({
            "status": if binding_available { "bound" } else { "not_bound" },
            "observed_at": chrono::Utc::now().timestamp(),
            "source": "session_store",
            "age_secs": 0,
            "stale_after_secs": Value::Null,
            "reason_code": if binding_available {
                Value::Null
            } else if bind_current {
                if resume_requested {
                    json!("stable_window_identity_unavailable")
                } else {
                    json!("window_identity_unavailable")
                }
            } else {
                json!("binding_disabled")
            },
            "process_local_cache": true,
            "durable_exact_binding": true,
            "restored_after_restart": true,
            "requires_stable_window_identity": true,
            "transport": transport.as_str(),
            "durable_resume": "the same exact principal, transport, stable window, project, and canonical repository root resumes the durable wc_sess_* session",
        });
        let recommended_flow = match &tool_manifest {
            Some(manifest) => recommended_flow_payload_for_manifest_tools(manifest),
            None => recommended_flow_payload(),
        };
        // Continuation feedback for reused/resumed/restored sessions. Pure
        // read-only projection over existing session ledger, validation evidence,
        // bounded job metadata, and the message board. Never executes shell,
        // reads project files, enqueues Agent requests, mutates the ledger,
        // refreshes activity, or consumes guidance. `created` (fresh empty
        // session) surfaces a compact `not_applicable` verdict.
        let continuation_kind = if resume_requested {
            "resumed_explicitly"
        } else if session_outcome.reused {
            "continued"
        } else {
            "created"
        };
        // Read the lifecycle-aware, project-scoped summary once, after the
        // potentially slow startup probes, then share it across continuation,
        // the legacy full verdict, and the model-facing brief.
        let active_jobs = self
            .active_jobs_summary(Some(&resolved.resolved_id), auth, 10)
            .await;
        let continuation_feedback = self
            .start_continuation_feedback(
                &session_outcome.summary,
                session_outcome.pre_instruction_summary.as_ref(),
                continuation_kind,
                &active_jobs,
                git.pointer("/counts/conflicted")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0,
            )
            .await;
        let mut output = json!({
            "detail": detail.as_str(),
            "project": project.clone(),
            "project_resolution": project_resolution.clone(),
            "resolved_project": resolved_project_payload(&resolved),
            "session": {
                "session_id": session_summary.session_id,
                "mode": session_summary.mode,
                "guards": session_summary.guards,
                "execution_context": session_summary.execution_context,
                "lifecycle": session_summary.lifecycle,
                "continuation": if resume_requested {
                    "resumed_explicitly"
                } else if session_outcome.reused {
                    "continued"
                } else {
                    "created"
                },
                "reused": session_outcome.reused,
                "resume_requested": resume_requested,
                "new_session_requested": new_session,
                "instruction_appended": title.is_some(),
                "root_title": session_summary.title,
                "capability": {
                    "changed": session_outcome.capability_changed,
                    "previous_mode": session_outcome.previous_mode,
                    "previous_guards": session_outcome.previous_guards,
                    "requested_mode": mode,
                    "mode": session_summary.mode,
                    "guards": session_summary.guards,
                    "write_scope_verified": write_scope_verified,
                },
                "context": {
                    "refreshed": true,
                    "git_state_recaptured": true,
                    "rules_recaptured": true,
                    "execution_context_changed": session_outcome.execution_context_changed,
                },
                "explicit_session_id_required_for_continuity": !binding_available,
                "explicit_session_id_recommended": !binding_available,
                "explicit_session_id_fields": {
                    "tool_business_input": "session_id",
                    "generic_wrapper_recorder": TOOL_CALL_RECORDING_SESSION_ID_FIELD
                },
                "current_binding": current_binding,
            },
            "runtime_status": runtime_status.clone(),
            "connection_state": connection_state,
            "authority": authority_profile_payload(),
            "rules": rules_summary(Some(&project_instructions)),
            "git": git.clone(),
            "semantic_navigation": semantic_navigation.clone(),
            "recommended_flow": recommended_flow,
            "continuation_feedback": continuation_feedback.clone(),
            "deterministic": true,
            "llm_summary": false,
            "warnings": warnings,
        });
        if let Some(tool_manifest) = tool_manifest {
            output["tool_manifest"] = tool_manifest;
        }
        output["startup_verdict"] = startup_verdict(
            &output,
            &active_jobs,
            owning_runner_available,
            runtime_status_call_failed,
            include_tool_manifest,
        );
        let previous_instructions = session_outcome
            .pre_instruction_summary
            .as_ref()
            .and_then(|summary| summary.project_instructions.as_ref());
        // Reload rule bodies only when there is no prior snapshot to compare
        // against (fresh session, or a session whose rules were never
        // persisted, e.g. restored after a restart). Otherwise the shared
        // brief compares fingerprints and reports reused/changed: an exact
        // resume with unchanged rules returns `reused` without repeating the
        // body, and changed rules return `changed` with the new bounded body.
        let force_instruction_load = previous_instructions.is_none();
        let binding_reason_code = if binding_available {
            None
        } else if bind_current {
            Some(if resume_requested {
                "stable_window_identity_unavailable"
            } else {
                "window_identity_unavailable"
            })
        } else {
            Some("binding_disabled")
        };
        let canonical_repository_root_matches = if resume_requested {
            None
        } else {
            // A fresh Session starts at the resolved root. Automatic reuse can
            // only come from the exact current binding, whose identity includes
            // the canonical repository-root key.
            Some(true)
        };
        let project_resolution_value =
            serde_json::to_value(&project_resolution).unwrap_or_else(|_| json!({}));
        let startup_brief = build_startup_brief(StartupBriefInput {
            detail,
            requested_project: &project,
            project_resolution: &project_resolution_value,
            resolved: &resolved,
            session: session_summary,
            continuation_kind,
            reused: session_outcome.reused,
            resume_requested,
            binding_available,
            binding_reason_code,
            instructions: &project_instructions,
            previous_instructions,
            force_instruction_load,
            git: &git,
            semantic_navigation: &semantic_navigation,
            repository: &repository_overview,
            continuation_feedback: &continuation_feedback,
            active_jobs: &active_jobs,
            owning_runner_available,
            canonical_repository_root_matches,
            runtime_status_call_failed,
        });
        let result = if detail == StartupDetail::Full {
            output["startup_brief"] = startup_brief;
            ToolResult::ok(output)
        } else {
            ToolResult::ok(startup_brief)
        };
        attach_permission(result, project_resolution.permission.as_ref())
    }

    /// Thin `start_coding_task` wrapper for the daily model coding loop.
    ///
    /// The wrapper validates only its three simple inputs, maps them onto
    /// normal coding-task defaults, delegates the entire business implementation
    /// to `start_coding_task`, and projects a compact startup result. It never
    /// reads the current window, binds a current Session, guesses a recent
    /// Session, or falls back to a credential-wide Session. With `session_id`
    /// present, it exactly resumes that one Workflow Session after the existing
    /// project/lifecycle/access/capability checks and never creates or falls
    /// back on failure.
    pub(crate) async fn work_on_project(
        &self,
        project: String,
        client_id: Option<String>,
        path: Option<String>,
        instruction: String,
        session_id: Option<String>,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        let project_source = match resolve_project_source(project, client_id, path, None, false) {
            Ok(source) => source,
            Err(result) => return result,
        };
        let (project, client_id, path) = match project_source {
            CodingProjectSource::Existing { project } => (project, None, None),
            CodingProjectSource::RunnerPath { client_id, path } => {
                (String::new(), Some(client_id), Some(path))
            }
            CodingProjectSource::ManagedTemporary { .. } => {
                unreachable!("managed temporary project is disabled for work_on_project")
            }
        };
        let instruction = instruction.trim().to_string();
        if instruction.is_empty()
            || instruction.chars().count() > sessions::MAX_CODING_INSTRUCTION_CHARS
        {
            return ToolResult::err_with_output(
                format!(
                    "instruction must contain 1..={} characters",
                    sessions::MAX_CODING_INSTRUCTION_CHARS
                ),
                json!({
                    "error_kind": "invalid_coding_instruction",
                    "field": "instruction",
                    "max_chars": sessions::MAX_CODING_INSTRUCTION_CHARS,
                    "state_changed": false,
                }),
            );
        }
        let session_id = match session_id {
            Some(session_id)
                if session_id != session_id.trim()
                    || !sessions::is_valid_session_id(&session_id) =>
            {
                return ToolResult::err_with_output(
                    "session_id must be a valid wc_sess_* Workflow Session id",
                    json!({
                        "error_kind": "invalid_session_id",
                        "failure_kind": "invalid_arguments",
                        "field": "session_id",
                        "expected_format": "wc_sess_*",
                        "state_changed": false,
                    }),
                );
            }
            Some(session_id) => Some(session_id),
            None => None,
        };
        // Map onto the existing coding-task business implementation. The
        // internal work-on-project profile keeps the standard shared brief,
        // including rules, semantic navigation, workspace, and job metadata,
        // while deliberately skipping the optional repository overview.
        // bind_current=false keeps the wrapper window-agnostic; it never
        // establishes a current-window binding or guesses an old Session.
        let new_session = session_id.is_none();
        let result = self
            .start_coding_task_with_options(
                project.clone(),
                client_id,
                path,
                None,
                Some(instruction.clone()),
                SessionMode::Normal,
                false,
                false,
                CodingStartupOptions::work_on_project(),
                session_id.clone(),
                false,
                new_session,
                None,
                auth,
                transport,
                window,
            )
            .await;
        if !result.success {
            return result;
        }
        let projected_project = if project.is_empty() {
            startup_brief_from_output(&result.output)
                .and_then(|brief| {
                    brief
                        .pointer("/project_resolution/resolved_project")
                        .and_then(Value::as_str)
                })
                .unwrap_or_default()
                .to_string()
        } else {
            project
        };
        project_work_on_project_output(projected_project, result.output)
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

        let show_changes_call = ToolCall::ShowChanges {
            project: resolved.resolved_id.clone(),
            session_id: Some(session_id.clone()),
            include_diff: Some(include_diff),
            max_hunks: None,
            max_hunk_lines: None,
            session_event_limit: Some(50),
        };
        let show_changes_start = self.sessions.record_tool_call_started_with_options(
            Some(&session_id),
            SessionTransport::Api,
            show_changes_call.tool_name(),
            &show_changes_call.session_log_arguments(),
            Some(resolved.resolved_id.clone()),
        );
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
        self.sessions.record_tool_call_finished(
            show_changes_start,
            changes_result.success,
            &changes_result.output,
            changes_result.error.as_deref(),
            None,
        );
        if !changes_result.success {
            final_warnings.push(json!({
                "kind": "show_changes_failed",
                "message": changes_result.error,
            }));
        }
        let workspace = workspace_payload_from_show_changes(&changes_result.output);
        append_workspace_warnings(&workspace, &mut final_warnings);

        let validation = if include_validation_summary {
            self.validation_summary_for_session_with_jobs(&session_summary, 10, auth)
                .await
        } else {
            skipped_validation_summary()
        };
        let permissions = permission_summary_from_events(
            &session_summary.events,
            super::permissions::DEFAULT_PERMISSION_RECENT_LIMIT,
        );

        let hygiene = if include_hygiene {
            let hygiene_call = ToolCall::WorkspaceHygieneCheck {
                project: resolved.resolved_id.clone(),
                max_findings: None,
                include_tracked: None,
                session_id: Some(session_id.clone()),
            };
            let hygiene_start = self.sessions.record_tool_call_started_with_options(
                Some(&session_id),
                SessionTransport::Api,
                hygiene_call.tool_name(),
                &hygiene_call.session_log_arguments(),
                Some(resolved.resolved_id.clone()),
            );
            let result = self
                .workspace_hygiene_check(
                    resolved.resolved_id.clone(),
                    None,
                    None,
                    Some(session_id.clone()),
                )
                .await;
            self.sessions.record_tool_call_finished(
                hygiene_start,
                result.success,
                &result.output,
                result.error.as_deref(),
                None,
            );
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
        let closeout_session_summary = self
            .sessions
            .summary(&session_id, Some(FINISH_SESSION_EVENT_LIMIT))
            .unwrap_or_else(|| session_summary.clone());
        let review_evidence = review_evidence_summary_for_session(&closeout_session_summary);
        let (work_performed, changed_paths) =
            closeout_work_projection(&closeout_session_summary.events);

        // Continuation feedback reuses the same attempt summary and validation
        // delta projections as start/handoff. It is a read-only projection over
        // the existing closeout summary, validation, and job metadata; it never
        // re-runs validation, mutates the ledger, or replaces the closeout
        // verdict.
        let (discussion, guidance_available) = self.discussion_snapshot(&session_id);
        let continuation_validation = if include_validation_summary {
            validation.clone()
        } else {
            json!({ "available": false, "not_requested": true })
        };
        let continuation_feedback = if closeout_session_summary.events.is_empty() {
            not_applicable_continuation_feedback_value("empty_session")
        } else {
            continuation_feedback_value(ContinuationFeedbackInput {
                session_summary: &closeout_session_summary,
                validation: &continuation_validation,
                jobs: &jobs,
                discussion: &discussion,
                continuation: "continued",
                suggest_exploration_continuity: false,
                workspace_conflicts: workspace
                    .pointer("/counts/conflicted")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0,
            })
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
            "continuation_feedback": continuation_feedback,
            "permissions": permissions,
            "tool_failures": tool_failure_summary_from_events(&session_summary.events, 10),
            "review_evidence": review_evidence,
            "work_performed": work_performed,
            "changed_paths": changed_paths,
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
        let resolved_unexpected_validation_failures = resolved_unexpected_validation_failure_count(
            &session_summary.events,
            output.get("validation").unwrap_or(&Value::Null),
            true,
            output
                .get("workspace")
                .and_then(|workspace| workspace.get("clean"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            output
                .get("hygiene")
                .and_then(|hygiene| hygiene.get("clean"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            output
                .get("jobs")
                .and_then(|jobs| jobs.get("blocking_active_count"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
        output["suggested_next_actions"] = json!(finish_suggested_next_actions(
            &output,
            resolved_unexpected_validation_failures,
        ));
        output["handoff_brief"] = build_handoff_brief(HandoffBriefInput {
            session_summary: &closeout_session_summary,
            continuation_feedback: output.get("continuation_feedback").unwrap_or(&Value::Null),
            workspace_requested: include_workspace,
            workspace: output.get("workspace"),
            validation_requested: include_validation_summary,
            validation: output.get("validation"),
            jobs: output.get("jobs"),
            guidance_available,
            existing_suggested_actions: output.get("suggested_next_actions"),
        });
        let compact = compact_finish_output(&output, resolved_unexpected_validation_failures);
        if summary_only {
            return ToolResult::ok(compact);
        }
        for field in [
            "facts",
            "hard_blockers",
            "advisories",
            "task_outcome",
            "evidence_history",
            "evidence_integrity",
            "informational_notes",
        ] {
            output[field] = compact.get(field).cloned().unwrap_or(Value::Null);
        }
        output["suggested_next_actions"] = compact["suggested_next_actions"].clone();
        ToolResult::ok(output)
    }

    /// Build the bounded continuation feedback projection for `start_coding_task`.
    ///
    /// Pure read-only: validation is derived from the session ledger only
    /// (`validation_summary_from_events`, no job-status enrichment), jobs come
    /// from the bounded `active_jobs_summary` metadata, and guidance is read
    /// from the message board without marking anything read or resolved. No
    /// shell, no file reads, no Agent requests, no ledger mutation.
    async fn start_continuation_feedback(
        &self,
        summary: &sessions::SessionSummary,
        pre_instruction_summary: Option<&sessions::SessionSummary>,
        continuation_kind: &'static str,
        jobs: &Value,
        workspace_conflicts: bool,
    ) -> Value {
        // Fresh new session: nothing to continue from.
        if continuation_kind == "created" {
            return not_applicable_continuation_feedback_value("fresh_session");
        }
        // For a reused/resumed/restored session, project over the snapshot taken
        // *before* this new `task_instruction` was appended, so the feedback
        // describes the previous attempt's work rather than the empty new
        // attempt. The returned session itself still contains the new
        // instruction; only the projection uses the pre-instruction window.
        // Guidance and job state are read from the live session id at the same
        // instant; neither is mutated.
        let projection_summary = pre_instruction_summary.unwrap_or(summary);
        if projection_summary.events.is_empty() {
            return not_applicable_continuation_feedback_value("empty_session");
        }
        let validation = super::validation_events::validation_summary_from_events(
            &projection_summary.events,
            20,
        );
        let (discussion, _) = self.discussion_snapshot(&summary.session_id);
        continuation_feedback_value(ContinuationFeedbackInput {
            session_summary: projection_summary,
            validation: &validation,
            jobs,
            discussion: &discussion,
            continuation: continuation_kind,
            suggest_exploration_continuity: true,
            workspace_conflicts,
        })
    }

    fn discussion_snapshot(&self, session_id: &str) -> (sessions::SessionDiscussionSummary, bool) {
        match self.sessions.discussion_summary(session_id, Some(20)) {
            Ok(discussion) => (discussion, true),
            Err(_) => (
                sessions::SessionDiscussionSummary {
                    counts: sessions::SessionDiscussionCounts {
                        total: 0,
                        open: 0,
                        resolved: 0,
                        guidance: 0,
                        progress: 0,
                        risk: 0,
                        todo: 0,
                        question: 0,
                        decision: 0,
                    },
                    open_guidance: Vec::new(),
                    open_questions: Vec::new(),
                    open_risks: Vec::new(),
                    open_todos: Vec::new(),
                    recent_progress: Vec::new(),
                    recent_decisions: Vec::new(),
                },
                false,
            ),
        }
    }

    async fn start_coding_task_git_summary(
        &self,
        project: &str,
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

        {
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
        } else if let Some(object) = output.as_object_mut() {
            object.remove("recent_commits");
        }

        output
    }

    /// Deterministic repository structure overview for the coding startup
    /// brief. Reuses the existing `project_overview` implementation and keeps
    /// every safety property: directory entries, file types, and the git
    /// tracked index only; no file bodies, no project code execution, no
    /// symlink following, no protected/sensitive/build/cache paths, and only
    /// project-relative paths are returned.
    ///
    /// For agent-backed projects the overview is routed to the owning Runner
    /// via the `file_project_overview` op with a short startup probe timeout.
    /// On timeout the request is cancelled. An optional overview failure never
    /// fails the already-legal coding task: it returns a deterministic
    /// unavailable marker and the caller surfaces a
    /// `repository_overview_unavailable` warning without leaking raw errors,
    /// absolute paths, or Runner output.
    async fn repository_overview_for_startup(
        &self,
        resolved: &ResolvedProject,
        auth: Option<&AuthContext>,
    ) -> Value {
        if !resolved.config.is_agent() {
            return repository_overview_local(&resolved.config.path);
        }
        let client_id = match resolved.config.agent_client_id() {
            Ok(client_id) => client_id,
            Err(_) => return repository_overview_unavailable(),
        };
        // The owning runner must support the structured file capability.
        if !self
            .shell_clients
            .client_supports_for_auth(client_id, SHELL_CLIENT_CAPABILITY_FILE_READ, auth)
            .await
            .unwrap_or(false)
        {
            return repository_overview_unavailable();
        }
        // Short startup probe budget, much tighter than the standalone
        // `project_overview` tool's 30s wait.
        let probe_wait_timeout = self.repository_overview_probe_timeout.as_secs().max(1);
        let (request_id, receiver) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: "project_overview".to_string(),
                    client_id: client_id.to_string(),
                    path: ".".to_string(),
                    cwd: Some(resolved.config.path.clone()),
                    content: Some(
                        json!({
                            "max_depth": STARTUP_OVERVIEW_REQUEST_MAX_DEPTH,
                            "limit": STARTUP_OVERVIEW_REQUEST_LIMIT,
                        })
                        .to_string(),
                    ),
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: probe_wait_timeout,
                },
                "coding_startup".to_string(),
            )
            .await
        {
            Ok(enqueued) => enqueued,
            Err(_) => return repository_overview_unavailable(),
        };
        match tokio::time::timeout(
            Duration::from_secs(probe_wait_timeout.saturating_add(2)),
            receiver,
        )
        .await
        {
            Ok(Ok(response)) if response.exit_code == Some(0) && response.error.is_none() => {
                // The Runner response is untrusted: it must parse and pass the
                // shared project-overview contract validation against the fixed
                // request bounds (root / depth 2 / limit 120). A malformed,
                // boundary-mismatched, or schema-violating payload fails closed
                // to an unavailable marker without leaking raw stdout, errors,
                // or absolute paths. The normalized result keeps only the
                // formal contract fields.
                match serde_json::from_str::<Value>(response.stdout.as_deref().unwrap_or_default())
                {
                    Ok(parsed) => match validate_project_overview_for_startup(&parsed) {
                        Ok(mut overview) => {
                            if let Some(object) = overview.as_object_mut() {
                                object.insert("status".to_string(), json!("available"));
                                object.insert("reason_code".to_string(), Value::Null);
                                object.insert(
                                    "project".to_string(),
                                    json!(resolved.resolved_id.clone()),
                                );
                            }
                            overview
                        }
                        Err(_) => repository_overview_unavailable(),
                    },
                    Err(_) => repository_overview_unavailable(),
                }
            }
            Ok(Ok(_)) => repository_overview_unavailable(),
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                repository_overview_unavailable()
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                repository_overview_unavailable()
            }
        }
    }
}

/// Deterministic unavailable marker for the startup repository overview. Never
/// carries raw errors, absolute paths, or Runner output.
fn repository_overview_unavailable() -> Value {
    json!({
        "status": "unavailable",
        "reason_code": "unsupported_or_unavailable",
    })
}

/// Compact marker for the ordinary work_on_project profile. This is an
/// intentional omission, not a failed repository probe, so it must not produce
/// an unavailable warning or lower readiness.
fn repository_overview_not_requested() -> Value {
    json!({
        "status": "unavailable",
        "reason_code": REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON,
    })
}

/// Fixed startup overview request bounds. The overview is always scoped to the
/// project root with depth 2 and limit 120; a Runner response that reports a
/// different `path`, `max_depth`, or `limit` is malformed and fails closed.
const STARTUP_OVERVIEW_REQUEST_PATH: &str = "";
const STARTUP_OVERVIEW_REQUEST_MAX_DEPTH: usize = 2;
const STARTUP_OVERVIEW_REQUEST_LIMIT: usize = 120;

/// Validate a startup overview payload against the fixed request bounds using
/// the shared contract entry. Returns the normalized formal-contract payload
/// on success. Used for both agent-backed Runner responses and the local
/// builder so the two paths cannot drift.
fn validate_project_overview_for_startup(payload: &Value) -> Result<Value, String> {
    crate::project_overview::validate_project_overview(
        payload,
        STARTUP_OVERVIEW_REQUEST_PATH,
        STARTUP_OVERVIEW_REQUEST_MAX_DEPTH,
        STARTUP_OVERVIEW_REQUEST_LIMIT,
    )
}

/// Build the repository overview locally for a non-agent project. Mirrors the
/// `project_overview` bounds and safety contract without a shell. The output
/// is normalized through the same shared contract entry used for Runner
/// responses, so extra fields can never reach the startup brief.
fn repository_overview_local(project_root: &str) -> Value {
    match crate::project_overview::build_project_overview(
        Path::new(project_root),
        ".",
        Some(STARTUP_OVERVIEW_REQUEST_MAX_DEPTH),
        Some(STARTUP_OVERVIEW_REQUEST_LIMIT),
    ) {
        Ok(overview) => match validate_project_overview_for_startup(&overview) {
            Ok(mut normalized) => {
                if let Some(object) = normalized.as_object_mut() {
                    object.insert("status".to_string(), json!("available"));
                    object.insert("reason_code".to_string(), Value::Null);
                }
                normalized
            }
            Err(_) => repository_overview_unavailable(),
        },
        Err(_) => repository_overview_unavailable(),
    }
}

#[derive(Deserialize)]
struct WorkOnProjectBriefProjection {
    session: WorkOnProjectSessionProjection,
    project: WorkOnProjectProjectProjection,
    project_resolution: ProjectResolutionMetadata,
    workspace: WorkOnProjectWorkspaceProjection,
    workflow: Value,
    instructions: WorkOnProjectInstructionsProjection,
    semantic_navigation: WorkOnProjectSemanticNavigationProjection,
    repository: Value,
    continuation: WorkOnProjectContinuationProjection,
    blockers: Vec<String>,
    warnings: Vec<String>,
    startup_verdict: WorkOnProjectStartupVerdictProjection,
}

#[derive(Deserialize)]
struct WorkOnProjectSessionProjection {
    session_id: String,
    continuation: String,
    execution_context: sessions::SessionExecutionContext,
}

#[derive(Deserialize)]
struct WorkOnProjectProjectProjection {
    resolved_id: String,
}

#[derive(Deserialize)]
struct WorkOnProjectSemanticNavigationProjection {
    #[serde(default)]
    supported: bool,
    available: bool,
    status: String,
    capability: WorkOnProjectRequiredNullable<String>,
    reason_code: WorkOnProjectRequiredNullable<String>,
}

#[derive(Deserialize)]
struct WorkOnProjectJobsProjection {
    active_count: u64,
    blocking_active_count: u64,
    nonblocking_active_count: u64,
    recovering_count: u64,
    terminal_pending_count: u64,
    latest_status: String,
}

#[derive(Deserialize, Serialize)]
#[serde(transparent)]
struct WorkOnProjectRequiredNullable<T>(Option<T>);

#[derive(Deserialize)]
struct WorkOnProjectWorkspaceProjection {
    status: String,
    git_available: WorkOnProjectRequiredNullable<bool>,
    branch: WorkOnProjectRequiredNullable<String>,
    head: WorkOnProjectRequiredNullable<String>,
    clean: WorkOnProjectRequiredNullable<bool>,
    conflicts: u64,
}

#[derive(Deserialize)]
struct WorkOnProjectInstructionsProjection {
    status: String,
    sources: Vec<WorkOnProjectInstructionSourceProjection>,
    #[serde(default)]
    changed_sources: Option<Vec<String>>,
    #[serde(default)]
    content_included: bool,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    total_chars: u64,
}

#[derive(Deserialize, Serialize)]
struct WorkOnProjectInstructionSourceProjection {
    path: String,
    fingerprint: String,
    truncated: bool,
    headings: Vec<String>,
    content: WorkOnProjectRequiredNullable<String>,
    read_more: WorkOnProjectRequiredNullable<WorkOnProjectReadMoreProjection>,
}

#[derive(Deserialize, Serialize)]
struct WorkOnProjectReadMoreProjection {
    path: String,
    start_line: u64,
    limit: u64,
}

#[derive(Deserialize)]
struct WorkOnProjectContinuationProjection {
    suggested_next_actions: WorkOnProjectActionItemsProjection,
    jobs: WorkOnProjectJobsProjection,
}

#[derive(Deserialize)]
struct WorkOnProjectActionItemsProjection {
    items: Vec<String>,
}

#[derive(Deserialize)]
struct WorkOnProjectStartupVerdictProjection {
    status: String,
    blocking: bool,
    suggested_next_actions: Vec<String>,
}

/// Convert a successful `start_coding_task` result into the compact
/// `work_on_project` contract. The delegated call may already have changed
/// Session state, so protocol drift fails closed with `state_changed=true`.
pub(crate) fn project_work_on_project_output(project: String, output: Value) -> ToolResult {
    let permission = output.get("permission").cloned();
    let Some(brief) = startup_brief_from_output(&output) else {
        return work_on_project_projection_failed(
            "output",
            "complete startup brief object",
            "non-object",
            None,
        );
    };
    let projection = match serde_json::from_value::<WorkOnProjectBriefProjection>(brief.clone()) {
        Ok(projection) => projection,
        Err(error) => {
            return work_on_project_projection_failed(
                "output",
                "complete typed startup brief",
                "missing or wrongly typed field",
                Some(error.to_string()),
            )
        }
    };
    if !sessions::is_valid_session_id(&projection.session.session_id) {
        return work_on_project_projection_failed(
            "session.session_id",
            "valid wc_sess_* string",
            "invalid string",
            None,
        );
    }
    if !matches!(
        projection.session.continuation.as_str(),
        "created" | "continued" | "resumed_explicitly"
    ) {
        return work_on_project_projection_failed(
            "session.continuation",
            "created, continued, or resumed_explicitly",
            "unsupported string",
            None,
        );
    }
    if !matches!(
        projection.workspace.status.as_str(),
        "clean" | "dirty" | "blocked" | "unavailable"
    ) {
        return work_on_project_projection_failed(
            "workspace.status",
            "clean, dirty, blocked, or unavailable",
            "unsupported string",
            None,
        );
    }
    if !matches!(
        projection.instructions.status.as_str(),
        "loaded" | "reused" | "changed" | "not_found" | "unavailable"
    ) {
        return work_on_project_projection_failed(
            "instructions.status",
            "loaded, reused, changed, not_found, or unavailable",
            "unsupported string",
            None,
        );
    }
    if projection.instructions.sources.len() > 5 {
        return work_on_project_projection_failed(
            "instructions.sources",
            "at most 5 source objects",
            "invalid array contents",
            None,
        );
    }
    if projection.workflow != builtin_coding_workflow_projection() {
        return work_on_project_projection_failed(
            "workflow",
            "canonical built-in coding workflow contract",
            "non-canonical workflow projection",
            None,
        );
    }

    let suggested_next_actions = if projection.startup_verdict.suggested_next_actions.is_empty() {
        projection.continuation.suggested_next_actions.items
    } else {
        projection.startup_verdict.suggested_next_actions
    };
    let mut instructions = json!({
        "status": projection.instructions.status,
        "sources": projection.instructions.sources,
        "content_included": projection.instructions.content_included,
        "truncated": projection.instructions.truncated,
        "total_chars": projection.instructions.total_chars,
    });
    if let Some(changed_sources) = projection.instructions.changed_sources {
        instructions["changed_sources"] = json!(changed_sources);
    }
    let semantic_navigation = json!({
        "supported": projection.semantic_navigation.supported,
        "available": projection.semantic_navigation.available,
        "status": projection.semantic_navigation.status,
        "capability": projection.semantic_navigation.capability,
        "reason_code": projection.semantic_navigation.reason_code,
    });
    let mut result = ToolResult::ok(json!({
        "session_id": projection.session.session_id,
        "project": project,
        "resolved_project": projection.project.resolved_id,
        "project_resolution": projection.project_resolution,
        "continuation": projection.session.continuation,
        "execution_context": projection.session.execution_context,
        "readiness": {
            "status": projection.startup_verdict.status,
            "blocking": projection.startup_verdict.blocking,
        },
        "workspace": {
            "status": projection.workspace.status,
            "git_available": projection.workspace.git_available,
            "branch": projection.workspace.branch,
            "head": projection.workspace.head,
            "clean": projection.workspace.clean,
            "conflicts": projection.workspace.conflicts,
        },
        "workflow": projection.workflow,
        "repository": projection.repository,
        "instructions": instructions,
        "semantic_navigation": semantic_navigation,
        "jobs": {
            "active_count": projection.continuation.jobs.active_count,
            "blocking_active_count": projection.continuation.jobs.blocking_active_count,
            "nonblocking_active_count": projection.continuation.jobs.nonblocking_active_count,
            "recovering_count": projection.continuation.jobs.recovering_count,
            "terminal_pending_count": projection.continuation.jobs.terminal_pending_count,
            "latest_status": projection.continuation.jobs.latest_status,
        },
        "blockers": projection.blockers,
        "warnings": projection.warnings,
        "suggested_next_actions": suggested_next_actions,
        "deterministic": true,
        "llm_summary": false,
    }));
    if let Some(permission) = permission {
        result.output["permission"] = permission;
    }
    result
}

fn work_on_project_projection_failed(
    field: &str,
    expected: &str,
    actual: &str,
    detail: Option<String>,
) -> ToolResult {
    ToolResult::err_with_output(
        format!("work_on_project projection failed: {field} expected {expected}, got {actual}"),
        json!({
            "error_kind": "work_on_project_projection_failed",
            "failure_kind": "work_on_project_projection_failed",
            "underlying_tool": "start_coding_task",
            "field": field,
            "expected": expected,
            "actual": actual,
            "detail": detail,
            "state_changed": true,
        }),
    )
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
        "scan_complete": snapshot.scan_complete,
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
        "fingerprint": file.fingerprint.clone(),
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

/// Full default startup recommended flow. Reuses the shared
/// `TOOL_RECOMMENDED_FLOWS` group definitions so top-level startup guidance
/// does not drift from `tool_manifest.recommended_flows`.
fn recommended_flow_payload() -> Value {
    recommended_flow_groups(None)
}

/// Project top-level `recommended_flow` onto tools present in the embedded
/// `tool_manifest`. Group keys stay fixed; empty groups are allowed.
fn recommended_flow_payload_for_manifest_tools(manifest: &Value) -> Value {
    let visible: HashSet<&str> = manifest
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect();
    recommended_flow_groups(Some(&visible))
}

fn recommended_flow_groups(visible: Option<&HashSet<&str>>) -> Value {
    const GROUPS: &[&str] = &["inspect", "edit", "validate", "review", "handoff"];
    let mut map = serde_json::Map::new();
    for group in GROUPS {
        let tools = TOOL_RECOMMENDED_FLOWS
            .iter()
            .find(|flow| flow.name == *group)
            .map(|flow| {
                let mut seen = HashSet::new();
                flow.tools
                    .iter()
                    .copied()
                    .filter(|tool| {
                        let allowed = match visible {
                            Some(set) => set.contains(*tool),
                            None => true,
                        };
                        allowed && seen.insert(*tool)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        map.insert((*group).to_string(), json!(tools));
    }
    Value::Object(map)
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

/// Map startup `git` summary fields into the workspace warning shape.
fn workspace_payload_from_git_summary(git: &Value) -> Value {
    let counts = git.get("counts").cloned().unwrap_or_else(|| json!({}));
    json!({
        "clean": git.get("clean").and_then(Value::as_bool).unwrap_or(false),
        "git_available": git
            .get("available")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "changed_files_count": git
            .get("changed_files_count")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| changed_files_count_from_counts(&counts)),
        "counts": counts,
    })
}

fn compact_finish_output(output: &Value, resolved_unexpected_validation_failures: usize) -> Value {
    let hygiene_checked = output
        .get("hygiene")
        .is_some_and(|hygiene| !hygiene.is_null());
    let workspace_clean = output
        .get("workspace")
        .and_then(|workspace| workspace.get("clean"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let workspace_conflicts = output
        .pointer("/workspace/counts/conflicted")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let hygiene_clean = output
        .get("hygiene")
        .and_then(|hygiene| hygiene.get("clean"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let hygiene_secret_like_paths = output
        .pointer("/hygiene/counts/secret_like_paths")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let hygiene_truncated = output
        .pointer("/hygiene/truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut compact = json!({
        "summary_only": true,
        "project": output.get("project").cloned().unwrap_or(Value::Null),
        "session_id": output.get("session_id").cloned().unwrap_or(Value::Null),
        "workspace_clean": workspace_clean,
        "workspace_conflicts": workspace_conflicts,
        "hygiene_clean": hygiene_clean,
        "hygiene_secret_like_paths": hygiene_secret_like_paths,
        "hygiene_truncated": hygiene_truncated,
        "jobs": compact_jobs(output.get("jobs").unwrap_or(&Value::Null)),
        "permissions": compact_permissions(output.get("permissions").unwrap_or(&Value::Null)),
        "tool_failures": compact_tool_failures(output.get("tool_failures").unwrap_or(&Value::Null)),
        "validation": compact_validation(output.get("validation").unwrap_or(&Value::Null)),
        "review_evidence": compact_review_evidence(output.get("review_evidence").unwrap_or(&Value::Null)),
        "work_performed": output.get("work_performed").cloned().unwrap_or_else(|| json!([])),
        "changed_paths": output.get("changed_paths").cloned().unwrap_or_else(|| json!([])),
        "handoff_brief": output.get("handoff_brief").cloned().unwrap_or(Value::Null),
        "warnings": output.get("final_warnings").cloned().unwrap_or_else(|| json!([])),
        "suggested_next_actions": output.get("suggested_next_actions").cloned().unwrap_or_else(|| json!([])),
    });
    apply_compact_workflow_outcomes(
        &mut compact,
        true,
        Some(hygiene_checked),
        resolved_unexpected_validation_failures,
    );
    let verdict = compact.get("verdict").cloned().unwrap_or_else(|| json!({}));
    compact["suggested_next_actions"] = json!(merged_suggested_next_actions(&compact, &verdict));
    compact
        .as_object_mut()
        .expect("compact finish output is an object")
        .remove("verdict");
    compact
}

fn startup_verdict(
    output: &Value,
    active_jobs: &Value,
    owning_runner_available: Option<bool>,
    runtime_status_call_failed: bool,
    tool_manifest_requested: bool,
) -> Value {
    let mut checks = Vec::new();
    let mut actions: Vec<String> = Vec::new();

    push_startup_check(
        &mut checks,
        "runtime_status",
        runtime_status_check(output, runtime_status_call_failed),
    );
    push_startup_check(&mut checks, "workspace", workspace_check(output));
    push_startup_check(&mut checks, "jobs", startup_jobs_check(active_jobs));
    push_startup_check(
        &mut checks,
        "agent",
        startup_agent_check(output, owning_runner_available),
    );
    push_startup_check(
        &mut checks,
        "tool_manifest",
        startup_tool_manifest_check(output, tool_manifest_requested),
    );

    for check in &checks {
        match check.get("reason").and_then(Value::as_str) {
            Some("runtime_status_call_failed") => {
                push_unique_action(&mut actions, "inspect runtime_status directly")
            }
            Some("workspace_dirty") => push_unique_action(
                &mut actions,
                "inspect existing worktree changes with show_changes and preserve them while editing",
            ),
            Some("workspace_conflicts") => push_unique_action(
                &mut actions,
                "review merge/rebase conflicts carefully; do not reset or overwrite conflict markers unless resolving them",
            ),
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
    runtime_status_call_failed: bool,
) -> (&'static str, Option<&'static str>) {
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

fn workspace_check(output: &Value) -> (&'static str, Option<&'static str>) {
    let git = output.get("git").unwrap_or(&Value::Null);
    if git.get("available").and_then(Value::as_bool) == Some(false) {
        return ("warn", Some("git_unavailable"));
    }
    // Ordinary tracked/staged/untracked edits are expected development state.
    // An unresolved merge/rebase conflict is a deterministic blocker until it
    // is resolved; the session itself remains usable for inspection and repair.
    let conflicted = git
        .pointer("/counts/conflicted")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if conflicted > 0 {
        return ("fail", Some("workspace_conflicts"));
    }
    match git.get("clean").and_then(Value::as_bool) {
        Some(true) => ("pass", None),
        Some(false) => ("warn", Some("workspace_dirty")),
        None => ("warn", Some("workspace_unknown")),
    }
}

fn startup_jobs_check(jobs: &Value) -> (&'static str, Option<&'static str>) {
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
    owning_runner_available: Option<bool>,
) -> (&'static str, Option<&'static str>) {
    let executor = output
        .pointer("/resolved_project/executor")
        .and_then(Value::as_str);
    match (executor, owning_runner_available) {
        (Some("agent"), Some(false)) => ("fail", Some("agent_offline")),
        (Some("agent"), Some(true)) => ("pass", None),
        (Some("agent"), None) => ("warn", Some("agent_health_unknown")),
        (Some("local"), _) => ("pass", None),
        _ => ("warn", Some("agent_health_unknown")),
    }
}

fn owning_runner_available(
    resolved: &ResolvedProject,
    runtime_status: &Value,
    runtime_status_call_failed: bool,
) -> Option<bool> {
    if !resolved.config.is_agent() {
        return Some(true);
    }
    if runtime_status_call_failed {
        return None;
    }
    Some(
        runtime_status
            .pointer("/agents/summary/clients")
            .and_then(Value::as_array)
            .and_then(|clients| {
                clients.iter().find(|client| {
                    client.get("client_id").and_then(Value::as_str)
                        == Some(resolved.config.client_id.as_str())
                })
            })
            .and_then(|client| client.get("status").and_then(Value::as_str))
            == Some("online"),
    )
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

fn merged_suggested_next_actions(output: &Value, verdict: &Value) -> Vec<String> {
    let mut actions = string_array(output.get("suggested_next_actions"));
    for action in string_array(verdict.get("suggested_next_actions")) {
        push_unique_action(&mut actions, &action);
    }
    actions
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn finish_suggested_next_actions(
    output: &Value,
    resolved_unexpected_validation_failures: usize,
) -> Vec<String> {
    let mut actions = Vec::new();
    let push = |actions: &mut Vec<String>, action: &str| {
        if !actions.iter().any(|existing| existing == action) {
            actions.push(action.to_string());
        }
    };
    let tool_failures = output.get("tool_failures").unwrap_or(&Value::Null);
    let expectation_mismatch_count = tool_failures
        .get("expectation_mismatch_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unexpected_success_count = tool_failures
        .get("unexpected_success_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    if unresolved_unexpected_failure_count(tool_failures, resolved_unexpected_validation_failures)
        > 0
    {
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
    if validation_has_cargo_test_zero_tests(output.get("validation").unwrap_or(&Value::Null)) {
        push(
            &mut actions,
            "cargo_test ran zero tests; verify the test filter or command",
        );
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
        "conflicted",
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
        let conflicted = workspace
            .pointer("/counts/conflicted")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let message = if conflicted > 0 {
            "workspace has merge/rebase conflicts; inspect and preserve existing worktree state"
        } else {
            "workspace has existing tracked or untracked changes; inspect and preserve them while editing"
        };
        warnings.push(json!({
            "kind": "dirty_worktree",
            "changed_files_count": workspace
                .get("changed_files_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "conflicted": conflicted,
            "message": message,
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

#[cfg(test)]
mod startup_runner_tests {
    use super::*;
    use crate::projects::ProjectConfig;

    fn resolved_agent(client_id: &str) -> ResolvedProject {
        ResolvedProject {
            input: "demo".to_string(),
            resolved_id: format!("agent:{client_id}:demo"),
            config: ProjectConfig {
                path: "/tmp/demo".to_string(),
                client_id: client_id.to_string(),
                allow_patch: true,
            },
        }
    }

    #[test]
    fn missing_target_runner_is_unavailable_even_when_a_peer_is_online() {
        let runtime_status = json!({
            "agents": {
                "summary": {
                    "clients": [{"client_id": "peer", "status": "online"}]
                }
            }
        });
        assert_eq!(
            owning_runner_available(&resolved_agent("target"), &runtime_status, false),
            Some(false)
        );
    }

    #[test]
    fn target_runner_online_is_available_even_when_a_peer_is_stale() {
        let runtime_status = json!({
            "agents": {
                "summary": {
                    "clients": [
                        {"client_id": "peer", "status": "stale"},
                        {"client_id": "target", "status": "online"}
                    ]
                }
            }
        });
        assert_eq!(
            owning_runner_available(&resolved_agent("target"), &runtime_status, false),
            Some(true)
        );
    }
}

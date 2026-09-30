//! Project selection, exact Session bindings, and authorized registration.

use super::*;

#[derive(Debug, Clone)]
pub(super) struct ManagedWorktreeRequest {
    pub(super) base_ref: Option<String>,
    pub(super) operation_id: String,
    pub(super) resume_project_id: Option<String>,
}

pub(super) enum CodingProjectSource {
    Existing { project: String },
    RunnerPath { client_id: String, path: String },
}
pub(super) fn invalid_project_source(message: impl Into<String>, fields: Value) -> ToolResult {
    let mut output = json!({
        "error_kind": "invalid_arguments",
        "failure_kind": "invalid_arguments",
        "constraint": "exactly_one_project_source",
        "state_changed": false,
    });
    if let (Some(output), Some(fields)) = (output.as_object_mut(), fields.as_object()) {
        output.extend(fields.clone());
    }
    ToolResult::err_with_output(message, output).with_recovery(RecoveryKind::FixInput)
}

#[cfg(test)]
#[test]
fn invalid_project_source_exposes_fix_input_recovery() {
    let result = invalid_project_source("choose one project source", json!({"project": "demo"}));
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "invalid_arguments");
    assert_eq!(result.output["failure_kind"], "invalid_arguments");
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["recovery_kind"], "fix_input");
    assert!(result.output.get("recovery_tool").is_none());
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

pub(super) fn resolve_project_source(
    project: String,
    client_id: Option<String>,
    path: Option<String>,
) -> Result<CodingProjectSource, ToolResult> {
    let project = project.trim().to_string();
    let client_id = non_empty_optional_field("client_id", client_id)?;
    let path = non_empty_optional_field("path", path)?;

    if !project.is_empty() {
        let mut conflicts = Vec::new();
        if client_id.is_some() {
            conflicts.push("client_id");
        }
        if path.is_some() {
            conflicts.push("path");
        }
        if !conflicts.is_empty() {
            let mut fields = vec!["project"];
            fields.extend(conflicts);
            return Err(invalid_project_source(
                "project cannot be combined with client_id or path",
                json!({"conflicting_fields": fields}),
            ));
        }
        return Ok(CodingProjectSource::Existing { project });
    }

    if let Some(path) = path {
        if let Err(error) = crate::tool_runtime::projects::validate_project_op_path(&path) {
            return Err(invalid_project_source(
                error,
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

    Err(if client_id.is_some() {
        invalid_project_source(
            "client_id requires path",
            json!({"field": "path", "required_with": "client_id"}),
        )
    } else {
        invalid_project_source(
            "project or client_id + path is required",
            json!({"required_any_of": ["project", "client_id + path"]}),
        )
    })
}

fn registration_scope_denied(auth: Option<&AuthContext>, operation: &str) -> Option<ToolResult> {
    auth.is_some_and(|auth| !auth.has_scope(crate::auth::SCOPE_PROJECT_WRITE))
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
            .with_recovery(RecoveryKind::UserAction)
        })
}

fn runner_coding_capability_error(client_id: &str, error: String) -> ToolResult {
    if error.contains("unknown shell client") {
        return ToolResult::err_with_output(
            format!("Runner client_id is unknown or not visible: {client_id}"),
            json!({
                "error_kind": "unknown_runner",
                "failure_kind": "unknown_runner",
                "client_id": client_id,
                "state_changed": false,
                "suggested_call": crate::tool_runtime::SuggestedToolCall::fallback_recovery(
                    "list_runners",
                    json!({
                        "include_projects": false,
                        "summary_only": true,
                    }),
                ).to_value()
            }),
        );
    }
    ToolResult::err(error)
}

pub(super) fn attach_permission(
    mut result: ToolResult,
    permission: Option<&PermissionDecision>,
) -> ToolResult {
    if let Some(permission) = permission {
        crate::tool_runtime::permissions::add_permission_to_result(&mut result, permission);
    }
    result
}

pub(super) fn attach_project_resolution(
    mut result: ToolResult,
    resolution: &ProjectResolutionMetadata,
) -> ToolResult {
    // Existing-project aliases are not authoritative until runtime resolution
    // succeeds. Path sources already carry a Runner-issued full id, so their
    // metadata remains useful on later Session failures.
    if !resolution.resolved_project.is_empty() {
        result.output["project_resolution"] =
            serde_json::to_value(resolution).unwrap_or_else(|_| json!({}));
    }
    if resolution.registered && !result.success {
        result.output["state_changed"] = json!(true);
    }
    attach_permission(result, resolution.permission.as_ref())
}

fn project_resolution_from_runner_result(
    resolved: ToolResult,
    managed_requested: bool,
    permission: Option<PermissionDecision>,
) -> Result<(String, ProjectResolutionMetadata), ToolResult> {
    if !resolved.success {
        return Err(attach_permission(resolved, permission.as_ref()));
    }
    let Some(project) = resolved
        .output
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
    else {
        return Err(attach_permission(
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
        ));
    };
    let outcome = resolved
        .output
        .get("outcome")
        .and_then(Value::as_str)
        .filter(|outcome| {
            if managed_requested {
                matches!(
                    *outcome,
                    "managed_worktree_created" | "managed_worktree_recovered"
                )
            } else {
                matches!(*outcome, "reused_existing_registration" | "auto_registered")
            }
        })
        .map(str::to_string);
    let registered = resolved.output.get("registered").and_then(Value::as_bool);
    let (Some(outcome), Some(registered)) = (outcome, registered) else {
        return Err(attach_permission(
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
        ));
    };
    if !managed_requested && registered != (outcome == "auto_registered") {
        return Err(attach_permission(
            ToolResult::err_with_output(
                "Runner returned inconsistent path resolution metadata",
                json!({
                    "error_kind": "operation_failed",
                    "failure_kind": "operation_failed",
                    "state_changed": registered,
                }),
            ),
            permission.as_ref(),
        ));
    }
    let worktree = if managed_requested {
        let base_ref = resolved
            .output
            .get("base_ref")
            .and_then(Value::as_str)
            .map(str::to_string);
        let base_sha = resolved
            .output
            .get("base_sha")
            .and_then(Value::as_str)
            .filter(|sha| {
                matches!(sha.len(), 40 | 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .map(str::to_string);
        let source_dirty = resolved.output.get("source_dirty").and_then(Value::as_bool);
        let managed = resolved.output.get("managed").and_then(Value::as_bool);
        match (managed, base_ref, base_sha, source_dirty) {
            (Some(true), Some(base_ref), Some(base_sha), Some(source_dirty)) => {
                Some(ManagedWorktreeProjection {
                    managed: true,
                    base_ref,
                    base_sha,
                    source_dirty,
                })
            }
            _ => {
                return Err(attach_permission(
                    ToolResult::err_with_output(
                        "Runner returned malformed managed worktree metadata",
                        json!({
                            "error_kind": "operation_failed",
                            "failure_kind": "operation_failed",
                            "state_changed": true,
                        }),
                    ),
                    permission.as_ref(),
                ));
            }
        }
    } else {
        None
    };
    let resolution = ProjectResolutionMetadata {
        source: if managed_requested {
            "managed_worktree".to_string()
        } else {
            "path".to_string()
        },
        outcome,
        resolved_project: project.clone(),
        registered,
        worktree,
        permission,
    };
    Ok((project, resolution))
}

impl ToolRuntime {
    async fn require_runner_coding_capability(
        &self,
        client_id: &str,
        auth: Option<&AuthContext>,
    ) -> Result<(), ToolResult> {
        let access = crate::runner_http::runner_access_from_auth(auth);
        let supports_shell = self
            .runner_registry
            .runner_supports_for_auth(client_id, RUNNER_CAPABILITY_SHELL, access.as_ref())
            .await
            .map_err(|error| runner_coding_capability_error(client_id, error))?;
        let supports_git = if supports_shell {
            false
        } else {
            self.runner_registry
                .runner_supports_for_auth(client_id, RUNNER_CAPABILITY_GIT, access.as_ref())
                .await
                .map_err(|error| runner_coding_capability_error(client_id, error))?
        };
        if supports_shell || supports_git {
            Ok(())
        } else {
            Err(ToolResult::err(format!(
                "Runner {client_id} does not support shell or git"
            )))
        }
    }

    pub(super) async fn explicit_coding_session_project(
        &self,
        session_id: &str,
        tool_name: &str,
        auth: Option<&AuthContext>,
    ) -> Result<Option<ResolvedProject>, ToolResult> {
        if let Some(resolved) = self
            .authorize_session_target(session_id, tool_name, auth)
            .await?
        {
            return Ok(Some(resolved));
        }
        let Some(project) = self
            .sessions
            .session_project(session_id)
            .expect("authorized Workflow Session must still exist")
        else {
            return Ok(None);
        };
        self.resolve_project_input_for_auth(&project, auth)
            .await
            .map(Some)
            .map_err(|error| error.into_tool_result())
    }

    fn path_source_session_mismatch(
        session_id: &str,
        tool_name: &str,
        session_project: Option<&ResolvedProject>,
        client_id: &str,
        path: &str,
    ) -> Option<ToolResult> {
        let request_project = format!("path:{client_id}:{path}");
        let Some(session_project) = session_project else {
            return Some(session_project_mismatch_result(
                session_id,
                tool_name,
                &SessionProjectMismatch {
                    session_project: "<unscoped>".to_string(),
                    request_project,
                },
            ));
        };
        if session_project.config.client_id != client_id || session_project.config.path != path {
            return Some(session_project_mismatch_result(
                session_id,
                tool_name,
                &SessionProjectMismatch {
                    session_project: session_project.resolved_id.clone(),
                    request_project,
                },
            ));
        }
        None
    }
}

impl ToolRuntime {
    /// Resolve/register the selected project after the caller's exact Session
    /// bindings have been authorized. Registration permissions and Runner fences
    /// stay within this stage; later observations do not gain registration authority.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn resolve_coding_startup_project(
        &self,
        project_source: CodingProjectSource,
        managed_worktree: Option<&ManagedWorktreeRequest>,
        resume_session_id: Option<&str>,
        resume_session_project: Option<&ResolvedProject>,
        trusted_recording_session_id: Option<&str>,
        trusted_recording_session_resolved_project: Option<&ResolvedProject>,
        tool_name: &str,
        auth: Option<&AuthContext>,
    ) -> Result<(String, ProjectResolutionMetadata), ToolResult> {
        Ok(match project_source {
            CodingProjectSource::Existing { project } => {
                if let Some(worktree) = managed_worktree {
                    let source = match self.resolve_project_input_for_auth(&project, auth).await {
                        Ok(source) => source,
                        Err(error) => return Err(error.into_tool_result()),
                    };
                    if let Some(session_id) = resume_session_id {
                        return Err(session_project_mismatch_result(
                            session_id,
                            tool_name,
                            &SessionProjectMismatch {
                                session_project: resume_session_project
                                    .map(|project| project.resolved_id.clone())
                                    .unwrap_or_else(|| "<unscoped>".to_string()),
                                request_project: format!(
                                    "managed_worktree_from:{}",
                                    source.resolved_id
                                ),
                            },
                        ));
                    }
                    if let Some(recording_session_id) = trusted_recording_session_id {
                        return Err(session_project_mismatch_result(
                            recording_session_id,
                            tool_name,
                            &SessionProjectMismatch {
                                session_project: trusted_recording_session_resolved_project
                                    .map(|project| project.resolved_id.clone())
                                    .unwrap_or_else(|| "<unscoped>".to_string()),
                                request_project: format!(
                                    "managed_worktree_from:{}",
                                    source.resolved_id
                                ),
                            },
                        ));
                    }
                    if let Some(result) =
                        registration_scope_denied(auth, "managed worktree project registration")
                    {
                        return Err(result);
                    }
                    let permission = crate::tool_runtime::permissions::evaluate_permission_for_tool(
                        &self.permission_evaluator,
                        "register_project",
                        None,
                    );
                    if let Some(decision) = permission.as_ref() {
                        if !decision.allows_execution() {
                            let mut result =
                                crate::tool_runtime::permissions::permission_execution_denied_result(decision);
                            crate::tool_runtime::permissions::add_permission_to_result(
                                &mut result,
                                decision,
                            );
                            return Err(result);
                        }
                    }
                    if let Err(result) = self
                        .require_runner_coding_capability(&source.config.client_id, auth)
                        .await
                    {
                        return Err(attach_permission(result, permission.as_ref()));
                    }
                    let Some(source_project_id) =
                        crate::tool_runtime::lsp_tools::runner_local_project_id(
                            &source.resolved_id,
                        )
                        .map(str::to_string)
                    else {
                        return Err(attach_permission(
                            ToolResult::err_with_output(
                                "managed worktree source must resolve to one Runner project",
                                json!({
                                    "error_kind": "managed_worktree_source_identity_unavailable",
                                    "failure_kind": "operation_failed",
                                    "source_project": source.resolved_id,
                                    "state_changed": false,
                                }),
                            )
                            .with_recovery(RecoveryKind::Reobserve),
                            permission.as_ref(),
                        ));
                    };
                    let Some(source_root_fingerprint) = source.root_fingerprint.clone() else {
                        return Err(attach_permission(
                            ToolResult::err_with_output(
                                "managed worktree source root identity is unavailable",
                                json!({
                                    "error_kind": "managed_worktree_source_identity_unavailable",
                                    "failure_kind": "operation_failed",
                                    "source_project": source.resolved_id,
                                    "state_changed": false,
                                }),
                            )
                            .with_recovery(RecoveryKind::Reobserve),
                            permission.as_ref(),
                        ));
                    };
                    let source_runtime_id = source.resolved_id.clone();
                    let mut resolved = self
                        .prepare_managed_worktree(
                            source.config.client_id.clone(),
                            source.config.path.clone(),
                            worktree.base_ref.clone(),
                            worktree.operation_id.clone(),
                            None,
                            Some(source_project_id),
                            Some(source_root_fingerprint.clone()),
                            auth,
                        )
                        .await;
                    if !resolved.success {
                        // Root fingerprints and Runner-local source ids are internal
                        // identity fences, not model-facing recovery inputs. Keep the
                        // already-authorized canonical source Project for diagnostics,
                        // but scrub internal identity material from Runner failures.
                        if let Some(output) = resolved.output.as_object_mut() {
                            output.remove("source_project_id");
                            output.remove("source_root_fingerprint");
                        }
                        resolved.output["source_project"] = json!(source_runtime_id);
                    }
                    match project_resolution_from_runner_result(resolved, true, permission) {
                        Ok(resolved) => resolved,
                        Err(result) => return Err(result),
                    }
                } else {
                    let resolution = ProjectResolutionMetadata {
                        source: "project".to_string(),
                        outcome: "resolved_existing_project".to_string(),
                        resolved_project: String::new(),
                        registered: false,
                        worktree: None,
                        permission: None,
                    };
                    (project, resolution)
                }
            }
            CodingProjectSource::RunnerPath { client_id, path } => {
                if managed_worktree.is_none() {
                    if let Some(session_id) = resume_session_id {
                        if let Some(result) = Self::path_source_session_mismatch(
                            session_id,
                            tool_name,
                            resume_session_project,
                            &client_id,
                            &path,
                        ) {
                            return Err(result);
                        }
                    }
                    if let Some(recording_session_id) = trusted_recording_session_id {
                        if let Some(result) = Self::path_source_session_mismatch(
                            recording_session_id,
                            tool_name,
                            trusted_recording_session_resolved_project,
                            &client_id,
                            &path,
                        ) {
                            return Err(result);
                        }
                    }
                }
                if let Some(result) = registration_scope_denied(auth, "project path registration") {
                    return Err(result);
                }
                if let Err(result) = self
                    .project_scoped_visible_project_for_exact_path(&client_id, &path, auth)
                    .await
                {
                    return Err(result);
                }
                let permission = crate::tool_runtime::permissions::evaluate_permission_for_tool(
                    &self.permission_evaluator,
                    "register_project",
                    None,
                );
                if let Some(decision) = permission.as_ref() {
                    if !decision.allows_execution() {
                        let mut result =
                            crate::tool_runtime::permissions::permission_execution_denied_result(
                                decision,
                            );
                        crate::tool_runtime::permissions::add_permission_to_result(
                            &mut result,
                            decision,
                        );
                        return Err(result);
                    }
                }
                if let Err(result) = self
                    .require_runner_coding_capability(&client_id, auth)
                    .await
                {
                    return Err(attach_permission(result, permission.as_ref()));
                }
                let managed_requested = managed_worktree.is_some();
                if let (Some(worktree), Some(session_project), Some(session_id)) =
                    (managed_worktree, resume_session_project, resume_session_id)
                {
                    if session_project.config.client_id != client_id {
                        return Err(session_project_mismatch_result(
                            session_id,
                            tool_name,
                            &SessionProjectMismatch {
                                session_project: session_project.resolved_id.clone(),
                                request_project: format!("managed_worktree:{client_id}"),
                            },
                        ));
                    }
                    debug_assert!(worktree.resume_project_id.is_some());
                }
                let resolved = if let Some(worktree) = managed_worktree {
                    self.prepare_managed_worktree(
                        client_id,
                        path,
                        worktree.base_ref.clone(),
                        worktree.operation_id.clone(),
                        worktree.resume_project_id.clone(),
                        None,
                        None,
                        auth,
                    )
                    .await
                } else {
                    self.resolve_or_register_project(client_id, path, auth)
                        .await
                };
                match project_resolution_from_runner_result(resolved, managed_requested, permission)
                {
                    Ok(resolved) => resolved,
                    Err(result) => return Err(result),
                }
            }
        })
    }
}

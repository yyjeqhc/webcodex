use super::model_ergonomics_telemetry::{ModelErgonomicsCompletion, ModelErgonomicsTimer};
use super::sessions::{
    strip_tool_call_expectation_metadata, SessionTransport, ToolCallRecorderMetadata,
};
use super::tool_audit::{session_log_arguments_for_tool_request, session_log_result_for_tool};
use super::{
    session_context, session_guard_denied_result, tool_disabled_result_from_definition, ToolCall,
    ToolResult, ToolRuntime, ALLOW_CROSS_PROJECT_SESSION_FIELD,
};
use crate::auth::scopes::OAuthToolScopePolicy;
use crate::auth::AuthContext;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolTransport {
    Api,
    Mcp,
}

impl From<ToolTransport> for SessionTransport {
    fn from(value: ToolTransport) -> Self {
        match value {
            ToolTransport::Api => SessionTransport::Api,
            ToolTransport::Mcp => SessionTransport::Mcp,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum HostFileImportTrust {
    #[default]
    Untrusted,
    TrustedOAuthClient,
}

impl HostFileImportTrust {
    pub(crate) fn is_trusted(self) -> bool {
        matches!(self, Self::TrustedOAuthClient)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolCallContext<'a> {
    pub(crate) transport: ToolTransport,
    pub(crate) session_id: Option<&'a str>,
    pub(crate) auth: Option<&'a AuthContext>,
    pub(crate) window: Option<&'a crate::client_window::ClientWindow>,
    /// REST records scope denials with session metadata. MCP rejects scope
    /// denials before `_session_id` becomes recorder metadata. Keep both
    /// adapter-visible behaviors stable.
    pub(crate) record_oauth_scope_denials: bool,
    /// Server-derived provenance for ChatGPT host file references. Raw tool
    /// arguments cannot set this value.
    pub(crate) host_file_import_trust: HostFileImportTrust,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolCallRequest {
    pub(crate) tool_name: String,
    pub(crate) arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolCallErrorStatus {
    InvalidArguments {
        message: String,
    },
    InsufficientScope {
        required_scope: Option<&'static str>,
        description: String,
    },
}

#[derive(Debug)]
pub(crate) struct ToolCallOutcome {
    pub(crate) success: bool,
    pub(crate) result: Option<ToolResult>,
    pub(crate) error_status: Option<ToolCallErrorStatus>,
    pub(crate) project: Option<String>,
    pub(crate) model_ergonomics: Option<ModelErgonomicsCompletion>,
}

pub(crate) fn check_runtime_tool_scope(
    auth: Option<&AuthContext>,
    tool_name: &str,
) -> Result<(), ToolCallErrorStatus> {
    let Some(auth) = auth else {
        return Ok(());
    };

    match crate::auth::scopes::oauth_scope_policy_for_runtime_tool(tool_name) {
        OAuthToolScopePolicy::Require(scope) => {
            if auth.has_scope(scope) {
                Ok(())
            } else {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: Some(scope),
                    description: format!("missing required scope: {}", scope),
                })
            }
        }
        OAuthToolScopePolicy::RequireAll(scopes) => {
            if let Some(scope) = scopes.iter().copied().find(|scope| !auth.has_scope(scope)) {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: Some(scope),
                    description: format!("missing required scope: {}", scope),
                })
            } else {
                Ok(())
            }
        }
        OAuthToolScopePolicy::FirstPartyOnly => {
            if matches!(
                auth.kind,
                crate::auth::AuthKind::Bootstrap | crate::auth::AuthKind::ApiToken
            ) {
                Ok(())
            } else {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: None,
                    description: "tool requires a first-party bootstrap or personal API token"
                        .to_string(),
                })
            }
        }
        OAuthToolScopePolicy::Unknown => {
            if auth.is_bootstrap() {
                Ok(())
            } else {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: None,
                    description: "runtime tool has no declared scope policy".to_string(),
                })
            }
        }
    }
}

impl ToolRuntime {
    pub(crate) async fn call_tool_with_context(
        &self,
        request: ToolCallRequest,
        context: ToolCallContext<'_>,
    ) -> ToolCallOutcome {
        let telemetry = ModelErgonomicsTimer::start(&request.tool_name);
        let mut outcome = self.call_tool_with_context_inner(request, context).await;
        outcome.model_ergonomics = telemetry.map(ModelErgonomicsTimer::finish);
        outcome
    }

    async fn call_tool_with_context_inner(
        &self,
        request: ToolCallRequest,
        context: ToolCallContext<'_>,
    ) -> ToolCallOutcome {
        let recorder_metadata = ToolCallRecorderMetadata::from_arguments(&request.arguments);
        let concrete_arguments = strip_tool_call_expectation_metadata(request.arguments.clone());
        let allow_cross_project_session =
            extract_bool_arg(&concrete_arguments, ALLOW_CROSS_PROJECT_SESSION_FIELD);
        // A wrapper recording_session_id is authority-bearing internal context,
        // not a ledger address. Authorize it before any lifecycle/guard lookup,
        // project mismatch computation, provenance derivation, or ledger write.
        if let Some(recorder_session_id) = context.session_id {
            if let Err(mut result) = self
                .authorize_session_target(recorder_session_id, &request.tool_name, context.auth)
                .await
            {
                super::dispatch::decorate_structured_execution_prestart_denial(
                    &request.tool_name,
                    &mut result,
                    "session_authority_denied",
                );
                return ToolCallOutcome {
                    success: false,
                    result: Some(result),
                    error_status: None,
                    project: None,
                    model_ergonomics: None,
                };
            }
        }
        if collaboration_session_tool(&request.tool_name) {
            if let (Some(recorder_session_id), Some(target_session_id)) = (
                context.session_id,
                collaboration_target_session_id(&request.tool_name, &concrete_arguments),
            ) {
                // Both ends of a cross-Session collaboration relationship must
                // be independently authorized before comparing scope. This makes
                // None/None safe while rejecting either mixed scoped/unscoped
                // direction and never consulting allow_cross_project_session.
                if let Err(result) = self
                    .authorize_session_target(target_session_id, &request.tool_name, context.auth)
                    .await
                {
                    return ToolCallOutcome {
                        success: false,
                        result: Some(result),
                        error_status: None,
                        project: None,
                        model_ergonomics: None,
                    };
                }
                let recorder_project = self
                    .sessions
                    .session_project(recorder_session_id)
                    .expect("authorized recording Session must exist");
                let target_project = self
                    .sessions
                    .session_project(target_session_id)
                    .expect("authorized collaboration target Session must exist");
                if recorder_project != target_project {
                    let result = session_context::session_project_mismatch_no_escape_result(
                        target_session_id,
                        &request.tool_name,
                        &session_context::SessionProjectMismatch {
                            session_project: target_project
                                .unwrap_or_else(|| "<unscoped>".to_string()),
                            request_project: recorder_project
                                .unwrap_or_else(|| "<unscoped>".to_string()),
                        },
                    );
                    return ToolCallOutcome {
                        success: false,
                        result: Some(result),
                        error_status: None,
                        project: None,
                        model_ergonomics: None,
                    };
                }
            }
        }
        let mut recording_session_project_mismatch = match context.session_id {
            Some(session_id) => {
                self.recording_session_project_mismatch(
                    session_id,
                    &request.tool_name,
                    &concrete_arguments,
                    context.auth,
                )
                .await
            }
            None => None,
        };
        if let (Some(session_id), Some(mismatch)) = (
            context.session_id,
            recording_session_project_mismatch.as_ref(),
        ) {
            if !allow_cross_project_session
                && session_context::session_project_mismatch_requires_escape(&request.tool_name)
            {
                let session_event = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    context.transport.into(),
                    &request.tool_name,
                    &session_log_arguments_for_tool_request(
                        &request.tool_name,
                        &concrete_arguments,
                    ),
                    Some(mismatch.request_project.clone()),
                    recorder_metadata.clone(),
                );
                let mut result = session_context::session_project_mismatch_result(
                    session_id,
                    &request.tool_name,
                    mismatch,
                );
                super::dispatch::decorate_structured_execution_prestart_denial(
                    &request.tool_name,
                    &mut result,
                    session_context::SESSION_PROJECT_MISMATCH_KIND,
                );
                let event_id = self.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some(session_context::SESSION_PROJECT_MISMATCH_KIND),
                );
                super::add_session_telemetry_hint(
                    &mut result,
                    &self.sessions,
                    session_id,
                    event_id,
                );
                return ToolCallOutcome {
                    success: false,
                    result: Some(result),
                    error_status: None,
                    project: None,
                    model_ergonomics: None,
                };
            }
        }
        if let Some(mut result) = tool_disabled_result_from_definition(&request.tool_name) {
            super::dispatch::decorate_structured_execution_prestart_denial(
                &request.tool_name,
                &mut result,
                "capability_unavailable",
            );
            if let Some(session_id) = context.session_id {
                let session_event = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    context.transport.into(),
                    &request.tool_name,
                    &session_log_arguments_for_tool_request(
                        &request.tool_name,
                        &concrete_arguments,
                    ),
                    None,
                    recorder_metadata.clone(),
                );
                let event_id = self.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some("tool_disabled"),
                );
                super::add_session_telemetry_hint(
                    &mut result,
                    &self.sessions,
                    session_id,
                    event_id,
                );
            }
            return ToolCallOutcome {
                success: false,
                result: Some(result),
                error_status: None,
                project: None,
                model_ergonomics: None,
            };
        }
        if let Some(session_id) = context.session_id {
            // Lifecycle denial wins before mode/guards: Closed is orthogonal to
            // read_only and must not be confused with session_guard_denied.
            if let Some(denial) = self
                .sessions
                .lifecycle_denial(session_id, &request.tool_name)
            {
                let session_event = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    context.transport.into(),
                    &request.tool_name,
                    &session_log_arguments_for_tool_request(
                        &request.tool_name,
                        &concrete_arguments,
                    ),
                    None,
                    recorder_metadata.clone(),
                );
                let mut result = session_context::session_lifecycle_denied_result(
                    session_id,
                    &request.tool_name,
                    denial,
                );
                super::dispatch::decorate_structured_execution_prestart_denial(
                    &request.tool_name,
                    &mut result,
                    "session_lifecycle_denied",
                );
                let error_kind = result
                    .output
                    .get("error_kind")
                    .and_then(Value::as_str)
                    .unwrap_or("session_closed");
                let event_id = self.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some(error_kind),
                );
                super::add_session_telemetry_hint(
                    &mut result,
                    &self.sessions,
                    session_id,
                    event_id,
                );
                return ToolCallOutcome {
                    success: false,
                    result: Some(result),
                    error_status: None,
                    project: None,
                    model_ergonomics: None,
                };
            }
            if let Some(denial) = self.sessions.guard_denial(session_id, &request.tool_name) {
                let session_event = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    context.transport.into(),
                    &request.tool_name,
                    &session_log_arguments_for_tool_request(
                        &request.tool_name,
                        &concrete_arguments,
                    ),
                    None,
                    recorder_metadata.clone(),
                );
                let mut result =
                    session_guard_denied_result(session_id, &request.tool_name, denial);
                super::dispatch::decorate_structured_execution_prestart_denial(
                    &request.tool_name,
                    &mut result,
                    "session_guard_denied",
                );
                let event_id = self.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some("session_guard_denied"),
                );
                super::add_session_telemetry_hint(
                    &mut result,
                    &self.sessions,
                    session_id,
                    event_id,
                );
                return ToolCallOutcome {
                    success: false,
                    result: Some(result),
                    error_status: None,
                    project: None,
                    model_ergonomics: None,
                };
            }
        }

        if !context.record_oauth_scope_denials {
            if let Err(error_status) = check_runtime_tool_scope(context.auth, &request.tool_name) {
                return ToolCallOutcome {
                    success: false,
                    result: None,
                    error_status: Some(error_status),
                    project: None,
                    model_ergonomics: None,
                };
            }
        }

        let session_log_arguments =
            session_log_arguments_for_tool_request(&request.tool_name, &concrete_arguments);
        let mut session_event = self.sessions.record_tool_call_started_with_metadata(
            context.session_id,
            context.transport.into(),
            &request.tool_name,
            &session_log_arguments,
            None,
            recorder_metadata.clone(),
        );

        if context.record_oauth_scope_denials {
            if let Err(error_status) = check_runtime_tool_scope(context.auth, &request.tool_name) {
                let error_message = match &error_status {
                    ToolCallErrorStatus::InsufficientScope { description, .. } => {
                        description.as_str()
                    }
                    ToolCallErrorStatus::InvalidArguments { message } => message.as_str(),
                };
                self.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &Value::Null,
                    Some(error_message),
                    Some("insufficient_scope"),
                );
                return ToolCallOutcome {
                    success: false,
                    result: None,
                    error_status: Some(error_status),
                    project: None,
                    model_ergonomics: None,
                };
            }
        }

        // `work_on_project` can resolve its canonical project only after a
        // Runner path registration/lookup. Preserve enough source information
        // to reconcile an outer recording Session once dispatch returns that
        // resolved project id; explicit `project` inputs are already handled by
        // `recording_session_project_mismatch` above.
        let late_work_on_project_path_mismatch = request.tool_name == "work_on_project"
            && concrete_arguments.as_object().is_some_and(|arguments| {
                arguments
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| !path.trim().is_empty())
                    && arguments
                        .get("client_id")
                        .and_then(Value::as_str)
                        .is_some_and(|client_id| !client_id.trim().is_empty())
                    && !arguments
                        .get("project")
                        .and_then(Value::as_str)
                        .is_some_and(|project| !project.trim().is_empty())
            });
        let mut call = match ToolCall::from_tool_name(&request.tool_name, concrete_arguments) {
            Ok(call) => call,
            Err(message) => {
                self.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &Value::Null,
                    Some(&message),
                    Some("invalid_arguments"),
                );
                return ToolCallOutcome {
                    success: false,
                    result: None,
                    error_status: Some(ToolCallErrorStatus::InvalidArguments { message }),
                    project: None,
                    model_ergonomics: None,
                };
            }
        };
        if let ToolCall::ImportConversationFilesToProject {
            trusted_mcp_host_file_import,
            ..
        } = &mut call
        {
            *trusted_mcp_host_file_import = context.host_file_import_trust.is_trusted();
        }
        if let ToolCall::CompleteSessionMessage {
            trusted_recording_session_id,
            ..
        } = &mut call
        {
            // Private provenance is derived from the already-authorized outer
            // recording Session. Public arguments can never populate this field.
            *trusted_recording_session_id = context.session_id.map(str::to_string);
        }

        let project = tool_project(&call);
        // Permission is evaluated once inside dispatch (pre-exec gate). Kernel
        // only reuses the attached decision for the outer recording session —
        // never re-evaluate (no second request id / inconsistent outcome).
        let inherited_sandbox = context
            .session_id
            .and_then(|session_id| self.sessions.session_mode(session_id))
            .filter(|mode| matches!(mode, crate::tool_runtime::SessionMode::Inspect))
            .map(|_| crate::command_sandbox::INSPECT_SANDBOX_MODE);
        let mut result = self
            .dispatch_with_auth_transport_options_and_metadata_with_sandbox(
                call,
                context.auth,
                context.transport.into(),
                context.session_id.is_none(),
                allow_cross_project_session,
                recorder_metadata,
                inherited_sandbox,
                context.window,
            )
            .await;
        if let Some(start) = session_event.as_mut() {
            if let Some(permission) =
                super::permissions::permission_decision_from_output(&result.output)
            {
                self.sessions.record_permission_decision(start, permission);
            }
        }
        if recording_session_project_mismatch.is_none() && late_work_on_project_path_mismatch {
            if let Some(recording_session_id) = context.session_id {
                let session_project = self
                    .sessions
                    .session_project(recording_session_id)
                    .flatten();
                let request_project = result
                    .output
                    .get("resolved_project")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        result
                            .output
                            .pointer("/project_resolution/resolved_project")
                            .and_then(Value::as_str)
                    })
                    .map(str::trim)
                    .filter(|project| !project.is_empty());
                if let (Some(session_project), Some(request_project)) =
                    (session_project, request_project)
                {
                    if session_project != request_project {
                        recording_session_project_mismatch =
                            Some(session_context::SessionProjectMismatch {
                                session_project,
                                request_project: request_project.to_string(),
                            });
                    }
                }
            }
        }
        if let Some(mismatch) = recording_session_project_mismatch.as_ref() {
            session_context::add_session_project_mismatch_warning(
                &mut result,
                mismatch,
                allow_cross_project_session,
            );
        }
        let session_log_result = session_log_result_for_tool(&request.tool_name, &result.output);
        let outer_event_id = self.sessions.record_tool_call_finished(
            session_event,
            result.success,
            &session_log_result,
            result.error.as_deref(),
            None,
        );
        // When a `recording_session_id` (context.session_id) recorded this
        // generic wrapper call into the tracking session, surface the recorder
        // telemetry hint. This is the only telemetry path for tools like
        // session_summary whose `session_id` is business input rather than a
        // recorder session, so the inner dispatch does not emit it. The hint
        // preserves any existing business `output.session_id`.
        if let Some(session_id) = context.session_id {
            super::add_session_telemetry_hint(
                &mut result,
                &self.sessions,
                session_id,
                outer_event_id,
            );
        }
        ToolCallOutcome {
            success: result.success,
            result: Some(result),
            error_status: None,
            project,
            model_ergonomics: None,
        }
    }

    async fn recording_session_project_mismatch(
        &self,
        session_id: &str,
        _tool_name: &str,
        arguments: &Value,
        auth: Option<&AuthContext>,
    ) -> Option<session_context::SessionProjectMismatch> {
        let session_project = self.sessions.session_project(session_id)??;
        let request_project = arguments
            .as_object()
            .and_then(|obj| obj.get("project"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|project| !project.is_empty())?;
        let resolved = self
            .resolve_project_input_for_auth(request_project, auth)
            .await
            .ok()?;
        if session_project == resolved.resolved_id {
            return None;
        }
        Some(session_context::SessionProjectMismatch {
            session_project,
            request_project: resolved.resolved_id,
        })
    }
}

fn collaboration_session_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "post_session_message"
            | "list_session_messages"
            | "observe_session_messages"
            | "resolve_session_message"
            | "complete_session_message"
            | "session_discussion_summary"
            | "session_handoff_summary"
    )
}

fn collaboration_target_session_id<'a>(tool_name: &str, arguments: &'a Value) -> Option<&'a str> {
    if !collaboration_session_tool(tool_name) {
        return None;
    }
    arguments
        .as_object()
        .and_then(|obj| obj.get("session_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())
}

fn extract_bool_arg(arguments: &Value, key: &str) -> bool {
    arguments
        .as_object()
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn tool_project(call: &ToolCall) -> Option<String> {
    call.project().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthContext, AuthKind};
    use serde_json::json;

    fn test_runtime() -> ToolRuntime {
        ToolRuntime::new_for_tests()
    }

    fn oauth(scopes: &[&str]) -> AuthContext {
        AuthContext {
            user_id: Some("u".to_string()),
            username: Some("alice".to_string()),
            role: Some("user".to_string()),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            token_kind: Some("oauth2".to_string()),
            ..AuthContext::new(AuthKind::OAuth2Token)
        }
    }

    #[tokio::test]
    async fn tool_kernel_records_success_event() {
        let runtime = test_runtime();
        let session = runtime.sessions.start_session(None, None);
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "list_projects".to_string(),
                    arguments: json!({}),
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: Some(&session.session_id),
                    auth: None,
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;

        assert!(outcome.success);
        assert!(outcome.error_status.is_none());
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 1);
        assert_eq!(summary.counts.succeeded, 1);
        assert_eq!(summary.events[0].kind, "tool_call_started");
        assert_eq!(summary.events[1].kind, "tool_call_finished");
        assert_eq!(summary.events[1].status.as_deref(), Some("succeeded"));
    }

    #[tokio::test]
    async fn tool_kernel_records_failure_event() {
        let runtime = test_runtime();
        let session = runtime.sessions.start_session(None, None);
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "read_file".to_string(),
                    arguments: json!({"project": "demo"}),
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: Some(&session.session_id),
                    auth: None,
                    window: None,
                    record_oauth_scope_denials: false,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;

        assert!(!outcome.success);
        assert!(matches!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InvalidArguments { .. })
        ));
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 1);
        assert_eq!(summary.counts.failed, 1);
        let finished = &summary.events[1];
        assert_eq!(finished.transport, "mcp");
        assert_eq!(finished.error_kind.as_deref(), Some("invalid_arguments"));
    }

    #[tokio::test]
    async fn tool_kernel_guard_denial_sanitizes_edit_content() {
        let runtime = test_runtime();
        let session = runtime.sessions.start_session_with_guards(
            None,
            Some("readonly".to_string()),
            crate::tool_runtime::SessionMode::ReadOnly,
            crate::tool_runtime::sessions::SessionGuards::default(),
        );
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "write_project_file".to_string(),
                    arguments: json!({
                        "project": "demo",
                        "path": "README.md",
                        "content": "secret-content"
                    }),
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: Some(&session.session_id),
                    auth: None,
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;

        assert!(!outcome.success);
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap();
        let serialized = serde_json::to_string(&summary.events).unwrap();
        assert!(serialized.contains("\"content_present\":true"));
        assert!(!serialized.contains("secret-content"));
    }

    #[test]
    fn computer_tools_require_independent_scope() {
        let denied = oauth(&["runtime:read", "project:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&denied), "computer_snapshot"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_READ),
                description: "missing required scope: computer:read".to_string(),
            })
        );
        let allowed = oauth(&["computer:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&allowed), "computer_list_windows"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&allowed), "computer_list_applications"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&allowed), "computer_launch_application"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_LAUNCH),
                description: "missing required scope: computer:launch".to_string(),
            })
        );
        let control_only = oauth(&["computer:control"]);
        assert!(matches!(
            check_runtime_tool_scope(Some(&control_only), "computer_launch_application"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_LAUNCH),
                ..
            })
        ));
        let launch_only = oauth(&["computer:launch"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&launch_only), "computer_launch_application"),
            Ok(())
        );
        assert!(matches!(
            check_runtime_tool_scope(Some(&launch_only), "computer_list_applications"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_READ),
                ..
            })
        ));
        assert_eq!(
            check_runtime_tool_scope(Some(&allowed), "computer_list_targets"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&allowed), "computer_find_elements"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&allowed), "computer_element_state"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&allowed), "list_agents"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_RUNTIME_READ),
                description: "missing required scope: runtime:read".to_string(),
            })
        );
    }

    #[test]
    fn computer_display_observation_requires_read_and_display_read() {
        let read_only = oauth(&["computer:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&read_only), "computer_list_displays"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_DISPLAY_READ),
                description: "missing required scope: computer:display_read".to_string(),
            })
        );
        let display_only = oauth(&["computer:display_read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&display_only), "computer_snapshot_display"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_READ),
                description: "missing required scope: computer:read".to_string(),
            })
        );
        let both = oauth(&["computer:read", "computer:display_read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&both), "computer_list_displays"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&both), "computer_snapshot_display"),
            Ok(())
        );
    }

    #[test]
    fn computer_clipboard_tools_require_independent_dual_scopes() {
        let read_base = oauth(&["computer:read"]);
        assert!(matches!(
            check_runtime_tool_scope(Some(&read_base), "computer_read_clipboard"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_CLIPBOARD_READ),
                ..
            })
        ));
        let clipboard_read_only = oauth(&["computer:clipboard_read"]);
        assert!(matches!(
            check_runtime_tool_scope(Some(&clipboard_read_only), "computer_read_clipboard"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_READ),
                ..
            })
        ));
        let read_both = oauth(&["computer:read", "computer:clipboard_read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&read_both), "computer_read_clipboard"),
            Ok(())
        );
        assert!(check_runtime_tool_scope(Some(&read_both), "computer_write_clipboard").is_err());

        let control_base = oauth(&["computer:control"]);
        assert!(matches!(
            check_runtime_tool_scope(Some(&control_base), "computer_write_clipboard"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_CLIPBOARD_WRITE),
                ..
            })
        ));
        let clipboard_write_only = oauth(&["computer:clipboard_write"]);
        assert!(matches!(
            check_runtime_tool_scope(Some(&clipboard_write_only), "computer_write_clipboard"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_CONTROL),
                ..
            })
        ));
        let write_both = oauth(&["computer:control", "computer:clipboard_write"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&write_both), "computer_write_clipboard"),
            Ok(())
        );
        assert!(check_runtime_tool_scope(Some(&write_both), "computer_read_clipboard").is_err());
    }

    #[test]
    fn computer_pointer_control_requires_all_four_independent_scopes() {
        let cases = [
            (
                vec![
                    "computer:display_read",
                    "computer:control",
                    "computer:pointer_control",
                ],
                crate::auth::SCOPE_COMPUTER_READ,
            ),
            (
                vec![
                    "computer:read",
                    "computer:control",
                    "computer:pointer_control",
                ],
                crate::auth::SCOPE_COMPUTER_DISPLAY_READ,
            ),
            (
                vec![
                    "computer:read",
                    "computer:display_read",
                    "computer:pointer_control",
                ],
                crate::auth::SCOPE_COMPUTER_CONTROL,
            ),
            (
                vec!["computer:read", "computer:display_read", "computer:control"],
                crate::auth::SCOPE_COMPUTER_POINTER_CONTROL,
            ),
        ];
        for tool in ["computer_pointer_move", "computer_pointer_click"] {
            for (scopes, missing) in &cases {
                let context = oauth(scopes);
                assert_eq!(
                    check_runtime_tool_scope(Some(&context), tool),
                    Err(ToolCallErrorStatus::InsufficientScope {
                        required_scope: Some(*missing),
                        description: format!("missing required scope: {missing}"),
                    }),
                    "{tool} missing {missing}"
                );
            }
            let all = oauth(&[
                "computer:read",
                "computer:display_read",
                "computer:control",
                "computer:pointer_control",
            ]);
            assert_eq!(check_runtime_tool_scope(Some(&all), tool), Ok(()));
        }
    }

    #[test]
    fn computer_save_snapshot_requires_project_write_and_computer_read() {
        let read_only = oauth(&["computer:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&read_only), "computer_save_snapshot"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_PROJECT_WRITE),
                description: "missing required scope: project:write".to_string(),
            })
        );
        let write_only = oauth(&["project:write"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&write_only), "computer_save_snapshot"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_READ),
                description: "missing required scope: computer:read".to_string(),
            })
        );
        let both = oauth(&["project:write", "computer:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&both), "computer_save_snapshot"),
            Ok(())
        );
    }

    #[test]
    fn computer_control_requires_its_own_scope() {
        let observe_only = oauth(&["computer:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&observe_only), "computer_control"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_CONTROL),
                description: "missing required scope: computer:control".to_string(),
            })
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&observe_only), "computer_scroll_to_element"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_CONTROL),
                description: "missing required scope: computer:control".to_string(),
            })
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&observe_only), "computer_key_input"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_CONTROL),
                description: "missing required scope: computer:control".to_string(),
            })
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&observe_only), "computer_input_text"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_COMPUTER_CONTROL),
                description: "missing required scope: computer:control".to_string(),
            })
        );
        let control = oauth(&["computer:control"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&control), "computer_control"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&control), "computer_scroll_to_element"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&control), "computer_key_input"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&control), "computer_input_text"),
            Ok(())
        );
    }

    #[test]
    fn tool_scope_enforcement_applies_to_pat_and_direct_shared_key() {
        let mut pat = AuthContext::new(crate::auth::AuthKind::ApiToken);
        pat.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.to_string()];
        assert_eq!(
            check_runtime_tool_scope(Some(&pat), "read_file"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_PROJECT_READ),
                description: "missing required scope: project:read".to_string(),
            })
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&pat), "runtime_status"),
            Ok(())
        );

        let shared = crate::auth::shared_key_context("kernel-scope-matrix");
        assert_eq!(check_runtime_tool_scope(Some(&shared), "read_file"), Ok(()));
        assert_eq!(
            check_runtime_tool_scope(Some(&shared), "computer_snapshot"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&shared), "computer_control"),
            Ok(())
        );
    }

    #[tokio::test]
    async fn tool_kernel_rejects_missing_scope() {
        let runtime = test_runtime();
        let auth = oauth(&["runtime:read"]);
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "read_file".to_string(),
                    arguments: json!({"project": "demo", "path": "README.md"}),
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: None,
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;

        assert!(!outcome.success);
        assert_eq!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_PROJECT_READ),
                description: "missing required scope: project:read".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn tool_kernel_unknown_tool_fails_closed_or_invalid() {
        let runtime = test_runtime();
        let auth = oauth(&["runtime:read", "project:read"]);
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "definitely_not_a_tool".to_string(),
                    arguments: Value::Null,
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: None,
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: false,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;

        assert!(!outcome.success);
        assert!(matches!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InsufficientScope {
                required_scope: None,
                ..
            })
        ));

        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "definitely_not_a_tool".to_string(),
                    arguments: Value::Null,
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: None,
                    auth: None,
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;
        assert!(matches!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InvalidArguments { .. })
        ));
    }
}

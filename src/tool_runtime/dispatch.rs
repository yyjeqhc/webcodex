//! Runtime tool dispatch and session/permission guard flow.

use super::edit_tool_telemetry;
use super::session_context::{
    add_session_project_mismatch_warning, add_session_telemetry_hint, current_session_key,
    current_session_unavailable_result, is_current_session_eligible, session_guard_denied_result,
    session_lifecycle_denied_result, session_project_mismatch_requires_escape,
    session_project_mismatch_result, unknown_session_result, SessionProjectMismatch,
};
use super::{
    permissions, session_context, sessions, tool_disabled_result_from_definition, ToolCall,
    ToolResult, ToolRuntime,
};
use crate::auth::AuthContext;
use crate::tool_runtime::project_resolution::{ProjectResolverError, ResolvedProject};
use serde_json::Value;

/// Add the Phase A lifecycle tuple to a definite pre-execution structured
/// execution denial without changing generic denial helpers used by unrelated
/// tools.
pub(super) fn decorate_structured_execution_prestart_denial(
    tool_name: &str,
    result: &mut ToolResult,
    fallback_failure_kind: &'static str,
) {
    if !matches!(tool_name, "run_process" | "run_script") {
        return;
    }
    let mut output = match std::mem::take(&mut result.output) {
        Value::Object(output) => output,
        other => {
            let mut output = serde_json::Map::new();
            output.insert("value".to_string(), other);
            output
        }
    };
    let failure_kind = output
        .get("failure_kind")
        .and_then(Value::as_str)
        .or_else(|| output.get("error_kind").and_then(Value::as_str))
        .or_else(|| output.get("code").and_then(Value::as_str))
        .unwrap_or(fallback_failure_kind)
        .to_string();
    output.insert(
        "execution_state".to_string(),
        Value::String("not_started".to_string()),
    );
    output.insert("command_started".to_string(), Value::Bool(false));
    output.insert("command_completed".to_string(), Value::Bool(false));
    output.insert("command_ok".to_string(), Value::Bool(false));
    output.insert("exit_code".to_string(), Value::Null);
    output.insert("failure_kind".to_string(), Value::String(failure_kind));
    output.insert("tool_failure".to_string(), Value::Bool(true));
    result.output = Value::Object(output);
}

/// Snapshot of the activity-relevant request facts, captured before the
/// `ToolCall` is moved into execution.
struct WorkspaceActivityContext {
    tool: &'static str,
    project: Option<String>,
    client: Option<String>,
    command: Option<String>,
    paths: Vec<String>,
}

impl ToolRuntime {
    /// Main dispatch — call from MCP handler or GPT Actions handler.
    ///
    /// This no-auth convenience defaults the caller context to `None`, which
    /// means agent-backed tools are rejected (no owner can be proven). HTTP
    /// wrappers should prefer `dispatch_with_auth` so the depot `AuthContext`
    /// is forwarded. Tests use this wrapper for local-executor projects.
    #[cfg(test)]
    pub async fn dispatch(&self, call: ToolCall) -> ToolResult {
        self.dispatch_with_auth(call, None).await
    }

    /// Dispatch carrying the caller's auth context. Agent-backed tools enforce
    /// the owner boundary and capability requirements through
    /// `authorize_agent_tool`; local-executor tools are unaffected. Wrappers
    /// stay thin: they only forward the depot `AuthContext` here.
    pub async fn dispatch_with_auth(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.dispatch_with_auth_transport(call, auth, sessions::SessionTransport::Api)
            .await
    }

    pub(crate) async fn dispatch_with_auth_transport(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options(call, auth, transport, true, false)
            .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        use_current_session: bool,
        allow_cross_project_session: bool,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata(
            call,
            auth,
            transport,
            use_current_session,
            allow_cross_project_session,
            sessions::ToolCallRecorderMetadata::default(),
        )
        .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        use_current_session: bool,
        allow_cross_project_session: bool,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata_with_sandbox(
            call,
            auth,
            transport,
            use_current_session,
            allow_cross_project_session,
            recorder_metadata,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata_with_sandbox(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        use_current_session: bool,
        allow_cross_project_session: bool,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        inherited_sandbox: Option<&'static str>,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        // Phase-1 edit usage telemetry: argument-free structured log only.
        // Does not alter execution, session ledger, Action Audit, or schemas.
        let mut edit_usage = edit_tool_telemetry::start_edit_tool_usage(call.tool_name());
        let result = self
            .dispatch_with_auth_transport_options_and_metadata_inner(
                call,
                auth,
                transport,
                use_current_session,
                allow_cross_project_session,
                recorder_metadata,
                inherited_sandbox,
                window,
            )
            .await;
        if let Some(guard) = edit_usage.as_mut() {
            guard.finish_with_result(&result);
        }
        result
    }

    /// Everything the activity ledger needs from a call, captured before the
    /// call value is moved into execution. `None` for non-mutating tools.
    fn capture_workspace_activity_context(
        call: &ToolCall,
        resolved_project: Option<&str>,
    ) -> Option<WorkspaceActivityContext> {
        let tool = call.tool_name();
        let mutating = super::tool_definition::runtime_tool_is_write_like(tool)
            || super::tool_definition::runtime_tool_is_shell_like(tool);
        if !mutating {
            return None;
        }
        let sanitized = call.session_log_arguments();
        let project = resolved_project.or_else(|| call.project());
        Some(WorkspaceActivityContext {
            tool,
            project: project.map(str::to_string),
            client: project
                .and_then(super::activity::agent_client_from_project)
                .map(str::to_string),
            command: match call {
                ToolCall::RunProcess {
                    executable, args, ..
                } => Some(crate::shell_client::process_preview(
                    executable,
                    args.iter().map(String::as_str),
                )),
                ToolCall::RunScript {
                    language,
                    script,
                    args,
                    ..
                } => Some(crate::shell_client::script_preview(
                    language.as_str(),
                    script.len(),
                    args.len(),
                )),
                _ => call.command_text().map(str::to_string),
            },
            paths: super::activity::paths_from_sanitized_arguments(&sanitized, 16),
        })
    }

    async fn dispatch_with_auth_transport_options_and_metadata_inner(
        &self,
        mut call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        use_current_session: bool,
        allow_cross_project_session: bool,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        inherited_sandbox: Option<&'static str>,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        let project_resolution = match call.project() {
            Some(project) => Some(self.resolve_project_input_for_auth(project, auth).await),
            None => None,
        };
        let resolved_project = project_resolution
            .as_ref()
            .and_then(|resolution| resolution.as_ref().ok());
        // Preserve the canonical project for activity attribution before the
        // session recorder consumes the resolved value below. Short aliases
        // must not turn a real agent execution into a client-less row.
        let activity_project = resolved_project
            .as_ref()
            .map(|resolved| resolved.resolved_id.clone());
        if use_current_session && call.session_id().is_none() && is_current_session_eligible(&call)
        {
            if let (Some(resolved), Some(window)) = (resolved_project.as_ref(), window) {
                match current_session_key(
                    auth,
                    transport,
                    &resolved.resolved_id,
                    &resolved.config.path,
                    Some(window),
                ) {
                    Ok(key) => {
                        if let Some(session_id) = self.sessions.current_session_id(&key) {
                            call = call.with_effective_session_id(session_id);
                        }
                    }
                    Err(message) => {
                        let mut result = current_session_unavailable_result(message);
                        decorate_structured_execution_prestart_denial(
                            call.tool_name(),
                            &mut result,
                            "current_session_unavailable",
                        );
                        return result;
                    }
                }
            }
        }
        // A path-backed work_on_project call must let the Runner resolve or
        // register the canonical project before exact Workflow Session
        // validation. The wrapper delegates all Session checks to
        // start_coding_task, so suppress only the generic pre-dispatch Session
        // path here; the business session_id remains on the ToolCall.
        let defer_path_work_session = matches!(
            &call,
            ToolCall::WorkOnProject {
                project,
                path: Some(path),
                ..
            } if project.trim().is_empty() && !path.trim().is_empty()
        );
        let session_id = if defer_path_work_session {
            None
        } else {
            call.session_id().map(str::to_string)
        };
        if let Some(session_id) = session_id.as_deref() {
            // `work_on_project` owns validation of its public `session_id`
            // argument so malformed ids retain the stable invalid_session_id
            // error instead of being rewritten by the generic lookup guard.
            let malformed_work_on_project_session = matches!(&call, ToolCall::WorkOnProject { .. })
                && !sessions::is_valid_session_id(session_id);
            if !malformed_work_on_project_session && !self.sessions.contains_session(session_id) {
                let mut result = unknown_session_result(session_id);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "unknown_session_id",
                );
                return result;
            }
        }
        let execution_sandbox = inherited_sandbox.or_else(|| {
            session_id
                .as_deref()
                .and_then(|session_id| self.sessions.session_mode(session_id))
                .filter(|mode| matches!(mode, super::SessionMode::Inspect))
                .map(|_| crate::command_sandbox::INSPECT_SANDBOX_MODE)
        });
        let session_project_mismatch = session_id.as_deref().and_then(|session_id| {
            match (
                self.sessions.session_project(session_id),
                resolved_project.as_ref(),
            ) {
                (Some(Some(session_project)), Some(resolved))
                    if session_project != resolved.resolved_id =>
                {
                    Some(SessionProjectMismatch {
                        session_project,
                        request_project: resolved.resolved_id.clone(),
                    })
                }
                _ => None,
            }
        });
        if let (Some(session_id), Some(mismatch)) =
            (session_id.as_deref(), session_project_mismatch.as_ref())
        {
            if !allow_cross_project_session
                && session_project_mismatch_requires_escape(call.tool_name())
            {
                let session_start = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
                    Some(mismatch.request_project.clone()),
                    recorder_metadata.clone(),
                );
                let mut result =
                    session_project_mismatch_result(session_id, call.tool_name(), mismatch);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    session_context::SESSION_PROJECT_MISMATCH_KIND,
                );
                let event_id = self.sessions.record_tool_call_finished(
                    session_start,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some(session_context::SESSION_PROJECT_MISMATCH_KIND),
                );
                add_session_telemetry_hint(&mut result, &self.sessions, session_id, event_id);
                return result;
            }
        }
        // Inherit execution defaults only after exact project matching has
        // been established. Explicit per-call cwd/shell fields remain
        // authoritative; cross-project escape never carries Session context.
        let mut ssh_resource = None;
        if session_project_mismatch.is_none() {
            if let (Some(session_id), Some(resolved)) =
                (session_id.as_deref(), resolved_project.as_ref())
            {
                if let Some(execution_context) = self
                    .sessions
                    .execution_context_for_project(session_id, &resolved.resolved_id)
                {
                    if matches!(
                        &call,
                        ToolCall::RunProcess { .. }
                            | ToolCall::RunScript { .. }
                            | ToolCall::RunShell { .. }
                            | ToolCall::RunJob { .. }
                            | ToolCall::OpenSessionShell { .. }
                            | ToolCall::CargoFmt { .. }
                            | ToolCall::CargoCheck { .. }
                            | ToolCall::CargoTest { .. }
                            | ToolCall::GoTest { .. }
                    ) {
                        ssh_resource = execution_context.resource.clone();
                    }
                    call = call.with_session_execution_context(&execution_context);
                }
            }
        }
        if let Some(mut result) = tool_disabled_result_from_definition(call.tool_name()) {
            decorate_structured_execution_prestart_denial(
                call.tool_name(),
                &mut result,
                "capability_unavailable",
            );
            if let Some(session_id) = session_id.as_deref() {
                let session_start = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
                    None,
                    recorder_metadata.clone(),
                );
                let event_id = self.sessions.record_tool_call_finished(
                    session_start,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some("tool_disabled"),
                );
                add_session_telemetry_hint(&mut result, &self.sessions, session_id, event_id);
            }
            return result;
        }
        if let Some(session_id) = session_id.as_deref() {
            // Lifecycle denial is orthogonal to mode/guards and wins first.
            if let Some(denial) = self.sessions.lifecycle_denial(session_id, call.tool_name()) {
                let session_start = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
                    None,
                    recorder_metadata.clone(),
                );
                let mut result =
                    session_lifecycle_denied_result(session_id, call.tool_name(), denial);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "session_lifecycle_denied",
                );
                let error_kind = result
                    .output
                    .get("error_kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("session_closed");
                let event_id = self.sessions.record_tool_call_finished(
                    session_start,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some(error_kind),
                );
                add_session_telemetry_hint(&mut result, &self.sessions, session_id, event_id);
                return result;
            }
            if let Some(denial) = self.sessions.guard_denial(session_id, call.tool_name()) {
                let session_start = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
                    None,
                    recorder_metadata.clone(),
                );
                let mut result = session_guard_denied_result(session_id, call.tool_name(), denial);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "session_guard_denied",
                );
                let event_id = self.sessions.record_tool_call_finished(
                    session_start,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some("session_guard_denied"),
                );
                add_session_telemetry_hint(&mut result, &self.sessions, session_id, event_id);
                return result;
            }
        }
        let mut session_start = if session_id.is_some() {
            let resolved_project = resolved_project.map(|resolved| resolved.resolved_id.clone());
            self.sessions.record_tool_call_started_with_metadata(
                session_id.as_deref(),
                transport,
                call.tool_name(),
                &call.session_log_arguments(),
                resolved_project,
                recorder_metadata,
            )
        } else {
            None
        };
        if let Err(err) = self
            .authorize_agent_tool(
                &call,
                ssh_resource.as_deref(),
                auth,
                project_resolution.as_ref(),
            )
            .await
        {
            let mut err = err;
            let failure_kind =
                super::process::classify_process_failure(err.error.as_deref().unwrap_or_default());
            decorate_structured_execution_prestart_denial(call.tool_name(), &mut err, failure_kind);
            if let Some(session_id) = session_id.as_deref() {
                let event_id = self.sessions.record_tool_call_finished(
                    session_start,
                    false,
                    &err.output,
                    err.error.as_deref(),
                    None,
                );
                add_session_telemetry_hint(&mut err, &self.sessions, session_id, event_id);
            }
            return err;
        }
        // Authoritative single evaluation (kernel must not re-evaluate).
        // Order: session/auth guards above → permission gate → mutation below.
        // Path/sensitive hard checks still run inside tools; hard-deny filter
        // suppresses permission attach so soft policy never overrides them.
        let permission = self
            .permission_evaluator
            .evaluate(call.tool_name(), call.project());
        if let Some(decision) = permission.as_ref() {
            if !decision.allows_execution() {
                let mut result = permissions::permission_execution_denied_result(decision);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "permission_denied",
                );
                if let Some(start) = session_start.as_mut() {
                    self.sessions
                        .record_permission_decision(start, decision.clone());
                }
                permissions::add_permission_to_result(&mut result, decision);
                if let Some(session_id) = session_id.as_deref() {
                    if let Some(mismatch) = session_project_mismatch.as_ref() {
                        add_session_project_mismatch_warning(
                            &mut result,
                            mismatch,
                            allow_cross_project_session,
                        );
                    }
                    let event_id = self.sessions.record_tool_call_finished(
                        session_start,
                        result.success,
                        &result.output,
                        result.error.as_deref(),
                        None,
                    );
                    add_session_telemetry_hint(&mut result, &self.sessions, session_id, event_id);
                }
                return result;
            }
        }
        let activity_context =
            Self::capture_workspace_activity_context(&call, activity_project.as_deref());
        let tool_name = call.tool_name();
        let mut result = self
            .dispatch_authorized_inner(
                call,
                auth,
                transport,
                execution_sandbox,
                window,
                ssh_resource.as_deref(),
                project_resolution,
            )
            .await;
        let permission = permission.filter(|_| {
            !permissions::is_hard_denied_output(&result.output, result.error.as_deref())
        });
        if let Some(permission) = permission.as_ref() {
            if let Some(start) = session_start.as_mut() {
                self.sessions
                    .record_permission_decision(start, permission.clone());
            }
            permissions::add_permission_to_result(&mut result, permission);
        }
        if let Some(session_id) = session_id.as_deref() {
            if let Some(mismatch) = session_project_mismatch.as_ref() {
                add_session_project_mismatch_warning(
                    &mut result,
                    mismatch,
                    allow_cross_project_session,
                );
            }
            let event_id = self.sessions.record_tool_call_finished(
                session_start,
                result.success,
                &result.output,
                result.error.as_deref(),
                None,
            );
            add_session_telemetry_hint(&mut result, &self.sessions, session_id, event_id);
        }
        if let Some(context) = activity_context {
            self.activity.record(super::activity::ActivityRecord {
                tool: context.tool,
                project: context.project.as_deref(),
                surface: transport.as_str(),
                client: context.client.as_deref(),
                success: result.success,
                session_id: session_id.as_deref(),
                command: context.command.as_deref(),
                paths: context.paths,
                error_summary: result.error.as_deref(),
                // Derived from the verified caller here, not looked up later
                // from whoever holds this client id at read time.
                scope: super::activity::ActivityScope::from_auth(auth),
            });
        }
        if result.success && super::observations::is_meaningful_activity_tool(tool_name) {
            if let Ok((principal_kind, principal_id)) =
                super::session_context::current_session_principal(auth)
            {
                self.observations.record_successful_tool_call(
                    super::observations::ToolCallObservation {
                        principal_kind,
                        principal_id,
                        project: activity_project.clone(),
                        surface: transport.as_str().to_string(),
                        session_id: session_id.clone(),
                        tool: tool_name.to_string(),
                        observed_at: chrono::Utc::now().timestamp(),
                    },
                );
            }
        }
        result
    }

    async fn dispatch_authorized_inner(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        execution_sandbox: Option<&'static str>,
        window: Option<&crate::client_window::ClientWindow>,
        ssh_resource: Option<&str>,
        project_resolution: Option<Result<ResolvedProject, ProjectResolverError>>,
    ) -> ToolResult {
        match call {
            call @ (ToolCall::ListTools { .. }
            | ToolCall::ListAgents
            | ToolCall::RuntimeStatus { .. }
            | ToolCall::ToolManifest { .. }) => self.dispatch_discovery_tool(call, auth).await,

            call @ (ToolCall::StartSession { .. }
            | ToolCall::SessionSummary { .. }
            | ToolCall::UpdateSessionContext { .. }
            | ToolCall::CloseSession { .. }
            | ToolCall::ValidationSummary { .. }
            | ToolCall::PostSessionMessage { .. }
            | ToolCall::ListSessionMessages { .. }
            | ToolCall::ResolveSessionMessage { .. }
            | ToolCall::SessionDiscussionSummary { .. }
            | ToolCall::BindCurrentSession { .. }
            | ToolCall::CurrentSession { .. }
            | ToolCall::UnbindCurrentSession { .. }) => {
                self.dispatch_session_tool(call, auth, transport, window)
                    .await
            }

            call @ (ToolCall::StartCodingTask { .. }
            | ToolCall::WorkOnProject { .. }
            | ToolCall::FinishCodingTask { .. }) => {
                self.dispatch_coding_task_tool(call, auth, transport, window)
                    .await
            }

            call @ ToolCall::SessionHandoffSummary { .. } => {
                self.dispatch_handoff_tool(call, auth).await
            }

            call @ (ToolCall::WorkspaceCheckpointCreate { .. }
            | ToolCall::WorkspaceCheckpointList { .. }
            | ToolCall::WorkspaceCheckpointShow { .. }
            | ToolCall::WorkspaceCheckpointRestore { .. }
            | ToolCall::WorkspaceCheckpointDelete { .. }) => {
                self.dispatch_workspace_checkpoint_tool(call).await
            }

            call @ (ToolCall::ComputerListTargets
            | ToolCall::ComputerListWindows { .. }
            | ToolCall::ComputerAccessibilityStatus { .. }
            | ToolCall::ComputerAccessibilityTree { .. }
            | ToolCall::ComputerFindElements { .. }
            | ToolCall::ComputerElementState { .. }
            | ToolCall::ComputerActivateWindow { .. }
            | ToolCall::ComputerControl { .. }
            | ToolCall::ComputerScrollToElement { .. }
            | ToolCall::ComputerKeyInput { .. }
            | ToolCall::ComputerInputText { .. }
            | ToolCall::ComputerSnapshot { .. }
            | ToolCall::ComputerSaveSnapshot { .. }) => {
                self.dispatch_computer_tool(call, auth).await
            }

            call @ (ToolCall::ListProjects
            | ToolCall::RegisterProject { .. }
            | ToolCall::CreateProject { .. }) => self.dispatch_project_tool(call, auth).await,

            call @ (ToolCall::RunProcess { .. }
            | ToolCall::RunScript { .. }
            | ToolCall::RunShell { .. }) => {
                self.dispatch_shell_tool(call, execution_sandbox, ssh_resource, auth)
                    .await
            }

            call @ (ToolCall::OpenSessionShell { .. }
            | ToolCall::SessionShellExec { .. }
            | ToolCall::SessionShellStatus { .. }
            | ToolCall::CloseSessionShell { .. }) => {
                self.dispatch_session_shell_tool(call, ssh_resource).await
            }

            call @ (ToolCall::ApplyPatch { .. }
            | ToolCall::ApplyPatchChecked { .. }
            | ToolCall::ValidatePatch { .. }) => self.dispatch_patch_tool(call).await,

            call @ ToolCall::ImportConversationFilesToProject { .. } => {
                self.dispatch_conversation_import_tool(call, auth, transport)
                    .await
            }

            call @ (ToolCall::DeleteProjectFiles { .. }
            | ToolCall::ReadFile { .. }
            | ToolCall::ReadFiles { .. }
            | ToolCall::ListProjectFiles { .. }
            | ToolCall::ListProjectTrackedFiles { .. }
            | ToolCall::ProjectOverview { .. }
            | ToolCall::SearchProjectText { .. }
            | ToolCall::SearchProjectTexts { .. }
            | ToolCall::WriteProjectFile { .. }
            | ToolCall::SaveProjectArtifact { .. }
            | ToolCall::ExportProjectArtifact { .. }
            | ToolCall::ReadProjectArtifactMetadata { .. }
            | ToolCall::ReadProjectArtifact { .. }
            | ToolCall::ArtifactUploadBegin { .. }
            | ToolCall::ArtifactUploadChunk { .. }
            | ToolCall::ArtifactUploadFinish { .. }
            | ToolCall::ArtifactUploadAbort { .. }
            | ToolCall::ApplyTextEdits { .. }) => {
                self.dispatch_file_tool(call, transport, project_resolution)
                    .await
            }

            call @ (ToolCall::GitRestorePaths { .. }
            | ToolCall::DiscardUntracked { .. }
            | ToolCall::GitStatus { .. }
            | ToolCall::GitDiff { .. }
            | ToolCall::GitDiffHunks { .. }
            | ToolCall::GitLog { .. }
            | ToolCall::GitDiffSummary { .. }
            | ToolCall::ShowChanges { .. }) => self.dispatch_git_tool(call).await,

            call @ (ToolCall::CargoFmt { .. }
            | ToolCall::CargoCheck { .. }
            | ToolCall::CargoTest { .. }
            | ToolCall::GoTest { .. }) => {
                self.dispatch_cargo_tool(call, execution_sandbox, ssh_resource, auth)
                    .await
            }

            call @ (ToolCall::RunJob { .. }
            | ToolCall::StopJob { .. }
            | ToolCall::JobStatus { .. }
            | ToolCall::JobLog { .. }
            | ToolCall::ObserveJobs { .. }
            | ToolCall::ListJobs { .. }
            | ToolCall::JobTail { .. }) => {
                self.dispatch_job_tool(call, auth, execution_sandbox, ssh_resource)
                    .await
            }

            call @ ToolCall::WorkspaceHygieneCheck { .. } => self.dispatch_hygiene_tool(call).await,

            call @ (ToolCall::LspStatus { .. }
            | ToolCall::DocumentSymbols { .. }
            | ToolCall::DocumentDiagnostics { .. }
            | ToolCall::Hover { .. }
            | ToolCall::WorkspaceSymbols { .. }
            | ToolCall::GotoDefinition { .. }
            | ToolCall::FindReferences { .. }
            | ToolCall::CallHierarchy { .. }) => self.dispatch_lsp_tool(call).await,
        }
    }
}

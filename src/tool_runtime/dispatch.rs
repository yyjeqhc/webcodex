//! Runtime tool dispatch and session/permission guard flow.

use super::session_context::{
    add_session_project_mismatch_warning, add_session_telemetry_hint, current_session_key,
    current_session_unavailable_result, is_current_session_eligible, session_guard_denied_result,
    session_project_mismatch_requires_escape, session_project_mismatch_result,
    unknown_session_result, SessionProjectMismatch,
};
use super::{
    permissions, session_context, sessions, tool_disabled_result_from_definition, ToolCall,
    ToolResult, ToolRuntime,
};
use crate::auth::AuthContext;

impl ToolRuntime {
    /// Main dispatch — call from MCP handler or GPT Actions handler.
    ///
    /// This no-auth convenience defaults the caller context to `None`, which
    /// means agent-backed tools are rejected (no owner can be proven). HTTP
    /// wrappers should prefer `dispatch_with_auth` so the depot `AuthContext`
    /// is forwarded. `dispatch` is kept for internal/tests callers that only
    /// use local-executor projects.
    #[allow(dead_code)]
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
        mut call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        use_current_session: bool,
        allow_cross_project_session: bool,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
    ) -> ToolResult {
        let mut resolved_project = match call.project() {
            Some(project) => self
                .resolve_project_input_for_auth(project, auth)
                .await
                .ok(),
            None => None,
        };
        if use_current_session && call.session_id().is_none() && is_current_session_eligible(&call)
        {
            if let Some(resolved) = resolved_project.as_ref() {
                match current_session_key(auth, transport, &resolved.resolved_id) {
                    Ok(key) => {
                        if let Some(session_id) = self.sessions.current_session_id(&key) {
                            call = call.with_effective_session_id(session_id);
                        }
                    }
                    Err(message) => return current_session_unavailable_result(message),
                }
            }
        }
        let session_id = call.session_id().map(str::to_string);
        if let Some(session_id) = session_id.as_deref() {
            if !self.sessions.contains_session(session_id) {
                return unknown_session_result(session_id);
            }
        }
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
        if let Some(mut result) = tool_disabled_result_from_definition(call.tool_name()) {
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
            let resolved_project = resolved_project.take().map(|resolved| resolved.resolved_id);
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
        if let Err(err) = self.authorize_agent_tool(&call, auth).await {
            let mut err = err;
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
        let permission =
            permissions::permission_decision_for_tool(call.tool_name(), call.project());
        let mut result = self.dispatch_authorized_inner(call, auth, transport).await;
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
        result
    }

    async fn dispatch_authorized_inner(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        match call {
            call @ (ToolCall::ListTools { .. }
            | ToolCall::ListAgents
            | ToolCall::RuntimeStatus { .. }
            | ToolCall::ToolManifest { .. }) => self.dispatch_discovery_tool(call, auth).await,

            call @ (ToolCall::StartSession { .. }
            | ToolCall::SessionSummary { .. }
            | ToolCall::PostSessionMessage { .. }
            | ToolCall::ListSessionMessages { .. }
            | ToolCall::ResolveSessionMessage { .. }
            | ToolCall::SessionDiscussionSummary { .. }
            | ToolCall::BindCurrentSession { .. }
            | ToolCall::CurrentSession { .. }
            | ToolCall::UnbindCurrentSession { .. }) => {
                self.dispatch_session_tool(call, auth, transport).await
            }

            call @ (ToolCall::StartCodingTask { .. } | ToolCall::FinishCodingTask { .. }) => {
                self.dispatch_coding_task_tool(call, auth, transport).await
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

            call @ (ToolCall::ListProjects
            | ToolCall::RegisterProject { .. }
            | ToolCall::CreateProject { .. }) => self.dispatch_project_tool(call, auth).await,

            call @ ToolCall::RunShell { .. } => self.dispatch_shell_tool(call).await,

            call @ (ToolCall::ApplyPatch { .. }
            | ToolCall::ApplyPatchChecked { .. }
            | ToolCall::ValidatePatch { .. }) => self.dispatch_patch_tool(call).await,

            call @ (ToolCall::DeleteProjectFiles { .. }
            | ToolCall::ReadFile { .. }
            | ToolCall::ListProjectFiles { .. }
            | ToolCall::ProjectOverview { .. }
            | ToolCall::SearchProjectText { .. }
            | ToolCall::ReplaceInFile { .. }
            | ToolCall::ReplaceExactBlock { .. }
            | ToolCall::InsertBeforePattern { .. }
            | ToolCall::InsertAfterPattern { .. }
            | ToolCall::WriteProjectFile { .. }
            | ToolCall::SaveProjectArtifact { .. }
            | ToolCall::ReadProjectArtifactMetadata { .. }
            | ToolCall::ReadProjectArtifact { .. }
            | ToolCall::ArtifactUploadBegin { .. }
            | ToolCall::ArtifactUploadChunk { .. }
            | ToolCall::ArtifactUploadFinish { .. }
            | ToolCall::ArtifactUploadAbort { .. }
            | ToolCall::ReplaceLineRange { .. }
            | ToolCall::InsertAtLine { .. }
            | ToolCall::DeleteLineRange { .. }
            | ToolCall::ApplyTextEdits { .. }) => self.dispatch_file_tool(call).await,

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
            | ToolCall::CargoTest { .. }) => self.dispatch_cargo_tool(call).await,

            call @ (ToolCall::RunJob { .. }
            | ToolCall::StopJob { .. }
            | ToolCall::JobStatus { .. }
            | ToolCall::JobLog { .. }
            | ToolCall::ListJobs { .. }
            | ToolCall::JobTail { .. }) => self.dispatch_job_tool(call, auth).await,

            call @ ToolCall::WorkspaceHygieneCheck { .. } => self.dispatch_hygiene_tool(call).await,

            call @ (ToolCall::LspStatus { .. }
            | ToolCall::DocumentSymbols { .. }
            | ToolCall::DocumentDiagnostics { .. }
            | ToolCall::GotoDefinition { .. }
            | ToolCall::FindReferences { .. }) => self.dispatch_lsp_tool(call).await,
        }
    }
}

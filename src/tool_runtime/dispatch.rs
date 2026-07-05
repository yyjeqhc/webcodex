//! Runtime tool dispatch and session/permission guard flow.

use super::session_context::{
    add_session_project_mismatch_warning, add_session_telemetry_hint, current_session_key,
    current_session_unavailable_result, is_current_session_eligible, session_guard_denied_result,
    session_project_mismatch_requires_escape, session_project_mismatch_result,
    unknown_session_result, SessionProjectMismatch,
};
use super::tool_inputs::ListToolsOptions;
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
            ToolCall::ListTools {
                category,
                features,
                summary_only,
                limit,
            } => ToolResult::ok(self.list_tools_payload(ListToolsOptions {
                category,
                features,
                summary_only,
                limit,
            })),

            ToolCall::StartSession {
                project,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
            } => {
                self.start_session_tool(
                    project,
                    title,
                    mode,
                    deny_write_tools,
                    deny_shell_tools,
                    auth,
                )
                .await
            }

            ToolCall::StartCodingTask {
                project,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
                include_runtime_status,
                include_git,
                include_recent_commits,
                include_rules,
                include_tool_manifest,
                tool_manifest_categories,
                tool_manifest_limit,
                bind_current,
            } => {
                self.start_coding_task(
                    project,
                    title,
                    mode,
                    deny_write_tools,
                    deny_shell_tools,
                    include_runtime_status,
                    include_git,
                    include_recent_commits,
                    include_rules,
                    include_tool_manifest,
                    tool_manifest_categories,
                    tool_manifest_limit,
                    bind_current,
                    auth,
                    transport,
                )
                .await
            }

            ToolCall::FinishCodingTask {
                project,
                session_id,
                summary_only,
                include_diff,
                include_hygiene,
                include_handoff,
                include_validation_summary,
            } => {
                self.finish_coding_task(
                    project,
                    session_id,
                    summary_only,
                    include_diff,
                    include_hygiene,
                    include_handoff,
                    include_validation_summary,
                    auth,
                )
                .await
            }

            ToolCall::SessionSummary { session_id, limit } => {
                self.session_summary_tool(session_id, limit)
            }

            ToolCall::PostSessionMessage {
                session_id,
                kind,
                message,
                tags,
                reply_to,
                priority,
            } => {
                self.post_session_message_tool(session_id, kind, message, tags, reply_to, priority)
            }

            ToolCall::ListSessionMessages {
                session_id,
                kind,
                status,
                limit,
            } => self.list_session_messages_tool(session_id, kind, status, limit),

            ToolCall::ResolveSessionMessage {
                session_id,
                message_id,
                resolution,
            } => self.resolve_session_message_tool(session_id, message_id, resolution),

            ToolCall::SessionDiscussionSummary { session_id, limit } => {
                self.session_discussion_summary_tool(session_id, limit)
            }

            ToolCall::SessionHandoffSummary {
                session_id,
                project,
                include_workspace,
                include_checkpoints,
                include_validation,
                summary_only,
                limit,
            } => {
                self.session_handoff_summary(
                    session_id,
                    project,
                    include_workspace,
                    include_checkpoints,
                    include_validation,
                    summary_only,
                    limit,
                    auth,
                )
                .await
            }

            ToolCall::BindCurrentSession {
                project,
                session_id,
            } => {
                self.bind_current_session_tool(project, session_id, auth, transport)
                    .await
            }

            ToolCall::CurrentSession { project } => {
                self.current_session_tool(project, auth, transport).await
            }

            ToolCall::UnbindCurrentSession { project } => {
                self.unbind_current_session_tool(project, auth, transport)
                    .await
            }

            call @ (ToolCall::WorkspaceCheckpointCreate { .. }
            | ToolCall::WorkspaceCheckpointList { .. }
            | ToolCall::WorkspaceCheckpointShow { .. }
            | ToolCall::WorkspaceCheckpointRestore { .. }
            | ToolCall::WorkspaceCheckpointDelete { .. }) => {
                self.dispatch_workspace_checkpoint_tool(call).await
            }

            ToolCall::ListProjects => self.list_projects(auth).await,

            ToolCall::RegisterProject {
                client_id,
                id,
                name,
                path,
                description,
                allow_patch,
                overwrite,
            } => {
                self.register_project(
                    client_id,
                    id,
                    name,
                    path,
                    description,
                    allow_patch,
                    overwrite,
                    auth,
                )
                .await
            }

            ToolCall::CreateProject {
                client_id,
                id,
                name,
                path,
                description,
                allow_patch,
                template,
                git_init,
                allow_existing_empty,
                overwrite,
            } => {
                self.create_project(
                    client_id,
                    id,
                    name,
                    path,
                    description,
                    allow_patch,
                    template,
                    git_init,
                    allow_existing_empty,
                    overwrite,
                    auth,
                )
                .await
            }

            ToolCall::ListAgents => self.list_agents(auth).await,

            ToolCall::RuntimeStatus => self.runtime_status(auth).await,

            ToolCall::ToolManifest {
                category,
                include_recommended_flows,
                include_risk_summary,
            } => {
                self.tool_manifest(category, include_recommended_flows, include_risk_summary)
                    .await
            }

            ToolCall::RunShell {
                project,
                command,
                session_id: _,
                timeout_secs,
                cwd,
            } => self.run_shell(project, command, timeout_secs, cwd).await,

            ToolCall::ApplyPatch {
                project,
                patch,
                session_id: _,
            } => self.apply_patch(project, patch).await,

            ToolCall::ApplyPatchChecked {
                project,
                patch,
                session_id: _,
                deny_sensitive_paths,
            } => {
                self.apply_patch_checked(project, patch, deny_sensitive_paths)
                    .await
            }

            call @ (ToolCall::DeleteProjectFiles { .. }
            | ToolCall::ReadFile { .. }
            | ToolCall::ListProjectFiles { .. }
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

            ToolCall::ValidatePatch {
                project,
                patch,
                session_id: _,
                deny_sensitive_paths,
            } => {
                self.validate_patch(project, patch, deny_sensitive_paths)
                    .await
            }

            ToolCall::CargoFmt {
                project,
                session_id: _,
                cwd,
                check,
                timeout_secs,
            } => self.cargo_fmt(project, cwd, check, timeout_secs).await,

            ToolCall::CargoCheck {
                project,
                session_id: _,
                cwd,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                timeout_secs,
            } => {
                self.cargo_check(
                    project,
                    cwd,
                    all_targets,
                    all_features,
                    no_default_features,
                    features,
                    package,
                    timeout_secs,
                )
                .await
            }

            ToolCall::CargoTest {
                project,
                session_id: _,
                cwd,
                filter,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                no_run,
                timeout_secs,
            } => {
                self.cargo_test(
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
                )
                .await
            }

            call @ (ToolCall::RunJob { .. }
            | ToolCall::StopJob { .. }
            | ToolCall::JobStatus { .. }
            | ToolCall::JobLog { .. }
            | ToolCall::ListJobs { .. }
            | ToolCall::JobTail { .. }) => self.dispatch_job_tool(call, auth).await,

            ToolCall::RunCodex {
                project: _,
                prompt: _,
                session_id: _,
                approval_mode: _,
                timeout_secs: _,
                cwd: _,
                extra_args: _,
            } => tool_disabled_result_from_definition("run_codex")
                .expect("run_codex must be disabled by ToolDefinition policy"),

            ToolCall::WorkspaceHygieneCheck {
                project,
                max_findings,
                include_tracked,
                session_id,
            } => {
                self.workspace_hygiene_check(project, max_findings, include_tracked, session_id)
                    .await
            }
        }
    }
}

//! Tool Runtime — unified execution layer for MCP and GPT Actions.
//!
//! Both protocol adapters call `ToolRuntime::dispatch()`.
//! No HTTP framework types here — pure Rust input/output.

mod agent_authorization;
mod cargo;
mod checkpoint;
mod codex;
pub(crate) mod files;
mod git;
mod handoff;
mod helpers;
mod hygiene;
mod jobs;
pub(crate) mod kernel;
pub(crate) mod metadata;
mod patch;
pub(crate) mod project_instructions;
mod project_resolution;
mod projects;
mod registry;
mod runtime;
mod session_context;
pub(crate) mod sessions;
mod shell;
mod types;

// Re-export the public API so `crate::tool_runtime::ToolCall` etc. still work.
#[allow(unused_imports)]
pub use runtime::ToolRuntime;
#[allow(unused_imports)]
pub use types::{
    default_true, is_known_tool_name, ApplyTextEditInput, ApplyTextEditKind, RuntimeInfo,
    SessionMode, ToolCall, ToolResult, ToolSpec, KNOWN_TOOL_NAMES,
};

use crate::auth::AuthContext;
use serde_json::{json, Value};
use std::path::PathBuf;

#[allow(unused_imports)]
pub(crate) use crate::config::CodexConfig;
use helpers::normalize_local_status;
#[allow(unused_imports)]
pub(crate) use project_resolution::{ProjectResolverError, ProjectResolverErrorKind};
#[allow(unused_imports)]
pub(crate) use session_context::{
    add_session_telemetry_hint, current_session_principal, session_guard_denied_result,
    unknown_session_result,
};
use session_context::{
    current_session_key, current_session_unavailable_result, is_current_session_eligible,
    session_message_error_result,
};
#[allow(unused_imports)]
pub(crate) use types::AgentCapability;
use types::ACTIVE_JOB_STATUSES;

pub(crate) const RUN_CODEX_DISABLED_MESSAGE: &str =
    "run_codex is currently disabled on model-facing surfaces; use run_job or external local Codex manually.";

pub(crate) fn run_codex_disabled_result() -> ToolResult {
    ToolResult::err_with_output(
        RUN_CODEX_DISABLED_MESSAGE,
        json!({
            "code": "run_codex_disabled",
            "tool": "run_codex",
            "message": RUN_CODEX_DISABLED_MESSAGE,
        }),
    )
}

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
        self.dispatch_with_auth_transport_options(call, auth, transport, true)
            .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options(
        &self,
        mut call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        use_current_session: bool,
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
        if matches!(&call, ToolCall::RunCodex { .. }) {
            let mut result = run_codex_disabled_result();
            if let Some(session_id) = session_id.as_deref() {
                let session_start = self.sessions.record_tool_call_started(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
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
                let session_start = self.sessions.record_tool_call_started(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
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
        let session_start = if session_id.is_some() {
            let resolved_project = resolved_project.take().map(|resolved| resolved.resolved_id);
            Some(self.sessions.record_tool_call_started_with_options(
                session_id.as_deref(),
                transport,
                call.tool_name(),
                &call.session_log_arguments(),
                resolved_project,
            ))
        } else {
            None
        };
        if let Err(err) = self.authorize_agent_tool(&call, auth).await {
            let mut err = err;
            if let Some(session_id) = session_id.as_deref() {
                let event_id = self.sessions.record_tool_call_finished(
                    session_start.flatten(),
                    false,
                    &err.output,
                    err.error.as_deref(),
                    None,
                );
                add_session_telemetry_hint(&mut err, &self.sessions, session_id, event_id);
            }
            return err;
        }
        let mut result = self.dispatch_authorized_inner(call, auth, transport).await;
        if let Some(session_id) = session_id.as_deref() {
            let event_id = self.sessions.record_tool_call_finished(
                session_start.flatten(),
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
            ToolCall::ListTools => ToolResult::ok(json!({ "tools": self.tool_specs() })),

            ToolCall::StartSession {
                project,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
            } => {
                let resolved = match project {
                    Some(project_input) => match self
                        .resolve_project_input_for_auth(&project_input, auth)
                        .await
                    {
                        Ok(resolved) => Some(resolved),
                        Err(err) => return err.into_tool_result(),
                    },
                    None => None,
                };
                // Best-effort load of project-local instruction files
                // (AGENTS.md, CLAUDE.md, ...). Any read failure is swallowed
                // and never fails start_session. `null` when no project was
                // provided; `loaded=false` when a project had no candidate.
                let project_instructions = match &resolved {
                    Some(resolved) => Some(self.load_project_instructions(&resolved.config).await),
                    None => None,
                };
                let summary =
                    self.sessions
                        .start_session_with_options(sessions::SessionCreateOptions {
                            project: resolved
                                .as_ref()
                                .map(|resolved| resolved.resolved_id.clone()),
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
                ToolResult::ok(json!({
                    "success": true,
                    "session_id": summary.session_id,
                    "project": summary.project,
                    "project_input": resolved.as_ref().map(|resolved| resolved.input.clone()),
                    "resolved_project": resolved.as_ref().map(|resolved| resolved.resolved_id.clone()),
                    "title": summary.title,
                    "mode": summary.mode,
                    "guards": summary.guards,
                    "created_at": summary.created_at,
                    "project_instructions": project_instructions,
                }))
            }

            ToolCall::SessionSummary { session_id, limit } => {
                match self.sessions.summary(&session_id, limit) {
                    Some(summary) => ToolResult::ok(
                        serde_json::to_value(summary)
                            .unwrap_or_else(|_| json!({"session_id": session_id, "events": []})),
                    ),
                    None => unknown_session_result(&session_id),
                }
            }

            ToolCall::PostSessionMessage {
                session_id,
                kind,
                message,
                tags,
                reply_to,
                priority,
            } => match self
                .sessions
                .post_message(sessions::PostSessionMessageInput {
                    session_id: session_id.clone(),
                    kind,
                    message,
                    tags,
                    reply_to,
                    priority,
                }) {
                Ok(message) => ToolResult::ok(json!({
                    "success": true,
                    "session_id": session_id,
                    "message_id": message.message_id,
                    "message": message,
                })),
                Err(err) => session_message_error_result(&session_id, None, err),
            },

            ToolCall::ListSessionMessages {
                session_id,
                kind,
                status,
                limit,
            } => match self.sessions.list_messages(
                &session_id,
                sessions::ListSessionMessagesFilter {
                    kind,
                    status,
                    limit,
                },
            ) {
                Ok(messages) => ToolResult::ok(json!({
                    "success": true,
                    "session_id": session_id,
                    "messages": messages,
                })),
                Err(err) => session_message_error_result(&session_id, None, err),
            },

            ToolCall::ResolveSessionMessage {
                session_id,
                message_id,
                resolution,
            } => match self
                .sessions
                .resolve_message(&session_id, &message_id, resolution)
            {
                Ok(message) => ToolResult::ok(json!({
                    "success": true,
                    "session_id": session_id,
                    "message_id": message.message_id,
                    "message": message,
                })),
                Err(err) => session_message_error_result(&session_id, Some(&message_id), err),
            },

            ToolCall::SessionDiscussionSummary { session_id, limit } => {
                match self.sessions.discussion_summary(&session_id, limit) {
                    Ok(summary) => ToolResult::ok(json!({
                        "success": true,
                        "session_id": session_id,
                        "counts": summary.counts,
                        "open_guidance": summary.open_guidance,
                        "open_questions": summary.open_questions,
                        "open_risks": summary.open_risks,
                        "open_todos": summary.open_todos,
                        "recent_progress": summary.recent_progress,
                        "recent_decisions": summary.recent_decisions,
                    })),
                    Err(err) => session_message_error_result(&session_id, None, err),
                }
            }

            ToolCall::SessionHandoffSummary {
                session_id,
                project,
                include_workspace,
                include_checkpoints,
                limit,
            } => {
                self.session_handoff_summary(
                    session_id,
                    project,
                    include_workspace,
                    include_checkpoints,
                    limit,
                )
                .await
            }

            ToolCall::BindCurrentSession {
                project,
                session_id,
            } => {
                let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
                    Ok(resolved) => resolved,
                    Err(err) => return err.into_tool_result(),
                };
                let Some(summary) = self.sessions.summary(&session_id, None) else {
                    return unknown_session_result(&session_id);
                };
                if summary.project.as_deref() != Some(resolved.resolved_id.as_str()) {
                    return ToolResult::err_with_output(
                        "session_project_mismatch",
                        json!({
                            "error_kind": "session_project_mismatch",
                            "session_id": session_id,
                            "session_project": summary.project,
                            "project": project,
                            "resolved_project": resolved.resolved_id,
                        }),
                    );
                }
                let key = match current_session_key(auth, transport, &resolved.resolved_id) {
                    Ok(key) => key,
                    Err(message) => return current_session_unavailable_result(message),
                };
                let Some(bound) = self.sessions.bind_current_session(key, &session_id) else {
                    return unknown_session_result(&session_id);
                };
                ToolResult::ok(json!({
                    "bound": true,
                    "session_id": bound.session_id,
                    "project": project,
                    "resolved_project": resolved.resolved_id,
                    "mode": bound.mode,
                    "guards": bound.guards,
                }))
            }

            ToolCall::CurrentSession { project } => {
                let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
                    Ok(resolved) => resolved,
                    Err(err) => return err.into_tool_result(),
                };
                let key = match current_session_key(auth, transport, &resolved.resolved_id) {
                    Ok(key) => key,
                    Err(message) => return current_session_unavailable_result(message),
                };
                match self.sessions.current_session(&key) {
                    Some(summary) => ToolResult::ok(json!({
                        "found": true,
                        "session_id": summary.session_id,
                        "project": project,
                        "resolved_project": resolved.resolved_id,
                        "mode": summary.mode,
                        "guards": summary.guards,
                    })),
                    None => ToolResult::ok(json!({
                        "found": false,
                        "project": project,
                        "resolved_project": resolved.resolved_id,
                    })),
                }
            }

            ToolCall::UnbindCurrentSession { project } => {
                let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
                    Ok(resolved) => resolved,
                    Err(err) => return err.into_tool_result(),
                };
                let key = match current_session_key(auth, transport, &resolved.resolved_id) {
                    Ok(key) => key,
                    Err(message) => return current_session_unavailable_result(message),
                };
                let had_binding = self.sessions.unbind_current_session(&key);
                ToolResult::ok(json!({
                    "unbound": true,
                    "had_binding": had_binding,
                    "project": project,
                    "resolved_project": resolved.resolved_id,
                }))
            }

            ToolCall::WorkspaceCheckpointCreate {
                project,
                title,
                note,
                include_untracked,
                kind,
                labels,
                validation,
                session_id: _,
            } => {
                self.workspace_checkpoint_create(
                    project,
                    title,
                    note,
                    include_untracked,
                    kind,
                    labels,
                    validation,
                )
                .await
            }

            ToolCall::WorkspaceCheckpointList {
                project,
                limit,
                session_id: _,
            } => self.workspace_checkpoint_list(project, limit).await,

            ToolCall::WorkspaceCheckpointShow {
                project,
                checkpoint_id,
                include_diff_stat,
                session_id: _,
            } => {
                self.workspace_checkpoint_show(project, checkpoint_id, include_diff_stat)
                    .await
            }

            ToolCall::WorkspaceCheckpointRestore {
                project,
                checkpoint_id,
                confirm,
                session_id: _,
            } => {
                self.workspace_checkpoint_restore(project, checkpoint_id, confirm)
                    .await
            }

            ToolCall::WorkspaceCheckpointDelete {
                project,
                checkpoint_id,
                confirm,
                session_id: _,
            } => {
                self.workspace_checkpoint_delete(project, checkpoint_id, confirm)
                    .await
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

            ToolCall::DeleteProjectFiles {
                project,
                paths,
                session_id: _,
            } => self.delete_project_files(project, paths).await,

            ToolCall::GitRestorePaths {
                project,
                paths,
                session_id: _,
            } => self.git_restore_paths(project, paths).await,

            ToolCall::DiscardUntracked {
                project,
                paths,
                session_id: _,
            } => self.discard_untracked(project, paths).await,

            ToolCall::ValidatePatch {
                project,
                patch,
                session_id: _,
                deny_sensitive_paths,
            } => {
                self.validate_patch(project, patch, deny_sensitive_paths)
                    .await
            }

            ToolCall::GitStatus {
                project,
                session_id: _,
            } => self.git_status(project).await,

            ToolCall::GitDiff {
                project,
                session_id: _,
                args,
            } => self.git_diff(project, args).await,

            ToolCall::GitDiffHunks {
                project,
                session_id: _,
                paths,
                max_hunks,
                max_hunk_lines,
                cached,
            } => {
                self.git_diff_hunks(project, paths, max_hunks, max_hunk_lines, cached)
                    .await
            }

            ToolCall::GitLog {
                project,
                limit,
                skip,
                session_id: _,
            } => self.git_log(project, limit, skip).await,

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

            ToolCall::ReadFile {
                project,
                path,
                session_id: _,
                start_line,
                limit,
                with_line_numbers,
            } => {
                self.read_file(project, path, start_line, limit, with_line_numbers)
                    .await
            }

            ToolCall::RunJob {
                project,
                command,
                session_id: _,
                timeout_secs,
                cwd,
            } => self.run_job(project, command, timeout_secs, cwd).await,

            ToolCall::RunCodex {
                project: _,
                prompt: _,
                session_id: _,
                approval_mode: _,
                timeout_secs: _,
                cwd: _,
                extra_args: _,
            } => run_codex_disabled_result(),

            ToolCall::JobStatus { job_id } => self.job_status_for_auth(job_id, auth).await,

            ToolCall::JobLog {
                job_id,
                offset,
                tail_lines,
            } => {
                self.job_log_for_auth(job_id, offset, tail_lines, auth)
                    .await
            }

            ToolCall::ListProjectFiles {
                project,
                session_id: _,
                path,
                limit,
            } => self.list_project_files(project, path, limit).await,

            ToolCall::SearchProjectText {
                project,
                pattern,
                session_id: _,
                path,
                limit,
                context_before,
                context_after,
            } => {
                self.search_project_text(
                    project,
                    pattern,
                    path,
                    limit,
                    context_before,
                    context_after,
                )
                .await
            }

            ToolCall::GitDiffSummary {
                project,
                session_id: _,
            } => self.git_diff_summary(project).await,

            ToolCall::ShowChanges {
                project,
                session_id,
                include_diff,
                max_hunks,
                max_hunk_lines,
                session_event_limit,
            } => {
                self.show_changes(
                    project,
                    session_id,
                    include_diff,
                    max_hunks,
                    max_hunk_lines,
                    session_event_limit,
                )
                .await
            }

            ToolCall::WorkspaceHygieneCheck {
                project,
                max_findings,
                include_tracked,
                session_id,
            } => {
                self.workspace_hygiene_check(project, max_findings, include_tracked, session_id)
                    .await
            }

            ToolCall::ListJobs { limit, status } => {
                self.list_jobs_for_auth(limit, status, auth).await
            }

            ToolCall::JobTail { job_id, tail_lines } => {
                self.job_tail_for_auth(job_id, tail_lines, auth).await
            }

            ToolCall::ReplaceInFile {
                project,
                path,
                old,
                new,
                session_id: _,
                expected_replacements,
                allow_multiple,
            } => {
                self.replace_in_file(
                    project,
                    path,
                    old,
                    new,
                    expected_replacements,
                    allow_multiple,
                )
                .await
            }

            ToolCall::ReplaceExactBlock {
                project,
                path,
                old_text,
                new_text,
                session_id: _,
                expected_old_sha256,
            } => {
                self.replace_exact_block(project, path, old_text, new_text, expected_old_sha256)
                    .await
            }

            ToolCall::InsertBeforePattern {
                project,
                path,
                pattern,
                text,
                session_id: _,
            } => {
                self.insert_around_pattern(project, path, pattern, text, "insert_before_pattern")
                    .await
            }

            ToolCall::InsertAfterPattern {
                project,
                path,
                pattern,
                text,
                session_id: _,
            } => {
                self.insert_around_pattern(project, path, pattern, text, "insert_after_pattern")
                    .await
            }

            ToolCall::WriteProjectFile {
                project,
                path,
                content,
                session_id: _,
                overwrite,
                expected_sha256,
                expected_content_prefix,
            } => {
                self.write_project_file(
                    project,
                    path,
                    content,
                    overwrite,
                    expected_sha256,
                    expected_content_prefix,
                )
                .await
            }

            ToolCall::SaveProjectArtifact {
                project,
                path,
                content_base64,
                session_id: _,
                mime_type,
                overwrite,
            } => {
                self.save_project_artifact(project, path, content_base64, mime_type, overwrite)
                    .await
            }

            ToolCall::ReadProjectArtifactMetadata {
                project,
                path,
                session_id: _,
            } => self.read_project_artifact_metadata(project, path).await,

            ToolCall::ReadProjectArtifact {
                project,
                path,
                session_id: _,
                encoding,
                offset,
                length,
                max_bytes,
            } => {
                self.read_project_artifact(project, path, encoding, offset, length, max_bytes)
                    .await
            }

            ToolCall::ReplaceLineRange {
                project,
                path,
                start_line,
                end_line,
                new_text,
                session_id: _,
                expected_old_sha256,
                expected_old_prefix,
            } => {
                self.replace_line_range(
                    project,
                    path,
                    start_line,
                    end_line,
                    new_text,
                    expected_old_sha256,
                    expected_old_prefix,
                )
                .await
            }

            ToolCall::InsertAtLine {
                project,
                path,
                line,
                text,
                session_id: _,
                expected_anchor_sha256,
                expected_anchor_prefix,
            } => {
                self.insert_at_line(
                    project,
                    path,
                    line,
                    text,
                    expected_anchor_sha256,
                    expected_anchor_prefix,
                )
                .await
            }

            ToolCall::DeleteLineRange {
                project,
                path,
                start_line,
                end_line,
                session_id: _,
                expected_old_sha256,
                expected_old_prefix,
            } => {
                self.delete_line_range(
                    project,
                    path,
                    start_line,
                    end_line,
                    expected_old_sha256,
                    expected_old_prefix,
                )
                .await
            }

            ToolCall::ApplyTextEdits {
                project,
                path,
                edits,
                dry_run,
                expected_file_sha256,
                session_id: _,
            } => {
                self.apply_text_edits(project, path, edits, dry_run, expected_file_sha256)
                    .await
            }
        }
    }

    async fn list_projects(&self, auth: Option<&AuthContext>) -> ToolResult {
        let mut list: Vec<Value> = Vec::new();
        for client in self.shell_clients.list_clients_for_auth(auth).await {
            // Sanitized shell-profiles summary for this agent (carried inside
            // the registration policy). Used to resolve which profile a project
            // actually uses and whether that profile is configured. `None` for
            // older agents that did not report one.
            let shell_profiles = client
                .policy
                .as_ref()
                .and_then(|p| p.shell_profiles.as_ref());
            for project in client.projects.iter().filter(|p| !p.disabled) {
                let (resolved_shell_profile, shell_profile_status) =
                    resolve_project_shell_profile(project.shell_profile.as_deref(), shell_profiles);
                list.push(json!({
                    "id": Self::agent_project_runtime_id(&client.client_id, &project.id),
                    "agent_project_id": project.id,
                    "name": project.name,
                    "path": project.path,
                    "executor": "agent",
                    "client_id": client.client_id,
                    "allow_patch": project.allow_patch,
                    "source": "agent_registered",
                    "agent_status": client.status,
                    "connected": client.connected,
                    "last_seen": client.last_seen,
                    "shell_profile": project.shell_profile,
                    "resolved_shell_profile": resolved_shell_profile,
                    "shell_profile_status": shell_profile_status,
                }));
            }
        }
        list.sort_by(|a, b| {
            a["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["id"].as_str().unwrap_or_default())
        });
        ToolResult::ok(Value::Array(list))
    }

    async fn list_agents(&self, auth: Option<&AuthContext>) -> ToolResult {
        let clients = self.shell_clients.list_clients_for_auth(auth).await;
        let agents: Vec<Value> = clients
            .iter()
            .map(|c| {
                json!({
                    "client_id": c.client_id,
                    "agent_instance_id": c.agent_instance_id,
                    "display_name": c.display_name,
                    "owner": c.owner,
                    "hostname": c.hostname,
                    "status": c.status,
                    "connected": c.connected,
                    "agent_protocol_version": c.agent_protocol_version,
                    "transport": c.transport,
                    "last_seen": c.last_seen,
                    "pending_requests": c.pending_requests,
                    "capabilities": c.capabilities,
                    "projects": c.projects,
                    "policy": sanitized_policy_summary(c.policy.as_ref()),
                    "shell_profiles": sanitized_shell_profiles_summary(
                        c.policy.as_ref().and_then(|p| p.shell_profiles.as_ref())
                    ),
                })
            })
            .collect();
        ToolResult::ok(json!({ "agents": agents }))
    }

    /// Build the runtime observability summary. Read-only; never exposes
    /// tokens, api keys, full env, complete project path lists, or
    /// stdout/stderr. Returns a structured JSON object with service metadata,
    /// project config status, agent client summaries, and job counts.
    async fn runtime_status(&self, auth: Option<&AuthContext>) -> ToolResult {
        // -- projects summary -------------------------------------------------
        let (projects_configured, projects_count, projects_load_error) =
            match self.projects.config.as_ref() {
                Some(cfg) => (true, cfg.projects.len(), None),
                None => (
                    false,
                    0,
                    self.projects
                        .load_error
                        .clone()
                        .or_else(|| Some("Projects not configured".to_string())),
                ),
            };
        let projects = json!({
            "configured": projects_configured,
            "count": projects_count,
            "config_path": self.projects.config_path,
            "load_error": projects_load_error,
        });

        // -- agents summary ---------------------------------------------------
        // Build a trimmed client list so the summary never leaks per-request
        // state. Only carry fields useful for observability. `last_seen` is a
        // unix timestamp (seconds) of the most recent heartbeat/result; the
        // console uses it to render how stale an agent is and to make a
        // websocket agent flipping `online` -> `stale` visually obvious.
        let clients = self.shell_clients.list_clients_for_auth(auth).await;
        let agent_count = clients.len();
        let online_count = clients.iter().filter(|c| c.connected).count();
        // `stale_count` = registered agents whose `last_seen` is older than the
        // online window (status == "stale"). Truly offline agents are removed
        // from the registry on disconnect, so they never appear here; the
        // legacy `offline_count` field is retained (it mirrors `stale_count`
        // for the registered set) for backward compatibility with existing
        // callers/tests.
        let stale_count = agent_count.saturating_sub(online_count);
        let offline_count = stale_count;
        let clients_summary: Vec<Value> = clients
            .iter()
            .map(|c| {
                json!({
                    "client_id": c.client_id,
                    "agent_instance_id": c.agent_instance_id,
                    "display_name": c.display_name,
                    "owner": c.owner,
                    "status": c.status,
                    "connected": c.connected,
                    "agent_protocol_version": c.agent_protocol_version,
                    "transport": c.transport,
                    "last_seen": c.last_seen,
                    "pending_requests": c.pending_requests,
                    "capabilities": c.capabilities,
                    "projects_count": c.projects.len(),
                    "policy": sanitized_policy_summary(c.policy.as_ref()),
                    "shell_profiles": sanitized_shell_profiles_summary(
                        c.policy.as_ref().and_then(|p| p.shell_profiles.as_ref())
                    ),
                })
            })
            .collect();
        let agents = json!({
            "count": agent_count,
            "online_count": online_count,
            "stale_count": stale_count,
            "offline_count": offline_count,
            "clients": clients_summary,
        });

        // -- jobs summary -----------------------------------------------------
        // Agent-known jobs come from the registry; local jobs come from the
        // in-memory map. Active = running/queued/agent_queued/stop_requested.
        let agent_jobs = self.shell_clients.list_jobs_for_auth(auth, None).await;
        let agent_known_count = agent_jobs.len();
        let local_job_dirs: Vec<PathBuf> = if Self::local_jobs_visible_to_auth(auth) {
            let local_jobs_map = self.local_jobs.lock().await;
            local_jobs_map
                .values()
                .map(|record| record.dir.clone())
                .collect()
        } else {
            Vec::new()
        };
        let local_known_count = local_job_dirs.len();
        // Avoid double-counting: agent jobs are tracked separately from local
        // jobs (local jobs are only in the in-memory map; agent jobs are only
        // in the registry). Count active across both.
        let agent_active = agent_jobs
            .iter()
            .filter(|j| ACTIVE_JOB_STATUSES.contains(&j.status.as_str()))
            .count();
        let mut local_active = 0usize;
        for dir in local_job_dirs {
            if let Some(status) = std::fs::read_to_string(dir.join("status"))
                .ok()
                .map(|s| s.trim().to_string())
            {
                let normalized = normalize_local_status(&status);
                if ACTIVE_JOB_STATUSES.contains(&normalized.as_str()) {
                    local_active += 1;
                }
            }
        }
        let active_count = agent_active + local_active;
        let jobs = json!({
            "agent_known_count": agent_known_count,
            "local_known_count": local_known_count,
            "active_count": active_count,
        });

        // -- tools summary ----------------------------------------------------
        let specs = self.tool_specs();
        let tools_count = specs.len();
        let tools_names: Vec<String> = specs.iter().map(|s| s.name.clone()).collect();
        let tools = json!({
            "count": tools_count,
            "names": tools_names,
        });

        let quic = self.runtime_info.quic.as_ref().map(|status| {
            let status = status.lock().expect("quic runtime status mutex poisoned");
            json!({
                "enabled": status.enabled,
                "listen": status.listen,
                "alpn": status.alpn,
                "listener_started": status.listener_started,
                "last_error": status.last_error,
            })
        });

        let mut output = json!({
            "service": "webcodex",
            "version": env!("CARGO_PKG_VERSION"),
            "build": crate::build_info::runtime_build_info(),
            "server_time": chrono::Utc::now().timestamp(),
            "pid": std::process::id(),
            "auth_enabled": self.runtime_info.auth_enabled,
            "configured_public_url": self.runtime_info.configured_public_url,
            "projects": projects,
            "agents": agents,
            "jobs": jobs,
            "tools": tools,
            "session_store": self.sessions.status(),
        });
        if let Some(quic) = quic {
            output["quic"] = quic;
        }
        ToolResult::ok(output)
    }

    /// Return a compact, bounded tool manifest with categories, risk summary,
    /// and recommended flows. Read-only runtime introspection; never exposes
    /// full input/output schemas, tokens, secrets, or internal paths.
    /// Intended as a lightweight alternative to `list_tools` for long-running
    /// tasks where the full schemas cause ResponseTooLargeError.
    async fn tool_manifest(
        &self,
        category: Option<String>,
        include_recommended_flows: bool,
        include_risk_summary: bool,
    ) -> ToolResult {
        let specs = self.tool_specs();
        let tool_count = specs.len();

        // Build compact tool entries from metadata — no input/output schemas,
        // no long descriptions.
        let all_tools: Vec<Value> = specs
            .iter()
            .map(|spec| {
                let name = spec.name.as_str();
                let m = metadata::tool_metadata(name);
                json!({
                    "name": name,
                    "category": tool_manifest_category(name),
                    "provider": m.provider_id,
                    "risk": m.risk.session_risk_class(),
                    "read_only": m.read_only,
                    "requires_project": m.requires_project,
                    "path_hint": path_hint_str(m.path_hint),
                    "destructive": m.destructive,
                    "shell_like": m.shell_like,
                    "oauth_scope": m.oauth_scope,
                })
            })
            .collect();

        // Build the categories map from the full tool set so the caller can
        // always see valid categories even when filtering.
        let categories = build_manifest_categories(&all_tools);

        // Apply the optional category filter.
        let filtered_tools: Vec<Value> = match &category {
            Some(cat) => all_tools
                .iter()
                .filter(|t| t["category"].as_str() == Some(cat.as_str()))
                .cloned()
                .collect(),
            None => all_tools,
        };
        let filtered_count = filtered_tools.len();

        let mut output = json!({
            "schema_version": 1,
            "tool_count": tool_count,
            "filtered_count": filtered_count,
            "category": category,
            "categories": categories,
            "tools": filtered_tools,
        });

        if include_risk_summary {
            output["risk_summary"] =
                build_risk_summary(output["tools"].as_array().unwrap_or(&Vec::new()));
        }

        if include_recommended_flows {
            output["recommended_flows"] = Value::Array(tool_manifest_recommended_flows());
        }

        ToolResult::ok(output)
    }
}

/// Map a tool name to its primary manifest category. This is the single
/// centralized classification function for `tool_manifest`; it must cover
/// every name in `KNOWN_TOOL_NAMES`.
fn tool_manifest_category(name: &str) -> &'static str {
    match name {
        // Runtime introspection / discovery
        "list_tools" | "tool_manifest" | "runtime_status" | "list_agents" => "runtime",
        // Session lifecycle and messaging
        "start_session"
        | "session_summary"
        | "post_session_message"
        | "list_session_messages"
        | "resolve_session_message"
        | "session_discussion_summary"
        | "session_handoff_summary"
        | "bind_current_session"
        | "current_session"
        | "unbind_current_session" => "session",
        // Workspace checkpoints
        "workspace_checkpoint_create"
        | "workspace_checkpoint_list"
        | "workspace_checkpoint_show"
        | "workspace_checkpoint_restore"
        | "workspace_checkpoint_delete" => "checkpoint",
        // Git read / review
        "git_status" | "git_diff" | "git_diff_hunks" | "git_log" | "git_diff_summary"
        | "show_changes" => "git",
        // Structured file edits
        "replace_in_file"
        | "replace_exact_block"
        | "insert_before_pattern"
        | "insert_after_pattern"
        | "write_project_file"
        | "replace_line_range"
        | "insert_at_line"
        | "delete_line_range"
        | "apply_text_edits" => "edit",
        // File read / list / search
        "read_file" | "list_project_files" | "search_project_text" => "file",
        // Patch apply / validate
        "apply_patch" | "apply_patch_checked" | "validate_patch" => "patch",
        // Validation
        "cargo_fmt" | "cargo_check" | "cargo_test" => "validation",
        // Shell / job execution
        "run_shell" | "run_job" | "job_status" | "job_log" | "list_jobs" | "job_tail" => "job",
        // Project management
        "list_projects" | "register_project" | "create_project" => "project",
        // Artifacts
        "save_project_artifact" | "read_project_artifact_metadata" | "read_project_artifact" => {
            "artifact"
        }
        // Cleanup / destructive
        "delete_project_files"
        | "git_restore_paths"
        | "discard_untracked"
        | "workspace_hygiene_check" => "cleanup",
        // Codex delegation
        "run_codex" => "codex",
        _ => "other",
    }
}

/// String representation of a `ToolPathHint` for the compact manifest.
fn path_hint_str(hint: metadata::ToolPathHint) -> &'static str {
    match hint {
        metadata::ToolPathHint::None => "none",
        metadata::ToolPathHint::SinglePath => "single_path",
        metadata::ToolPathHint::PathList => "path_list",
        metadata::ToolPathHint::Patch => "patch",
        metadata::ToolPathHint::Artifact => "artifact",
    }
}

/// Build the categories map from the compact tool entries. Each category
/// maps to a sorted list of tool names.
fn build_manifest_categories(tools: &[Value]) -> Value {
    let mut map: std::collections::BTreeMap<&str, Vec<String>> = std::collections::BTreeMap::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or("");
        let category = tool["category"].as_str().unwrap_or("other");
        map.entry(category).or_default().push(name.to_string());
    }
    let result: serde_json::Map<String, Value> = map
        .into_iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                Value::Array(v.into_iter().map(Value::String).collect()),
            )
        })
        .collect();
    Value::Object(result)
}

/// Build the risk summary map from the compact tool entries.
fn build_risk_summary(tools: &[Value]) -> Value {
    let mut counts: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for tool in tools {
        let risk = tool["risk"].as_str().unwrap_or("unknown");
        *counts.entry(risk).or_insert(0) += 1;
    }
    let result: serde_json::Map<String, Value> = counts
        .into_iter()
        .map(|(k, v)| (k.to_string(), Value::from(v)))
        .collect();
    Value::Object(result)
}

/// Short, bounded list of recommended tool flows for common tasks. Each
/// entry references only known tool names. Kept under 10 entries.
fn tool_manifest_recommended_flows() -> Vec<Value> {
    vec![
        json!({
            "name": "discovery",
            "purpose": "Resolve the project and load rules/context before editing.",
            "tools": ["list_projects", "runtime_status", "read_file"]
        }),
        json!({
            "name": "inspect",
            "purpose": "Use the default inspect tools before editing.",
            "tools": ["read_file", "search_project_text", "show_changes"]
        }),
        json!({
            "name": "edit",
            "purpose": "Prefer structured line edits, batch text edits, or checked patches for source changes.",
            "tools": ["replace_line_range", "insert_at_line", "delete_line_range", "apply_text_edits", "apply_patch_checked"]
        }),
        json!({
            "name": "validate",
            "purpose": "Use structured validation; run_shell is a bounded diagnostics escape hatch, not the primary validation path.",
            "tools": ["cargo_check", "cargo_test", "validate_patch", "run_shell"]
        }),
        json!({
            "name": "review",
            "purpose": "Review diffs and workspace hygiene before the final response.",
            "tools": ["show_changes", "git_diff_hunks", "workspace_hygiene_check"]
        }),
        json!({
            "name": "handoff",
            "purpose": "Summarize or hand off multi-step session state.",
            "tools": ["session_summary", "session_handoff_summary"]
        }),
    ]
}

/// Build the sanitized policy summary JSON exposed in `runtime_status` and
/// `listAgents`. Only the safe fields are carried: `allow_raw_shell`,
/// `allow_cwd_anywhere`, `allowed_roots`, `max_timeout_secs`,
/// `max_output_bytes`. The agent token, shell env values, init_script
/// contents, and full agent.toml contents are NEVER included. Older agents
/// that registered without a policy produce `Value::Null` so the field is
/// present-but-null for clients that expect it.
fn sanitized_policy_summary(policy: Option<&crate::shell_protocol::AgentPolicySummary>) -> Value {
    match policy {
        Some(p) => json!({
            "allow_raw_shell": p.allow_raw_shell,
            "allow_cwd_anywhere": p.allow_cwd_anywhere,
            "allowed_roots": p.allowed_roots,
            "max_timeout_secs": p.max_timeout_secs,
            "max_output_bytes": p.max_output_bytes,
        }),
        None => Value::Null,
    }
}

/// Build the sanitized shell-profiles summary JSON exposed in
/// `runtime_status`, `listAgents`, and `listProjects`. Only safe metadata is
/// carried: default profile name, configured count, prepared-cache count, and
/// per-profile name / has_init_script (boolean) / env_keys_count / program /
/// args_count. NEVER includes init_script bodies, env values, tokens, or the
/// full env snapshot. Older agents that did not report a summary produce
/// `Value::Null`.
fn sanitized_shell_profiles_summary(
    summary: Option<&crate::shell_protocol::ShellProfilesSummary>,
) -> Value {
    match summary {
        Some(s) => {
            let profiles: Vec<Value> = s
                .profiles
                .iter()
                .map(|p| {
                    json!({
                        "name": p.name,
                        "has_init_script": p.has_init_script,
                        "env_keys_count": p.env_keys_count,
                        "program": p.program,
                        "args_count": p.args_count,
                    })
                })
                .collect();
            json!({
                "default_profile": s.default_profile,
                "configured_count": s.configured_count,
                "prepared_cache_count": s.prepared_cache_count,
                "profiles": profiles,
            })
        }
        None => Value::Null,
    }
}

/// Resolve which shell profile a project uses and whether it is configured.
/// Returns `(resolved_name, status)` where:
/// - `resolved_name` = `project_shell_profile` (if set) else the agent's
///   `default_profile` (if any) else `None`.
/// - `status` = `"configured"` if the resolved name exists in the agent's
///   configured profiles; `"missing"` if a name resolved but is not
///   configured; `"not_configured"` if no profile resolves at all; and
///   `"unknown"` if the agent did not report a shell-profiles summary so the
///   configured set cannot be checked.
fn resolve_project_shell_profile(
    project_shell_profile: Option<&str>,
    summary: Option<&crate::shell_protocol::ShellProfilesSummary>,
) -> (Option<String>, &'static str) {
    let resolved = project_shell_profile
        .map(str::to_string)
        .or_else(|| summary.and_then(|s| s.default_profile.clone()));
    match resolved {
        None => (None, "not_configured"),
        Some(name) => match summary {
            None => (Some(name), "unknown"),
            Some(s) => {
                if s.profiles.iter().any(|p| p.name == name) {
                    (Some(name), "configured")
                } else {
                    (Some(name), "missing")
                }
            }
        },
    }
}

#[cfg(test)]
mod tests;

//! Runtime handlers for session and current-session tool calls.

use super::session_context::{
    current_session_key, current_session_unavailable_result, session_lifecycle_denied_result,
    session_message_error_result, unknown_session_result,
};
use super::tool_inputs::SessionMode;
use super::{sessions, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde_json::json;

impl ToolRuntime {
    pub(crate) async fn dispatch_session_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        match call {
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
            ToolCall::SessionSummary { session_id, limit } => {
                self.session_summary_tool(session_id, limit)
            }
            ToolCall::CloseSession { session_id } => self.close_session_tool(session_id),
            ToolCall::ValidationSummary {
                project,
                session_id,
                limit,
            } => {
                self.validation_summary_tool(project, session_id, limit, auth)
                    .await
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
            _ => unreachable!("non-session tool routed to session dispatcher"),
        }
    }

    pub(crate) async fn start_session_tool(
        &self,
        project: Option<String>,
        title: Option<String>,
        mode: SessionMode,
        deny_write_tools: bool,
        deny_shell_tools: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
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
        // Best-effort load of project-local instruction files (AGENTS.md,
        // CLAUDE.md, ...). Any read failure is swallowed and never fails
        // start_session. `null` when no project was provided.
        let project_instructions = match &resolved {
            Some(resolved) => Some(self.load_project_instructions(&resolved.config).await),
            None => None,
        };
        let summary = self
            .sessions
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
            "lifecycle": summary.lifecycle,
            "created_at": summary.created_at,
            "project_instructions": project_instructions,
        }))
    }

    pub(crate) fn session_summary_tool(
        &self,
        session_id: String,
        limit: Option<usize>,
    ) -> ToolResult {
        match self.sessions.summary(&session_id, limit) {
            Some(summary) => ToolResult::ok(
                serde_json::to_value(summary)
                    .unwrap_or_else(|_| json!({"session_id": session_id, "events": []})),
            ),
            None => unknown_session_result(&session_id),
        }
    }

    pub(crate) fn close_session_tool(&self, session_id: String) -> ToolResult {
        match self.sessions.close_session(&session_id) {
            Ok(outcome) => ToolResult::ok(json!({
                "success": true,
                "session_id": outcome.summary.session_id,
                "lifecycle": outcome.summary.lifecycle,
                "already_closed": outcome.already_closed,
                "updated_at": outcome.summary.updated_at,
            })),
            Err(sessions::SessionCloseError::UnknownSession) => unknown_session_result(&session_id),
        }
    }

    pub(crate) fn post_session_message_tool(
        &self,
        session_id: String,
        kind: sessions::SessionMessageKind,
        message: String,
        tags: Vec<String>,
        reply_to: Option<String>,
        priority: sessions::SessionMessagePriority,
    ) -> ToolResult {
        match self
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
        }
    }

    pub(crate) fn list_session_messages_tool(
        &self,
        session_id: String,
        kind: Option<sessions::SessionMessageKind>,
        status: Option<sessions::SessionMessageStatus>,
        limit: Option<usize>,
    ) -> ToolResult {
        match self.sessions.list_messages(
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
        }
    }

    pub(crate) fn resolve_session_message_tool(
        &self,
        session_id: String,
        message_id: String,
        resolution: Option<String>,
    ) -> ToolResult {
        match self
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
        }
    }

    pub(crate) fn session_discussion_summary_tool(
        &self,
        session_id: String,
        limit: Option<usize>,
    ) -> ToolResult {
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

    pub(crate) async fn bind_current_session_tool(
        &self,
        project: String,
        session_id: String,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let Some(summary) = self.sessions.summary(&session_id, None) else {
            return unknown_session_result(&session_id);
        };
        if !summary.lifecycle.allows_mutation() {
            return session_lifecycle_denied_result(
                &session_id,
                "bind_current_session",
                sessions::SessionLifecycleDenial {
                    lifecycle: summary.lifecycle,
                },
            );
        }
        if summary.project.as_deref() != Some(resolved.resolved_id.as_str()) {
            return ToolResult::err_with_output(
                "session_project_mismatch",
                json!({
                    "error_kind": "session_project_mismatch",
                    "failure_kind": "session_project_mismatch",
                    "session_id": session_id,
                    "session_project": summary.project,
                    "project": project,
                    "resolved_project": resolved.resolved_id.clone(),
                    "request_project": resolved.resolved_id,
                    "allow_cross_project_session_required": true,
                    "allow_cross_project_session": false,
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

    pub(crate) async fn current_session_tool(
        &self,
        project: String,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
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

    pub(crate) async fn unbind_current_session_tool(
        &self,
        project: String,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
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
}

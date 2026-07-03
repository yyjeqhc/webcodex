use super::sessions::SessionTransport;
use super::types::{is_checkpoint_kind, is_checkpoint_validation_status};
use super::{
    run_codex_disabled_result, session_guard_denied_result, unknown_session_result, ToolCall,
    ToolResult, ToolRuntime,
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolCallContext<'a> {
    pub(crate) transport: ToolTransport,
    pub(crate) session_id: Option<&'a str>,
    pub(crate) auth: Option<&'a AuthContext>,
    /// REST already recorded OAuth scope denials with session metadata before
    /// this facade existed. MCP rejected scope denials before `_session_id`
    /// became recorder metadata. Keep both adapter-visible behaviors stable.
    pub(crate) record_oauth_scope_denials: bool,
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
}

pub(crate) fn check_oauth_runtime_tool_scope(
    auth: Option<&AuthContext>,
    tool_name: &str,
) -> Result<(), ToolCallErrorStatus> {
    let Some(auth) = auth else {
        return Ok(());
    };
    if !auth.is_oauth_token() {
        return Ok(());
    }

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
        OAuthToolScopePolicy::FirstPartyOnly => Err(ToolCallErrorStatus::InsufficientScope {
            required_scope: None,
            description: "OAuth2 access tokens cannot call first-party-only tools".to_string(),
        }),
        OAuthToolScopePolicy::Unknown => Err(ToolCallErrorStatus::InsufficientScope {
            required_scope: None,
            description: "OAuth2 access tokens cannot call unknown runtime tools".to_string(),
        }),
    }
}

impl ToolRuntime {
    pub(crate) async fn call_tool_with_context(
        &self,
        request: ToolCallRequest,
        context: ToolCallContext<'_>,
    ) -> ToolCallOutcome {
        if let Some(session_id) = context.session_id {
            if !self.sessions.contains_session(session_id) {
                return ToolCallOutcome {
                    success: false,
                    result: Some(unknown_session_result(session_id)),
                    error_status: None,
                    project: None,
                };
            }
        }
        if request.tool_name == "run_codex" {
            let mut result = run_codex_disabled_result();
            if let Some(session_id) = context.session_id {
                let session_event = self.sessions.record_tool_call_started(
                    Some(session_id),
                    context.transport.into(),
                    &request.tool_name,
                    &guard_denial_log_arguments(&request.tool_name, &request.arguments),
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
            };
        }
        if let Some(session_id) = context.session_id {
            if let Some(denial) = self.sessions.guard_denial(session_id, &request.tool_name) {
                let session_event = self.sessions.record_tool_call_started(
                    Some(session_id),
                    context.transport.into(),
                    &request.tool_name,
                    &guard_denial_log_arguments(&request.tool_name, &request.arguments),
                );
                let mut result =
                    session_guard_denied_result(session_id, &request.tool_name, denial);
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
                };
            }
        }

        if !context.record_oauth_scope_denials {
            if let Err(error_status) =
                check_oauth_runtime_tool_scope(context.auth, &request.tool_name)
            {
                return ToolCallOutcome {
                    success: false,
                    result: None,
                    error_status: Some(error_status),
                    project: None,
                };
            }
        }

        let session_log_arguments =
            guard_denial_log_arguments(&request.tool_name, &request.arguments);
        let session_event = self.sessions.record_tool_call_started(
            context.session_id,
            context.transport.into(),
            &request.tool_name,
            &session_log_arguments,
        );

        if context.record_oauth_scope_denials {
            if let Err(error_status) =
                check_oauth_runtime_tool_scope(context.auth, &request.tool_name)
            {
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
                };
            }
        }

        let call = match ToolCall::from_tool_name(&request.tool_name, request.arguments) {
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
                };
            }
        };

        let project = tool_project(&call);
        let mut result = self
            .dispatch_with_auth_transport_options(
                call,
                context.auth,
                context.transport.into(),
                context.session_id.is_none(),
            )
            .await;
        let outer_event_id = self.sessions.record_tool_call_finished(
            session_event,
            result.success,
            &result.output,
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
        }
    }
}

fn guard_denial_log_arguments(tool_name: &str, arguments: &Value) -> Value {
    let Some(obj) = arguments.as_object() else {
        return Value::Null;
    };
    let mut out = serde_json::Map::new();
    if let Some(project) = obj.get("project").cloned() {
        out.insert("project".to_string(), project);
    }
    match tool_name {
        "run_shell" | "run_job" => {
            out.insert(
                "command_present".to_string(),
                Value::Bool(obj.contains_key("command")),
            );
            for key in ["timeout_secs", "cwd"] {
                if let Some(value) = obj.get(key).cloned() {
                    out.insert(key.to_string(), value);
                }
            }
        }
        "run_codex" => {
            out.insert(
                "prompt_present".to_string(),
                Value::Bool(obj.contains_key("prompt")),
            );
            for key in ["approval_mode", "timeout_secs", "cwd"] {
                if let Some(value) = obj.get(key).cloned() {
                    out.insert(key.to_string(), value);
                }
            }
            if let Some(count) = obj
                .get("extra_args")
                .and_then(Value::as_array)
                .map(Vec::len)
            {
                out.insert("extra_args_count".to_string(), Value::from(count));
            }
        }
        "write_project_file" => {
            for key in [
                "path",
                "overwrite",
                "expected_sha256",
                "expected_content_prefix",
            ] {
                if let Some(value) = obj.get(key).cloned() {
                    out.insert(key.to_string(), value);
                }
            }
            out.insert(
                "content_present".to_string(),
                Value::Bool(obj.contains_key("content")),
            );
        }
        "save_project_artifact" => {
            for key in ["path", "mime_type", "overwrite"] {
                if let Some(value) = obj.get(key).cloned() {
                    out.insert(key.to_string(), value);
                }
            }
            out.insert(
                "content_base64_present".to_string(),
                Value::Bool(obj.contains_key("content_base64")),
            );
        }
        "artifact_upload_begin" => {
            copy_keys(
                obj,
                &mut out,
                &["path", "expected_bytes", "mime_type", "overwrite"],
            );
            out.insert(
                "expected_sha256_present".to_string(),
                Value::Bool(obj.contains_key("expected_sha256")),
            );
        }
        "artifact_upload_chunk" => {
            copy_keys(obj, &mut out, &["path", "upload_id", "offset"]);
            out.insert(
                "content_base64_present".to_string(),
                Value::Bool(obj.contains_key("content_base64")),
            );
        }
        "artifact_upload_finish" | "artifact_upload_abort" => {
            copy_keys(obj, &mut out, &["path", "upload_id"]);
        }
        "apply_patch" | "apply_patch_checked" | "validate_patch" => {
            out.insert(
                "patch_present".to_string(),
                Value::Bool(obj.contains_key("patch")),
            );
            if let Some(value) = obj.get("deny_sensitive_paths").cloned() {
                out.insert("deny_sensitive_paths".to_string(), value);
            }
        }
        "replace_in_file" => {
            copy_keys(
                obj,
                &mut out,
                &["path", "expected_replacements", "allow_multiple"],
            );
            out.insert(
                "old_present".to_string(),
                Value::Bool(obj.contains_key("old")),
            );
            out.insert(
                "new_present".to_string(),
                Value::Bool(obj.contains_key("new")),
            );
        }
        "replace_exact_block" => {
            copy_keys(obj, &mut out, &["path", "expected_old_sha256"]);
            out.insert(
                "old_text_present".to_string(),
                Value::Bool(obj.contains_key("old_text")),
            );
            out.insert(
                "new_text_present".to_string(),
                Value::Bool(obj.contains_key("new_text")),
            );
        }
        "insert_before_pattern" | "insert_after_pattern" => {
            copy_keys(obj, &mut out, &["path"]);
            out.insert(
                "pattern_present".to_string(),
                Value::Bool(obj.contains_key("pattern")),
            );
            out.insert(
                "text_present".to_string(),
                Value::Bool(obj.contains_key("text")),
            );
        }
        "replace_line_range" => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "path",
                    "start_line",
                    "end_line",
                    "expected_old_sha256",
                    "expected_old_prefix",
                ],
            );
            out.insert(
                "new_text_present".to_string(),
                Value::Bool(obj.contains_key("new_text")),
            );
        }
        "insert_at_line" => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "path",
                    "line",
                    "expected_anchor_sha256",
                    "expected_anchor_prefix",
                ],
            );
            out.insert(
                "text_present".to_string(),
                Value::Bool(obj.contains_key("text")),
            );
        }
        "delete_line_range" => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "path",
                    "start_line",
                    "end_line",
                    "expected_old_sha256",
                    "expected_old_prefix",
                ],
            );
        }
        "delete_project_files" | "git_restore_paths" | "discard_untracked" => {
            copy_keys(obj, &mut out, &["paths"]);
        }
        "workspace_checkpoint_create" => {
            copy_keys(obj, &mut out, &["title", "include_untracked"]);
            out.insert(
                "note_present".to_string(),
                Value::Bool(obj.contains_key("note")),
            );
            let kind = obj
                .get("kind")
                .and_then(Value::as_str)
                .filter(|value| is_checkpoint_kind(value))
                .unwrap_or(if obj.get("kind").is_some() {
                    "invalid"
                } else {
                    "snapshot"
                });
            out.insert("kind".to_string(), Value::String(kind.to_string()));
            let label_count = obj
                .get("labels")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            out.insert("label_count".to_string(), Value::from(label_count));
            let validation_status = obj
                .get("validation")
                .and_then(Value::as_object)
                .and_then(|validation| validation.get("status"))
                .and_then(Value::as_str)
                .filter(|value| is_checkpoint_validation_status(value))
                .unwrap_or(
                    if obj
                        .get("validation")
                        .and_then(Value::as_object)
                        .and_then(|validation| validation.get("status"))
                        .is_some()
                    {
                        "invalid"
                    } else {
                        "unknown"
                    },
                );
            out.insert(
                "validation_status".to_string(),
                Value::String(validation_status.to_string()),
            );
        }
        "workspace_checkpoint_list" => {
            copy_keys(obj, &mut out, &["limit"]);
        }
        "workspace_checkpoint_show" => {
            copy_keys(obj, &mut out, &["checkpoint_id", "include_diff_stat"]);
        }
        "workspace_checkpoint_restore" | "workspace_checkpoint_delete" => {
            copy_keys(obj, &mut out, &["checkpoint_id", "confirm"]);
        }
        _ => return arguments.clone(),
    }
    Value::Object(out)
}

fn copy_keys(
    obj: &serde_json::Map<String, Value>,
    out: &mut serde_json::Map<String, Value>,
    keys: &[&str],
) {
    for key in keys {
        if let Some(value) = obj.get(*key).cloned() {
            out.insert((*key).to_string(), value);
        }
    }
}

fn tool_project(call: &ToolCall) -> Option<String> {
    call.project().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthContext, AuthKind};
    use crate::config::CodexConfig;
    use crate::projects::ProjectsState;
    use crate::shell_client::ShellClientRegistry;
    use crate::tool_runtime::RuntimeInfo;
    use serde_json::json;
    use std::sync::Arc;

    fn test_runtime() -> ToolRuntime {
        let projects = Arc::new(ProjectsState::failed(
            "projects not configured for test".to_string(),
            "test".to_string(),
        ));
        ToolRuntime::new(
            projects,
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    fn oauth(scopes: &[&str]) -> AuthContext {
        AuthContext {
            kind: AuthKind::OAuth2Token,
            user_id: Some("u".to_string()),
            username: Some("alice".to_string()),
            api_key_id: None,
            api_key_name: None,
            role: Some("user".to_string()),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            is_bootstrap: false,
            token_kind: Some("oauth2".to_string()),
            allowed_client_id: None,
            shared_key_hash: None,
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
                    record_oauth_scope_denials: true,
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
                    record_oauth_scope_denials: false,
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
                    tool_name: "replace_in_file".to_string(),
                    arguments: json!({
                        "project": "demo",
                        "path": "README.md",
                        "old": "secret-old",
                        "new": "secret-new"
                    }),
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: Some(&session.session_id),
                    auth: None,
                    record_oauth_scope_denials: true,
                },
            )
            .await;

        assert!(!outcome.success);
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap();
        let serialized = serde_json::to_string(&summary.events).unwrap();
        assert!(serialized.contains("\"old_present\":true"));
        assert!(serialized.contains("\"new_present\":true"));
        assert!(!serialized.contains("secret-old"));
        assert!(!serialized.contains("secret-new"));
    }

    #[tokio::test]
    async fn tool_kernel_rejects_missing_oauth_scope() {
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
                    record_oauth_scope_denials: true,
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
                    record_oauth_scope_denials: false,
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
                    record_oauth_scope_denials: true,
                },
            )
            .await;
        assert!(matches!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InvalidArguments { .. })
        ));
    }
}

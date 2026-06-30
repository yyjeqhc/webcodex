use super::sessions::SessionTransport;
use super::{ToolCall, ToolResult, ToolRuntime};
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

        let session_event = self.sessions.record_tool_call_started(
            context.session_id,
            context.transport.into(),
            &request.tool_name,
            &request.arguments,
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
        let result = self
            .dispatch_with_auth_transport(call, context.auth, context.transport.into())
            .await;
        self.sessions.record_tool_call_finished(
            session_event,
            result.success,
            &result.output,
            result.error.as_deref(),
            None,
        );
        ToolCallOutcome {
            success: result.success,
            result: Some(result),
            error_status: None,
            project,
        }
    }
}

fn tool_project(call: &ToolCall) -> Option<String> {
    match call {
        ToolCall::RunShell { project, .. }
        | ToolCall::ApplyPatch { project, .. }
        | ToolCall::ApplyPatchChecked { project, .. }
        | ToolCall::DeleteProjectFiles { project, .. }
        | ToolCall::GitRestorePaths { project, .. }
        | ToolCall::DiscardUntracked { project, .. }
        | ToolCall::ValidatePatch { project, .. }
        | ToolCall::ReplaceInFile { project, .. }
        | ToolCall::ReplaceExactBlock { project, .. }
        | ToolCall::InsertBeforePattern { project, .. }
        | ToolCall::InsertAfterPattern { project, .. }
        | ToolCall::WriteProjectFile { project, .. }
        | ToolCall::SaveProjectArtifact { project, .. }
        | ToolCall::ReadProjectArtifactMetadata { project, .. }
        | ToolCall::ReadProjectArtifact { project, .. }
        | ToolCall::ReplaceLineRange { project, .. }
        | ToolCall::InsertAtLine { project, .. }
        | ToolCall::DeleteLineRange { project, .. }
        | ToolCall::GitStatus { project, .. }
        | ToolCall::GitDiff { project, .. }
        | ToolCall::GitDiffHunks { project, .. }
        | ToolCall::CargoFmt { project, .. }
        | ToolCall::CargoCheck { project, .. }
        | ToolCall::CargoTest { project, .. }
        | ToolCall::GitDiffSummary { project, .. }
        | ToolCall::ShowChanges { project, .. }
        | ToolCall::ReadFile { project, .. }
        | ToolCall::ListProjectFiles { project, .. }
        | ToolCall::SearchProjectText { project, .. }
        | ToolCall::RunJob { project, .. }
        | ToolCall::RunCodex { project, .. } => Some(project.clone()),
        _ => None,
    }
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

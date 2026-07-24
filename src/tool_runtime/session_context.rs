use super::sessions;
use super::tool_definition::{
    runtime_tool_allows_current_session_fallback, runtime_tool_is_shell_like,
    runtime_tool_requires_session_project_escape,
};
use super::{ToolCall, ToolResult};
use crate::auth::AuthContext;
use serde_json::{json, Value};

pub(crate) const SESSION_PROJECT_MISMATCH_KIND: &str = "session_project_mismatch";
pub(crate) const ALLOW_CROSS_PROJECT_SESSION_FIELD: &str = "allow_cross_project_session";

#[derive(Debug, Clone)]
pub(crate) struct SessionProjectMismatch {
    pub(crate) session_project: String,
    pub(crate) request_project: String,
}

pub(crate) fn unknown_session_result(session_id: &str) -> ToolResult {
    ToolResult::err_with_output(
        format!("unknown_session_id: {}", session_id),
        json!({
            "error_kind": "unknown_session_id",
            "session_id": session_id,
        }),
    )
}

pub(crate) fn session_project_mismatch_result(
    session_id: &str,
    tool_name: &str,
    mismatch: &SessionProjectMismatch,
) -> ToolResult {
    ToolResult::err_with_output(
        format!(
            "session_project_mismatch: session {} is scoped to project {} but {} requested project {}",
            session_id, mismatch.session_project, tool_name, mismatch.request_project
        ),
        json!({
            "error_kind": SESSION_PROJECT_MISMATCH_KIND,
            "failure_kind": SESSION_PROJECT_MISMATCH_KIND,
            "session_id": session_id,
            "tool_name": tool_name,
            "session_project": mismatch.session_project,
            "request_project": mismatch.request_project,
            "allow_cross_project_session_required": true,
            "allow_cross_project_session": false,
            "command_started": false,
        }),
    )
}

pub(crate) fn session_project_mismatch_warning(
    mismatch: &SessionProjectMismatch,
    allow_cross_project_session: bool,
) -> Value {
    json!({
        "kind": SESSION_PROJECT_MISMATCH_KIND,
        "warning_kind": SESSION_PROJECT_MISMATCH_KIND,
        "session_project": mismatch.session_project,
        "request_project": mismatch.request_project,
        "allow_cross_project_session_required": true,
        "allow_cross_project_session": allow_cross_project_session,
    })
}

pub(crate) fn add_session_project_mismatch_warning(
    result: &mut ToolResult,
    mismatch: &SessionProjectMismatch,
    allow_cross_project_session: bool,
) {
    let warning = session_project_mismatch_warning(mismatch, allow_cross_project_session);
    let mut output = match std::mem::take(&mut result.output) {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
    };

    output.insert(
        "warning_kind".to_string(),
        Value::String(SESSION_PROJECT_MISMATCH_KIND.to_string()),
    );
    output.insert(
        "session_project".to_string(),
        Value::String(mismatch.session_project.clone()),
    );
    output.insert(
        "request_project".to_string(),
        Value::String(mismatch.request_project.clone()),
    );
    output.insert(
        "allow_cross_project_session_required".to_string(),
        Value::Bool(true),
    );
    output.insert(
        "allow_cross_project_session".to_string(),
        Value::Bool(allow_cross_project_session),
    );
    match output.get_mut("warnings") {
        Some(Value::Array(warnings)) => warnings.push(warning),
        _ => {
            output.insert("warnings".to_string(), Value::Array(vec![warning]));
        }
    }
    result.output = Value::Object(output);
}

pub(crate) fn session_project_mismatch_requires_escape(tool_name: &str) -> bool {
    runtime_tool_requires_session_project_escape(tool_name)
}

pub(crate) fn session_guard_denied_result(
    session_id: &str,
    tool_name: &str,
    denial: sessions::SessionGuardDenial,
) -> ToolResult {
    let mut output = json!({
        "error_kind": "session_guard_denied",
        "session_id": session_id,
        "tool_name": tool_name,
        "guard": denial.guard,
        "mode": denial.mode.as_str(),
    });
    if denial.guard == "deny_shell_tools" {
        output["command_started"] = Value::Bool(false);
    }
    ToolResult::err_with_output(
        format!(
            "session_guard_denied: {} blocked by {} session",
            tool_name,
            denial.mode.as_str()
        ),
        output,
    )
}

/// Lifecycle denial for Closed/Archived workflow sessions (write/shell/mutation).
pub(crate) fn session_lifecycle_denied_result(
    session_id: &str,
    tool_name: &str,
    denial: sessions::SessionLifecycleDenial,
) -> ToolResult {
    let lifecycle = denial.lifecycle.as_str();
    let error_kind = match denial.lifecycle {
        sessions::SessionLifecycle::Closed => "session_closed",
        sessions::SessionLifecycle::Archived => "session_archived",
        sessions::SessionLifecycle::Active => "session_lifecycle_denied",
    };
    let mut output = json!({
        "error_kind": error_kind,
        "session_id": session_id,
        "tool_name": tool_name,
        "lifecycle": lifecycle,
    });
    // Match shell-guard shape so callers can detect "command never started".
    if runtime_tool_is_shell_like(tool_name) {
        output["command_started"] = Value::Bool(false);
    }
    ToolResult::err_with_output(
        format!("{error_kind}: {tool_name} blocked on {lifecycle} session"),
        output,
    )
}

pub(crate) fn session_message_error_result(
    session_id: &str,
    message_id: Option<&str>,
    error: sessions::SessionMessageError,
) -> ToolResult {
    match error {
        sessions::SessionMessageError::UnknownSession => unknown_session_result(session_id),
        sessions::SessionMessageError::UnknownMessage => ToolResult::err_with_output(
            match message_id {
                Some(message_id) => format!("unknown_message_id: {}", message_id),
                None => "unknown_message_id".to_string(),
            },
            json!({
                "error_kind": "unknown_message_id",
                "session_id": session_id,
                "message_id": message_id,
            }),
        ),
        sessions::SessionMessageError::SessionClosed { lifecycle } => {
            let error_kind = match lifecycle {
                sessions::SessionLifecycle::Archived => "session_archived",
                _ => "session_closed",
            };
            ToolResult::err_with_output(
                format!("{error_kind}: session message mutation blocked"),
                json!({
                    "error_kind": error_kind,
                    "session_id": session_id,
                    "lifecycle": lifecycle.as_str(),
                }),
            )
        }
        sessions::SessionMessageError::InvalidInput(message) => ToolResult::err_with_output(
            message.clone(),
            json!({
                "error_kind": "invalid_session_message",
                "session_id": session_id,
                "error": message,
            }),
        ),
    }
}

pub(crate) fn current_session_unavailable_result(message: impl Into<String>) -> ToolResult {
    ToolResult::err_with_output(
        message.into(),
        json!({
            "error_kind": "current_session_unavailable",
        }),
    )
}

pub(crate) fn add_session_telemetry_hint(
    result: &mut ToolResult,
    sessions: &sessions::SessionStore,
    session_id: &str,
    event_id: Option<String>,
) {
    let mut output = match std::mem::take(&mut result.output) {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
    };
    output.insert(
        "session_recorded".to_string(),
        Value::Bool(event_id.is_some()),
    );
    // Preserve an existing business `session_id` in the tool output (e.g.
    // session_summary's required business input) instead of overwriting it
    // with the recorder session id. Only synthesize one when the tool output
    // does not already carry one.
    if !output.contains_key("session_id") {
        output.insert(
            "session_id".to_string(),
            Value::String(session_id.to_string()),
        );
    }
    if let Some(event_id) = event_id {
        output.insert("session_event_id".to_string(), Value::String(event_id));
    }
    if let Some(hint) = sessions.inbox_hint(session_id) {
        output.insert(
            "session_hint".to_string(),
            serde_json::to_value(hint).unwrap_or(Value::Null),
        );
    }
    result.output = Value::Object(output);
}

pub(crate) fn is_current_session_eligible(call: &ToolCall) -> bool {
    call.project().is_some() && runtime_tool_allows_current_session_fallback(call.tool_name())
}

pub(crate) fn current_session_key(
    auth: Option<&AuthContext>,
    transport: sessions::SessionTransport,
    resolved_project: &str,
) -> Result<sessions::CurrentSessionKey, String> {
    let (principal_kind, principal_id) = current_session_principal(auth)?;
    Ok(sessions::CurrentSessionKey {
        principal_kind,
        principal_id,
        transport: transport.as_str().to_string(),
        resolved_project: resolved_project.to_string(),
    })
}

pub(crate) fn current_session_principal(
    auth: Option<&AuthContext>,
) -> Result<(String, String), String> {
    let Some(auth) = auth else {
        return Ok(("dev".to_string(), "dev".to_string()));
    };
    if auth.is_bootstrap {
        return Ok((
            "bootstrap".to_string(),
            auth.user_id
                .as_deref()
                .or(auth.username.as_deref())
                .unwrap_or("bootstrap")
                .to_string(),
        ));
    }
    let id = if matches!(auth.kind, crate::auth::AuthKind::OpenAnonymous) {
        Some("open-anonymous".to_string())
    } else {
        auth.api_key_id
            .as_deref()
            .or(auth.user_id.as_deref())
            .or(auth.username.as_deref())
            .or(auth.allowed_client_id.as_deref())
            .or(auth.shared_key_hash.as_deref())
            .or(auth.project_grant_id.as_deref())
            .map(str::to_string)
    };
    let Some(principal_id) = id else {
        return Err(
            "current_session_unavailable: authenticated caller has no stable principal id"
                .to_string(),
        );
    };
    let principal_kind = match auth.kind {
        crate::auth::AuthKind::ApiToken => auth.token_kind.as_deref().unwrap_or("api_token"),
        crate::auth::AuthKind::AgentToken => "agent_token",
        crate::auth::AuthKind::AccountCredential => "account_credential",
        crate::auth::AuthKind::OAuth2Token => "oauth2",
        crate::auth::AuthKind::Bootstrap => "bootstrap",
        crate::auth::AuthKind::SharedKey => "shared-key",
        crate::auth::AuthKind::ProjectCredential => "project-credential",
        crate::auth::AuthKind::OpenAnonymous => "open",
    };
    Ok((principal_kind.to_string(), principal_id))
}

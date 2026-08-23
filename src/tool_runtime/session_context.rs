use super::sessions;
use super::tool_definition::{
    runtime_tool_allows_current_session_fallback, runtime_tool_is_shell_like,
    runtime_tool_requires_session_project_escape,
};
use super::{RecoveryKind, ToolCall, ToolResult};
use crate::auth::AuthContext;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(crate) const SESSION_PROJECT_MISMATCH_KIND: &str = "session_project_mismatch";
pub(crate) const ALLOW_CROSS_PROJECT_SESSION_FIELD: &str = "allow_cross_project_session";
const SESSION_ATTENTION_MAX_MESSAGES: usize = 3;
const SESSION_ATTENTION_MAX_BODY_BYTES: usize = 3072;

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
    .with_recovery(RecoveryKind::FixInput, None)
}

pub(crate) fn session_authority_denied_result(session_id: &str, tool_name: &str) -> ToolResult {
    ToolResult::err_with_output(
        "session_authority_denied",
        json!({
            "error_kind": "session_authority_denied",
            "failure_kind": "session_authority_denied",
            "session_id": session_id,
            "tool_name": tool_name,
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::UserAction, None)
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
            "command_started": false,
        }),
    )
    .with_recovery(RecoveryKind::FixInput, None)
}

/// Project mismatch for a tool whose contract never permits cross-project
/// Session escape. Keep this distinct from the generic mismatch response so
/// callers are not told to supply an unsupported argument.
pub(crate) fn session_project_mismatch_no_escape_result(
    session_id: &str,
    tool_name: &str,
    mismatch: &SessionProjectMismatch,
) -> ToolResult {
    ToolResult::err_with_output(
        format!(
            "session_project_mismatch: session {} is scoped to project {} but {} requested project {}; cross-project escape is not supported",
            session_id, mismatch.session_project, tool_name, mismatch.request_project
        ),
        json!({
            "error_kind": SESSION_PROJECT_MISMATCH_KIND,
            "failure_kind": SESSION_PROJECT_MISMATCH_KIND,
            "session_id": session_id,
            "tool_name": tool_name,
            "session_project": mismatch.session_project,
            "request_project": mismatch.request_project,
            "cross_project_escape_supported": false,
            "command_started": false,
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::FixInput, None)
}

pub(crate) fn session_project_mismatch_warning(
    mismatch: &SessionProjectMismatch,
    allow_cross_project_session: bool,
) -> Value {
    let mut warning = json!({
        "kind": SESSION_PROJECT_MISMATCH_KIND,
        "warning_kind": SESSION_PROJECT_MISMATCH_KIND,
        "session_project": mismatch.session_project,
        "request_project": mismatch.request_project,
    });
    if allow_cross_project_session {
        warning[ALLOW_CROSS_PROJECT_SESSION_FIELD] = Value::Bool(true);
    }
    warning
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
    if allow_cross_project_session {
        output.insert(
            ALLOW_CROSS_PROJECT_SESSION_FIELD.to_string(),
            Value::Bool(true),
        );
    }
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
    .with_recovery(RecoveryKind::NoAction, None)
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
    .with_recovery(RecoveryKind::NoAction, None)
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
        sessions::SessionMessageError::NotTodo => ToolResult::err_with_output(
            "session_message_not_todo",
            json!({
                "error_kind": "session_message_not_todo",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
            }),
        ),
        sessions::SessionMessageError::IdempotencyConflict => ToolResult::err_with_output(
            "idempotency_conflict",
            json!({
                "error_kind": "idempotency_conflict",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
            }),
        ),
        sessions::SessionMessageError::AlreadyCompleted {
            answer_message_id,
            completion_id,
        } => ToolResult::err_with_output(
            "already_completed",
            json!({
                "error_kind": "already_completed",
                "session_id": session_id,
                "message_id": message_id,
                "answer_message_id": answer_message_id,
                "completion_id": completion_id,
                "state_changed": false,
            }),
        ),
        sessions::SessionMessageError::InvalidCompletionState => ToolResult::err_with_output(
            "invalid_completion_state",
            json!({
                "error_kind": "invalid_completion_state",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
            }),
        ),
        sessions::SessionMessageError::InvalidObservationState => ToolResult::err_with_output(
            "invalid_message_observation_state",
            json!({
                "error_kind": "invalid_message_observation_state",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
            }),
        )
        .with_recovery(RecoveryKind::NoAction, None),
        sessions::SessionMessageError::PersistenceUncertain => ToolResult::err_with_output(
            "completion_persistence_uncertain",
            json!({
                "error_kind": "completion_persistence_uncertain",
                "failure_kind": "outcome_unknown",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": true,
                "retry_same_completion": true,
            }),
        )
        .with_recovery(RecoveryKind::RetrySame, None),
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
            .with_recovery(RecoveryKind::NoAction, None)
        }
        sessions::SessionMessageError::InvalidInput(message) => ToolResult::err_with_output(
            message.clone(),
            json!({
                "error_kind": "invalid_session_message",
                "session_id": session_id,
                "error": message,
            }),
        )
        .with_recovery(RecoveryKind::FixInput, None),
    }
}

pub(crate) fn current_session_unavailable_result(message: impl Into<String>) -> ToolResult {
    ToolResult::err_with_output(
        message.into(),
        json!({
            "error_kind": "current_session_unavailable",
        }),
    )
    .with_recovery(RecoveryKind::FixInput, None)
}

#[cfg(test)]
#[test]
fn completion_persistence_uncertain_exposes_exact_retry_same_recovery() {
    let result = session_message_error_result(
        "wc_sess_test",
        Some("wc_msg_test"),
        sessions::SessionMessageError::PersistenceUncertain,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "completion_persistence_uncertain"
    );
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert_eq!(result.output["state_changed"], true);
    assert_eq!(result.output["retry_same_completion"], true);
    assert_eq!(result.output["recovery_kind"], "retry_same");
    assert!(result.output.get("recovery_tool").is_none());
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

pub(crate) fn add_session_attention(
    result: &mut ToolResult,
    sessions: &sessions::SessionStore,
    session_id: &str,
    ack_message_ids: &[String],
) {
    let ack = sessions.observe_message_acks(session_id, ack_message_ids);
    let attention = sessions.ack_required_guidance(session_id, &ack.accepted_ids);
    let unsuppressed_count = attention.messages.len();
    let mut remaining_bytes = SESSION_ATTENTION_MAX_BODY_BYTES;
    let mut messages = Vec::new();
    for message in attention
        .messages
        .into_iter()
        .take(SESSION_ATTENTION_MAX_MESSAGES)
    {
        if remaining_bytes == 0 {
            break;
        }
        let (body, truncated) = bound_utf8_bytes(&message.message, remaining_bytes);
        remaining_bytes = remaining_bytes.saturating_sub(body.len());
        messages.push(json!({
            "message_id": message.message_id,
            "kind": message.kind.as_str(),
            "priority": message.priority,
            "created_at": message.created_at,
            "message": body,
            "message_truncated": truncated,
        }));
    }
    if attention.total_open_requires_ack == 0 && ack_message_ids.is_empty() {
        return;
    }
    let omitted_count = unsuppressed_count.saturating_sub(messages.len());
    let mut output = match std::mem::take(&mut result.output) {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
    };
    output.insert(
        "session_attention".to_string(),
        json!({
            "requires_ack": attention.total_open_requires_ack > 0,
            "messages": messages,
            "omitted_count": omitted_count,
            "truncated": omitted_count > 0,
            "ack": {
                "accepted_count": ack.accepted_count,
                "ignored_count": ack.ignored_count,
            }
        }),
    );
    result.output = Value::Object(output);
}

fn bound_utf8_bytes(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_string(), true)
}

pub(crate) fn is_current_session_eligible(call: &ToolCall) -> bool {
    call.project().is_some() && runtime_tool_allows_current_session_fallback(call.tool_name())
}

pub(crate) fn workflow_session_authority_fingerprint(
    auth: Option<&AuthContext>,
) -> Result<String, String> {
    let (authority_kind, authority_id) = match auth {
        None => ("local-dev", "local-dev".to_string()),
        Some(auth) if auth.is_bootstrap => ("bootstrap", "server-bootstrap".to_string()),
        Some(auth) if auth.is_oauth_shared_key_subject() || auth.is_shared_key() => (
            "shared-key-group",
            stable_workflow_authority_id(
                auth.shared_key_hash.as_deref(),
                "shared-key authority has no stable group identity",
            )?,
        ),
        Some(auth)
            if auth.is_oauth_project_subject()
                || auth.is_project_credential()
                || auth.is_agent_token() =>
        {
            (
                "project-grant",
                stable_workflow_authority_id(
                    auth.project_grant_id.as_deref(),
                    "project-grant authority has no stable grant identity",
                )?,
            )
        }
        Some(auth) if auth.is_open_anonymous() => {
            // Open mode deliberately has one shared authority group. It remains
            // unsuitable for owning a project-less Session, which is rejected at
            // Session creation where there is no project boundary to contain it.
            ("open-anonymous", "open-anonymous".to_string())
        }
        Some(auth)
            if matches!(
                auth.kind,
                crate::auth::AuthKind::ApiToken
                    | crate::auth::AuthKind::AccountCredential
                    | crate::auth::AuthKind::OAuth2Token
            ) =>
        {
            (
                "managed-user",
                stable_workflow_authority_id(
                    auth.user_id.as_deref(),
                    "managed caller has no stable user identity",
                )?,
            )
        }
        Some(_) => {
            return Err(
                "authenticated caller has no canonical Workflow Session authority identity"
                    .to_string(),
            );
        }
    };
    Ok(hash_workflow_session_authority(
        b"webcodex.workflow-session-authority.v1\0",
        authority_kind,
        &authority_id,
    ))
}

fn stable_workflow_authority_id(value: Option<&str>, error: &str) -> Result<String, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| error.to_string())
}

fn hash_workflow_session_authority(domain: &[u8], kind: &str, id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(kind.as_bytes());
    hasher.update(b"\0");
    hasher.update(id.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Compatibility verifier for project-less Sessions written by c3a09275.
/// New Sessions always use the canonical authority-group fingerprint above.
pub(crate) fn legacy_workflow_session_owner_fingerprint(
    auth: Option<&AuthContext>,
) -> Result<String, String> {
    let (principal_kind, principal_id) = match auth {
        None => ("dev".to_string(), "dev".to_string()),
        Some(auth) if auth.is_open_anonymous() => {
            return Err(
                "project-less Workflow Session requires a distinct stable caller identity"
                    .to_string(),
            );
        }
        Some(auth) if auth.is_bootstrap => (
            "bootstrap".to_string(),
            auth.user_id
                .as_deref()
                .or(auth.username.as_deref())
                .unwrap_or("bootstrap")
                .to_string(),
        ),
        Some(auth) if auth.is_oauth_shared_key_subject() => (
            auth.principal_kind().to_string(),
            stable_workflow_authority_id(
                auth.shared_key_hash.as_deref(),
                "OAuth shared-key subject has no stable identity",
            )?,
        ),
        Some(auth) if auth.is_oauth_project_subject() => (
            auth.principal_kind().to_string(),
            stable_workflow_authority_id(
                auth.project_grant_id.as_deref(),
                "OAuth project subject has no stable identity",
            )?,
        ),
        Some(auth) if auth.is_oauth_token() => (
            auth.principal_kind().to_string(),
            auth.user_id
                .as_deref()
                .or(auth.username.as_deref())
                .or(auth.api_key_id.as_deref())
                .map(str::to_string)
                .ok_or_else(|| "OAuth caller has no stable owner identity".to_string())?,
        ),
        Some(auth) if auth.is_shared_key() => (
            auth.principal_kind().to_string(),
            stable_workflow_authority_id(
                auth.shared_key_hash.as_deref(),
                "shared-key caller has no stable owner identity",
            )?,
        ),
        Some(auth) if auth.is_project_credential() => (
            auth.principal_kind().to_string(),
            stable_workflow_authority_id(
                auth.project_grant_id.as_deref(),
                "project credential has no stable owner identity",
            )?,
        ),
        Some(auth) => current_session_principal(Some(auth))?,
    };
    Ok(hash_workflow_session_authority(
        b"webcodex.workflow-session-owner.v1\0",
        &principal_kind,
        &principal_id,
    ))
}

pub(crate) fn current_session_key(
    auth: Option<&AuthContext>,
    transport: sessions::SessionTransport,
    resolved_project: &str,
    repository_root: &str,
    window: Option<&crate::client_window::ClientWindow>,
) -> Result<sessions::CurrentSessionKey, String> {
    let (principal_kind, principal_id) = current_session_principal(auth)?;
    let Some(window) = window else {
        return Err(
            "current_session_unavailable: caller has no stable chat-window identity".to_string(),
        );
    };
    Ok(sessions::CurrentSessionKey {
        principal_kind,
        principal_id,
        transport: transport.as_str().to_string(),
        window_key: window.key().to_string(),
        resolved_project: resolved_project.to_string(),
        repository_root_key: canonical_repository_key(repository_root),
    })
}

pub(crate) fn canonical_repository_key(repository_root: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.workflow-repository-root.v1\0");
    hasher.update(repository_root.as_bytes());
    format!("{:x}", hasher.finalize())
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
    let principal_kind = auth.principal_kind();
    Ok((principal_kind.to_string(), principal_id))
}

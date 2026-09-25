use super::sessions;
use super::tool_definition::runtime_tool_is_shell_like;
use super::{RecoveryKind, ToolResult};
use crate::auth::AuthContext;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(crate) const SESSION_PROJECT_MISMATCH_KIND: &str = "session_project_mismatch";
pub(crate) const SESSION_ATTENTION_MAX_MESSAGES: usize = 3;
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
    .with_recovery(RecoveryKind::FixInput)
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
    .with_recovery(RecoveryKind::UserAction)
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
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::FixInput)
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
    .with_recovery(RecoveryKind::NoAction)
}

/// Lifecycle denial for Closed workflow sessions (write/shell/mutation).
pub(crate) fn session_lifecycle_denied_result(
    session_id: &str,
    tool_name: &str,
    denial: sessions::SessionLifecycleDenial,
) -> ToolResult {
    let lifecycle = denial.lifecycle.as_str();
    let error_kind = match denial.lifecycle {
        sessions::SessionLifecycle::Closed => "session_closed",
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
    .with_recovery(RecoveryKind::NoAction)
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
        sessions::SessionMessageError::MessageNotOpen => ToolResult::err_with_output(
            "session_message_not_open",
            json!({
                "error_kind": "session_message_not_open",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
            }),
        )
        .with_recovery(RecoveryKind::NoAction),
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
        sessions::SessionMessageError::DeliveryKeyConflict => ToolResult::err_with_output(
            "delivery_key_conflict",
            json!({
                "error_kind": "delivery_key_conflict",
                "failure_kind": "conflict",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
            }),
        )
        .with_recovery(RecoveryKind::FixInput),
        sessions::SessionMessageError::DeliveryPersistenceUncertain => {
            ToolResult::err_with_output(
                "message_delivery_persistence_uncertain",
                json!({
                    "error_kind": "message_delivery_persistence_uncertain",
                    "failure_kind": "outcome_unknown",
                    "session_id": session_id,
                    "message_id": message_id,
                    "state_changed": true,
                    "retry_same_delivery": true,
                }),
            )
            .with_recovery(RecoveryKind::RetrySame)
        }
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
        .with_recovery(RecoveryKind::NoAction),
        sessions::SessionMessageError::InvalidAssignmentFence => ToolResult::err_with_output(
            "invalid_assignment_fence",
            json!({
                "error_kind": "invalid_assignment_fence",
                "failure_kind": "invalid_arguments",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
                "retry_guidance": "read the exact assignment with get_session_assignment and pass its opaque assignment_fence unchanged",
            }),
        )
        .with_recovery(RecoveryKind::FixInput),
        sessions::SessionMessageError::AssignmentStale {
            current,
            fresh_assignment_fence,
        } => ToolResult::err_with_output(
            "assignment_stale",
            json!({
                "error_kind": "assignment_stale",
                "failure_kind": "conflict",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
                "current_assignment": current,
                "fresh_assignment_fence": fresh_assignment_fence,
                "retry_guidance": "re-evaluate the returned current assignment; when fresh_assignment_fence is present it is the durable fence for exactly that returned state, otherwise call get_session_assignment again",
            }),
        )
        .with_recovery(RecoveryKind::Reobserve),
        sessions::SessionMessageError::AssignmentHistoryLost { current } => ToolResult::err_with_output(
            "assignment_history_lost",
            json!({
                "error_kind": "assignment_history_lost",
                "failure_kind": "history_lost",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
                "current_assignment": current,
                "retry_guidance": "retained state cannot prove the full exact assignment; do not complete this todo from stale context",
            }),
        )
        .with_recovery(RecoveryKind::NoAction),
        sessions::SessionMessageError::AssignmentTooLarge {
            reply_count,
            max_replies,
            current,
        } => ToolResult::err_with_output(
            "assignment_too_large",
            json!({
                "error_kind": "assignment_too_large",
                "failure_kind": "bounded_output_exceeded",
                "session_id": session_id,
                "message_id": message_id,
                "state_changed": false,
                "reply_count": reply_count,
                "max_replies": max_replies,
                "current_assignment": current,
                "retry_guidance": "the coordinator must consolidate or supersede this assignment before a fenced completion can be issued",
            }),
        )
        .with_recovery(RecoveryKind::NoAction),
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
        .with_recovery(RecoveryKind::RetrySame),
        sessions::SessionMessageError::SessionClosed { lifecycle } => ToolResult::err_with_output(
            "session_closed: session message mutation blocked",
            json!({
                "error_kind": "session_closed",
                "session_id": session_id,
                "lifecycle": lifecycle.as_str(),
            }),
        )
        .with_recovery(RecoveryKind::NoAction),
        sessions::SessionMessageError::InvalidInput(message) => ToolResult::err_with_output(
            message.clone(),
            json!({
                "error_kind": "invalid_session_message",
                "session_id": session_id,
                "error": message,
            }),
        )
        .with_recovery(RecoveryKind::FixInput),
    }
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

pub(crate) fn add_session_hint(
    result: &mut ToolResult,
    sessions: &sessions::SessionStore,
    session_id: &str,
) {
    let hint = sessions.inbox_hint(session_id);
    if let Some(output) = result.output.as_object_mut() {
        // The outer recorder is authoritative only for collaboration guidance.
        // Recorder provenance stays ledger-only, and any business `session_id`
        // produced by the concrete tool is preserved untouched.
        output.remove("session_hint");
        if let Some(hint) = hint {
            output.insert(
                "session_hint".to_string(),
                serde_json::to_value(hint).unwrap_or(Value::Null),
            );
        }
        return;
    }
    if let Some(hint) = hint {
        let prior = std::mem::take(&mut result.output);
        result.output = json!({
            "value": prior,
            "session_hint": hint,
        });
    }
}

pub(crate) fn observe_session_attention_acks(
    sessions: &sessions::SessionStore,
    session_id: &str,
    ack_message_ids: &[String],
) -> sessions::SessionAckObservation {
    sessions.observe_message_acks(session_id, ack_message_ids)
}

pub(crate) fn session_ack_message_ids(
    sessions: &sessions::SessionStore,
    session_id: &str,
    legacy_ids: &[String],
    ack_ref: Option<&str>,
) -> Vec<String> {
    let mut ids = legacy_ids.to_vec();
    if let Some(resolved) =
        ack_ref.and_then(|ack_ref| sessions.resolve_ack_ref(session_id, ack_ref))
    {
        let mut seen = ids
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        for message_id in resolved {
            if seen.insert(message_id.clone()) {
                ids.push(message_id);
            }
        }
    }
    ids
}

pub(crate) fn add_session_attention_projection(
    result: &mut ToolResult,
    sessions: &sessions::SessionStore,
    session_id: &str,
    source: &str,
    ack: &sessions::SessionAckObservation,
    ack_requested: bool,
) {
    let attention = sessions.ack_required_messages(session_id, &ack.accepted_ids);
    let unsuppressed_count = attention.messages.len();
    let mut remaining_bytes = SESSION_ATTENTION_MAX_BODY_BYTES;
    let mut messages = Vec::new();
    let mut projected_ids = Vec::new();
    let mut body_truncated = false;
    for message in attention
        .messages
        .into_iter()
        .take(SESSION_ATTENTION_MAX_MESSAGES)
    {
        if remaining_bytes == 0 {
            break;
        }
        let (body, truncated) = bound_utf8_bytes(&message.message, remaining_bytes);
        body_truncated |= truncated;
        remaining_bytes = remaining_bytes.saturating_sub(body.len());
        projected_ids.push(message.message_id.clone());
        messages.push(json!({
            "message_id": message.message_id,
            "kind": message.kind.as_str(),
            "priority": message.priority,
            "created_at": message.created_at,
            "message": body,
            "message_truncated": truncated,
        }));
    }
    if attention.total_open_requires_ack == 0 && !ack_requested {
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
    // Strong hint fields are a counts-only fallback. Once this response has
    // fully conveyed every unacknowledged ACK-required body (or the request ACK
    // has suppressed it), keep only the ordinary inbox counts/tool suggestion.
    // Preserve the strong fallback when any required body is omitted or truncated.
    if omitted_count == 0 && !body_truncated {
        if let Some(hint) = output
            .get_mut("session_hint")
            .and_then(Value::as_object_mut)
        {
            hint.remove("attention_required");
            hint.remove("attention_reason");
            hint.remove("attention_instruction");
        }
    }
    let mut retained_ack_ids = ack.accepted_ids.clone();
    retained_ack_ids.extend(projected_ids);
    let ack_ref = sessions.issue_ack_ref(session_id, &retained_ack_ids);
    let mut projection = json!({
        "session_id": session_id,
        "source": source,
        "requires_ack": attention.total_open_requires_ack > 0,
        "messages": messages,
        "omitted_count": omitted_count,
        "truncated": omitted_count > 0,
        "ack": {
            "accepted_count": ack.accepted_count,
            "ignored_count": ack.ignored_count,
        }
    });
    if let Some(ack_ref) = ack_ref {
        projection["ack_ref"] = json!(ack_ref);
    }
    output.insert("session_attention".to_string(), projection);
    result.output = Value::Object(output);
}

#[cfg(test)]
pub(crate) fn add_session_attention(
    result: &mut ToolResult,
    sessions: &sessions::SessionStore,
    session_id: &str,
    ack_message_ids: &[String],
) {
    let ack = observe_session_attention_acks(sessions, session_id, ack_message_ids);
    add_session_attention_projection(
        result,
        sessions,
        session_id,
        "recording_session",
        &ack,
        !ack_message_ids.is_empty(),
    );
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

fn stable_caller_authority_identity(
    auth: Option<&AuthContext>,
) -> Result<(&'static str, String), String> {
    let (authority_kind, authority_id) = match auth {
        None => ("local-dev", "local-dev".to_string()),
        Some(auth) if auth.is_bootstrap => ("bootstrap", "server-bootstrap".to_string()),
        Some(auth) if auth.is_oauth_shared_key_subject() || auth.is_shared_key() => (
            "shared-key-group",
            stable_authority_id(
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
                stable_authority_id(
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
                stable_authority_id(
                    auth.user_id.as_deref(),
                    "managed caller has no stable user identity",
                )?,
            )
        }
        Some(_) => {
            return Err("authenticated caller has no canonical authority identity".to_string());
        }
    };
    Ok((authority_kind, authority_id))
}

pub(crate) fn workflow_session_authority_fingerprint(
    auth: Option<&AuthContext>,
) -> Result<String, String> {
    let (authority_kind, authority_id) = stable_caller_authority_identity(auth)?;
    Ok(hash_authority_identity(
        b"webcodex.workflow-session-authority.v1\0",
        authority_kind,
        &authority_id,
    ))
}

pub(crate) fn workflow_session_incarnation_fingerprint(
    session_id: &str,
    created_at: i64,
    project: Option<&str>,
    owner_authority_fingerprint: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.workflow-session-incarnation.v1\0");
    hasher.update(session_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(created_at.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(project.unwrap_or_default().as_bytes());
    hasher.update(b"\0");
    hasher.update(owner_authority_fingerprint.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn project_reference_principal_fingerprint(
    auth: Option<&AuthContext>,
) -> Result<String, String> {
    let (authority_kind, authority_id) = stable_caller_authority_identity(auth)?;
    Ok(hash_authority_identity(
        b"webcodex.project-reference-principal.v1\0",
        authority_kind,
        &authority_id,
    ))
}

fn stable_authority_id(value: Option<&str>, error: &str) -> Result<String, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| error.to_string())
}

fn hash_authority_identity(domain: &[u8], kind: &str, id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(kind.as_bytes());
    hasher.update(b"\0");
    hasher.update(id.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn canonical_repository_key(repository_root: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.workflow-repository-root.v1\0");
    hasher.update(repository_root.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn runtime_observation_principal(
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
            "runtime_observation_unavailable: authenticated caller has no stable principal id"
                .to_string(),
        );
    };
    let principal_kind = auth.principal_kind();
    Ok((principal_kind.to_string(), principal_id))
}

//! Atomic Session assignment snapshots and deterministic completion fences.
//!
//! The fence is conflict-detection metadata only. It is a Session/todo-bound
//! semantic snapshot fingerprint of the retained todo thread. It is never
//! authority, a completion key, a message-observation token, or credential
//! identity.

use super::model::{
    SessionAssignmentCurrentState, SessionAssignmentSnapshot, SessionMessage, SessionMessageError,
    SessionMessageKind, SessionMessageStatus, SessionRecord, MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES,
    MAX_SESSION_ASSIGNMENT_FENCE_LEN,
};
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};

const ASSIGNMENT_FENCE_PREFIX: &str = "wsa1_";
const ASSIGNMENT_DIGEST_BYTES: usize = 32;
const ASSIGNMENT_FENCE_PAYLOAD_BYTES: usize = 32;

#[derive(Debug, Clone)]
pub(super) struct AssignmentState {
    pub(super) todo: SessionMessage,
    pub(super) direct_replies: Vec<SessionMessage>,
    pub(super) semantic_digest: [u8; ASSIGNMENT_DIGEST_BYTES],
}

pub(super) fn open_assignment_state(
    record: &SessionRecord,
    todo_id: &str,
) -> Result<AssignmentState, SessionMessageError> {
    let (todo, direct_replies, reply_count) = retained_assignment_thread(record, todo_id)?;
    if todo.status != SessionMessageStatus::Open {
        return Err(SessionMessageError::MessageNotOpen);
    }
    if todo.closure_kind.is_some()
        || todo.resolved_at.is_some()
        || todo.resolution.is_some()
        || todo.resolved_by_message_id.is_some()
        || todo.completion_id.is_some()
    {
        return Err(SessionMessageError::InvalidCompletionState);
    }
    if reply_count > MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES {
        return Err(SessionMessageError::AssignmentTooLarge {
            reply_count,
            max_replies: MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES,
            current: current_state(todo, direct_replies, true),
        });
    }

    let Some(todo_revision) = record
        .message_observation_revisions
        .get(&todo.message_id)
        .copied()
    else {
        return Err(SessionMessageError::InvalidObservationState);
    };
    if todo_revision > record.message_observation_revision {
        return Err(SessionMessageError::InvalidObservationState);
    }
    // Retention proof is assignment-local. Unrelated message eviction must not
    // stale or block this todo, while any evicted direct reply makes the exact
    // assignment history incomplete and therefore fails closed.
    if !record.assignment_history_tracking_complete
        || record.assignment_history_floors.contains_key(todo_id)
    {
        return Err(SessionMessageError::AssignmentHistoryLost {
            current: current_state(todo, direct_replies, false),
        });
    }
    for reply in &direct_replies {
        let Some(revision) = record
            .message_observation_revisions
            .get(&reply.message_id)
            .copied()
        else {
            return Err(SessionMessageError::InvalidObservationState);
        };
        if revision > record.message_observation_revision {
            return Err(SessionMessageError::InvalidObservationState);
        }
    }

    let semantic_digest = assignment_semantic_digest(&todo, &direct_replies);
    Ok(AssignmentState {
        todo,
        direct_replies,
        semantic_digest,
    })
}

pub(super) fn current_assignment_state(
    record: &SessionRecord,
    todo_id: &str,
) -> Result<SessionAssignmentCurrentState, SessionMessageError> {
    let (todo, direct_replies, reply_count) = retained_assignment_thread(record, todo_id)?;
    Ok(current_state(
        todo,
        direct_replies,
        reply_count > MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES,
    ))
}

pub(super) fn assignment_fence_from_state(
    session_id: &str,
    todo_id: &str,
    state: &AssignmentState,
) -> String {
    encode_assignment_fence(session_id, todo_id, &state.semantic_digest)
}

pub(super) fn snapshot_from_state(
    session_id: &str,
    todo_id: &str,
    state: AssignmentState,
) -> SessionAssignmentSnapshot {
    let assignment_fence = assignment_fence_from_state(session_id, todo_id, &state);
    SessionAssignmentSnapshot {
        todo: state.todo,
        direct_replies: state.direct_replies,
        assignment_fence,
    }
}

pub(super) fn assignment_semantic_digest(
    todo: &SessionMessage,
    direct_replies: &[SessionMessage],
) -> [u8; ASSIGNMENT_DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex/session-assignment/semantic/v1\0");
    hash_semantic_message(&mut hasher, todo);
    for reply in direct_replies {
        hash_semantic_message(&mut hasher, reply);
    }
    hasher.finalize().into()
}

pub(super) fn assignment_fence_fingerprint(token: &str) -> String {
    general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

pub(super) fn is_valid_assignment_fence_fingerprint(value: &str) -> bool {
    general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .is_ok_and(|decoded| decoded.len() == 32)
}

pub(super) fn validate_assignment_fence(token: &str) -> Result<(), SessionMessageError> {
    if token.len() != MAX_SESSION_ASSIGNMENT_FENCE_LEN || !token.is_ascii() {
        return Err(SessionMessageError::InvalidAssignmentFence);
    }
    let encoded = token
        .strip_prefix(ASSIGNMENT_FENCE_PREFIX)
        .ok_or(SessionMessageError::InvalidAssignmentFence)?;
    let payload = general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| SessionMessageError::InvalidAssignmentFence)?;
    if payload.len() != ASSIGNMENT_FENCE_PAYLOAD_BYTES
        || general_purpose::URL_SAFE_NO_PAD.encode(&payload) != encoded
    {
        return Err(SessionMessageError::InvalidAssignmentFence);
    }
    Ok(())
}

fn retained_assignment_thread(
    record: &SessionRecord,
    todo_id: &str,
) -> Result<(SessionMessage, Vec<SessionMessage>, usize), SessionMessageError> {
    let todo = record
        .messages
        .iter()
        .find(|message| message.message_id == todo_id)
        .ok_or(SessionMessageError::UnknownMessage)?
        .as_ref()
        .clone();
    if todo.kind != SessionMessageKind::Todo {
        return Err(SessionMessageError::NotTodo);
    }
    let mut direct_replies = record
        .messages
        .iter()
        .filter(|message| message.reply_to.as_deref() == Some(todo_id))
        .map(|message| message.as_ref().clone())
        .collect::<Vec<_>>();
    let reply_count = direct_replies.len();
    direct_replies.truncate(MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES);
    Ok((todo, direct_replies, reply_count))
}

fn current_state(
    todo: SessionMessage,
    direct_replies: Vec<SessionMessage>,
    direct_replies_truncated: bool,
) -> SessionAssignmentCurrentState {
    SessionAssignmentCurrentState {
        todo,
        direct_replies,
        direct_replies_truncated,
    }
}

fn hash_semantic_message(hasher: &mut Sha256, message: &SessionMessage) {
    let mut semantic = message.clone();
    // ACK bookkeeping is delivery/context metadata, not assignment meaning.
    semantic.first_ack_observed_at = None;
    let encoded = serde_json::to_vec(&semantic).expect("SessionMessage serialization must succeed");
    hasher.update((encoded.len() as u64).to_be_bytes());
    hasher.update(encoded);
}

fn encode_assignment_fence(
    session_id: &str,
    todo_id: &str,
    semantic_digest: &[u8; ASSIGNMENT_DIGEST_BYTES],
) -> String {
    let payload = scoped_digest(
        b"webcodex/session-assignment/fence/v1\0",
        session_id,
        todo_id,
        semantic_digest,
    );
    let token = format!(
        "{ASSIGNMENT_FENCE_PREFIX}{}",
        general_purpose::URL_SAFE_NO_PAD.encode(payload)
    );
    debug_assert_eq!(token.len(), MAX_SESSION_ASSIGNMENT_FENCE_LEN);
    token
}

fn scoped_digest(domain: &[u8], session_id: &str, todo_id: &str, extra: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((session_id.len() as u64).to_be_bytes());
    hasher.update(session_id.as_bytes());
    hasher.update((todo_id.len() as u64).to_be_bytes());
    hasher.update(todo_id.as_bytes());
    hasher.update((extra.len() as u64).to_be_bytes());
    hasher.update(extra);
    hasher.finalize().into()
}

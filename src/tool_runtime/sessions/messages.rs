//! Session message board: post / list / resolve and discussion aggregates.
//!
//! All message-map mutations go through `SessionStoreInner` helpers.

use super::assignment::{open_assignment_state, snapshot_from_state};
use super::model::{
    CompleteSessionMessageInput, CompleteSessionMessageOutcome, ListSessionMessagesFilter,
    PostSessionMessageInput, ReplaceSessionMessageInput, ReplaceSessionMessageOutcome,
    SessionAckObservation, SessionAssignmentSnapshot, SessionAttentionSnapshot,
    SessionDiscussionSummary, SessionInboxHint, SessionMessage, SessionMessageError,
    SessionMessageKind, SessionMessageObservationError, SessionMessageObservationOutcome,
    SessionMessagePriority, SessionMessageStatus, WithdrawSessionMessageOutcome,
    DEFAULT_MESSAGE_LIST_LIMIT, MAX_MESSAGE_LIST_LIMIT, MAX_SESSION_MESSAGE_OBSERVATION_TOKEN_LEN,
};
use super::query::{build_discussion_summary, build_inbox_hint};
use super::store::SessionStore;
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};

impl SessionStore {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn post_message(
        &self,
        input: PostSessionMessageInput,
    ) -> Result<SessionMessage, SessionMessageError> {
        self.post_message_with_ack(input, false)
    }

    pub(crate) fn post_message_with_ack(
        &self,
        input: PostSessionMessageInput,
        requires_ack: bool,
    ) -> Result<SessionMessage, SessionMessageError> {
        let (message, changed) = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.post_message(input, requires_ack)?
        };
        self.persist_after_mutation();
        if changed {
            self.notify_message_observation();
        }
        Ok(message)
    }

    pub(crate) fn list_messages(
        &self,
        session_id: &str,
        filter: ListSessionMessagesFilter,
    ) -> Result<Vec<SessionMessage>, SessionMessageError> {
        self.with_record_for_query(session_id, |record, _| {
            let limit = filter
                .limit
                .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
                .clamp(0, MAX_MESSAGE_LIST_LIMIT);
            record
                .messages
                .iter()
                .filter(|message| filter.kind.is_none_or(|kind| message.kind == kind))
                .filter(|message| filter.status.is_none_or(|status| message.status == status))
                .filter(|message| {
                    filter
                        .message_id
                        .as_deref()
                        .is_none_or(|message_id| message.message_id == message_id)
                })
                .filter(|message| {
                    filter
                        .reply_to
                        .as_deref()
                        .is_none_or(|reply_to| message.reply_to.as_deref() == Some(reply_to))
                })
                .rev()
                .take(limit)
                .map(|message| message.as_ref().clone())
                .collect()
        })
        .ok_or(SessionMessageError::UnknownSession)
    }

    /// Read one exact open todo and every retained direct reply under one
    /// Session-store snapshot. The durable persistence barrier is completed
    /// before the opaque fence is returned so a post-restart retry can compare
    /// against the same observation state.
    pub(crate) fn get_assignment(
        &self,
        session_id: &str,
        todo_id: &str,
    ) -> Result<SessionAssignmentSnapshot, SessionMessageError> {
        let state = self
            .with_record_for_query(session_id, |record, _| {
                open_assignment_state(record, todo_id)
            })
            .ok_or(SessionMessageError::UnknownSession)??;
        if self.persist_after_mutation_durable().is_err() {
            return Err(SessionMessageError::InvalidObservationState);
        }
        Ok(snapshot_from_state(session_id, todo_id, state))
    }

    pub(crate) fn observe_message_acks(
        &self,
        session_id: &str,
        message_ids: &[String],
    ) -> SessionAckObservation {
        if message_ids.is_empty() {
            return SessionAckObservation::default();
        }
        let outcome = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.observe_message_acks(session_id, message_ids)
        };
        if outcome.first_observed_count > 0 {
            self.persist_after_mutation();
            self.notify_message_observation();
        }
        outcome
    }

    pub(crate) fn ack_required_guidance(
        &self,
        session_id: &str,
        suppressed_ids: &[String],
    ) -> SessionAttentionSnapshot {
        let suppressed = suppressed_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        self.with_record_for_query(session_id, |record, _| {
            let mut open = record
                .messages
                .iter()
                .filter(|message| {
                    message.status == SessionMessageStatus::Open
                        && message.kind == SessionMessageKind::Guidance
                        && message.priority == SessionMessagePriority::High
                        && message.requires_ack
                })
                .map(|message| message.as_ref().clone())
                .collect::<Vec<_>>();
            open.sort_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.message_id.cmp(&right.message_id))
            });
            let total_open_requires_ack = open.len();
            open.retain(|message| !suppressed.contains(message.message_id.as_str()));
            SessionAttentionSnapshot {
                messages: open,
                total_open_requires_ack,
            }
        })
        .unwrap_or_default()
    }

    pub(crate) fn withdraw_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<WithdrawSessionMessageOutcome, SessionMessageError> {
        let outcome = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.withdraw_message(session_id, message_id)?
        };
        if self.persist_after_mutation_durable().is_err() {
            if !outcome.replayed {
                self.notify_message_observation();
            }
            return Err(SessionMessageError::PersistenceUncertain);
        }
        if !outcome.replayed {
            self.notify_message_observation();
        }
        Ok(outcome)
    }

    pub(crate) fn replace_message(
        &self,
        input: ReplaceSessionMessageInput,
    ) -> Result<ReplaceSessionMessageOutcome, SessionMessageError> {
        let outcome = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.replace_message(input)?
        };
        if self.persist_after_mutation_durable().is_err() {
            if !outcome.replayed {
                self.notify_message_observation();
            }
            return Err(SessionMessageError::PersistenceUncertain);
        }
        if !outcome.replayed {
            self.notify_message_observation();
        }
        Ok(outcome)
    }

    pub(crate) fn resolve_message(
        &self,
        session_id: &str,
        message_id: &str,
        resolution: Option<String>,
    ) -> Result<SessionMessage, SessionMessageError> {
        let (message, changed) = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.resolve_message(session_id, message_id, resolution)?
        };
        self.persist_after_mutation();
        if changed {
            self.notify_message_observation();
        }
        Ok(message)
    }

    pub(crate) fn resolve_message_from_wrapper(
        &self,
        session_id: &str,
        message_id: &str,
        resolution: String,
        current_request_acknowledged: bool,
    ) -> Result<SessionMessage, SessionMessageError> {
        let (message, changed) = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.resolve_message_from_wrapper(
                session_id,
                message_id,
                resolution,
                current_request_acknowledged,
            )?
        };
        self.persist_after_mutation();
        if changed {
            self.notify_message_observation();
        }
        Ok(message)
    }

    pub(crate) fn complete_message(
        &self,
        input: CompleteSessionMessageInput,
    ) -> Result<CompleteSessionMessageOutcome, SessionMessageError> {
        let result = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.complete_message(input)
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(SessionMessageError::AssignmentStale {
                current,
                fresh_assignment_fence,
            }) => {
                if fresh_assignment_fence.is_some()
                    && self.persist_after_mutation_durable().is_err()
                {
                    return Err(SessionMessageError::AssignmentStale {
                        current,
                        fresh_assignment_fence: None,
                    });
                }
                return Err(SessionMessageError::AssignmentStale {
                    current,
                    fresh_assignment_fence,
                });
            }
            Err(error) => return Err(error),
        };
        if self.persist_after_mutation_durable().is_err() {
            if !outcome.replayed {
                self.notify_message_observation();
            }
            return Err(SessionMessageError::PersistenceUncertain);
        }
        if !outcome.replayed {
            self.notify_message_observation();
        }
        Ok(outcome)
    }

    pub(crate) async fn observe_messages(
        &self,
        session_id: &str,
        after_observation_token: Option<&str>,
        wait_secs: Option<u64>,
        limit: Option<usize>,
    ) -> Result<SessionMessageObservationOutcome, SessionMessageObservationError> {
        let limit = limit
            .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
            .clamp(1, MAX_MESSAGE_LIST_LIMIT);
        if after_observation_token.is_none() {
            let current_revision = self.message_observation_current_revision(session_id)?;
            self.fence_message_observation_token()?;
            return Ok(SessionMessageObservationOutcome {
                messages: Vec::new(),
                observation_token: encode_observation_token(session_id, current_revision)?,
                changed: false,
                wait_outcome: "immediate",
                waited_ms: 0,
                history_lost: false,
                has_more: false,
            });
        }

        let after_revision =
            parse_observation_token(after_observation_token.expect("checked above"), session_id)?;
        let mut receiver = self.message_observation_notify.subscribe();
        let initial = self.message_observation_snapshot(session_id, after_revision, limit)?;
        if initial.2 || wait_secs.is_none() {
            self.fence_message_observation_token()?;
            return observation_outcome(session_id, initial, "immediate", 0);
        }

        let wait_secs = wait_secs.expect("checked above");
        let started = std::time::Instant::now();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(wait_secs);
        loop {
            match tokio::time::timeout_at(deadline, receiver.changed()).await {
                Ok(Ok(())) => {
                    let snapshot =
                        self.message_observation_snapshot(session_id, after_revision, limit)?;
                    if snapshot.2 {
                        self.fence_message_observation_token()?;
                        return observation_outcome(
                            session_id,
                            snapshot,
                            "updated",
                            elapsed_millis(started),
                        );
                    }
                }
                Ok(Err(_)) | Err(_) => {
                    // Re-snapshot after the deadline so a mutation racing the
                    // timeout cannot be lost between the last check and return.
                    let snapshot =
                        self.message_observation_snapshot(session_id, after_revision, limit)?;
                    if snapshot.2 {
                        self.fence_message_observation_token()?;
                        return observation_outcome(
                            session_id,
                            snapshot,
                            "updated",
                            elapsed_millis(started),
                        );
                    }
                    self.fence_message_observation_token()?;
                    return observation_outcome(
                        session_id,
                        snapshot,
                        "timeout",
                        elapsed_millis(started),
                    );
                }
            }
        }
    }

    fn message_observation_current_revision(
        &self,
        session_id: &str,
    ) -> Result<u64, SessionMessageObservationError> {
        self.with_record_for_query(session_id, |record, _| record.message_observation_revision)
            .ok_or(SessionMessageObservationError::UnknownSession)
    }

    fn message_observation_snapshot(
        &self,
        session_id: &str,
        after_revision: u64,
        limit: usize,
    ) -> Result<(Vec<SessionMessage>, u64, bool, bool, bool), SessionMessageObservationError> {
        self.with_record_for_query(session_id, |record, _| {
            if after_revision > record.message_observation_revision {
                return Err(SessionMessageObservationError::FutureRevision);
            }
            let history_lost = after_revision < record.message_observation_floor;
            let changed = record.message_observation_revision > after_revision;
            let mut candidates = record
                .messages
                .iter()
                .filter_map(|message| {
                    let revision = *record
                        .message_observation_revisions
                        .get(&message.message_id)
                        .unwrap_or(&0);
                    (revision > after_revision).then_some((revision, message.as_ref().clone()))
                })
                .collect::<Vec<_>>();
            candidates.sort_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.message_id.cmp(&right.1.message_id))
            });
            let has_more = candidates.len() > limit;
            candidates.truncate(limit);
            let next_revision = if has_more {
                candidates
                    .last()
                    .map(|(revision, _)| *revision)
                    .unwrap_or(after_revision)
            } else {
                record.message_observation_revision
            };
            Ok((
                candidates.into_iter().map(|(_, message)| message).collect(),
                next_revision,
                changed,
                history_lost,
                has_more,
            ))
        })
        .ok_or(SessionMessageObservationError::UnknownSession)?
    }

    fn fence_message_observation_token(&self) -> Result<(), SessionMessageObservationError> {
        self.persist_after_mutation_durable()
            .map_err(|_| SessionMessageObservationError::InvalidObservationState)
    }

    fn notify_message_observation(&self) {
        self.message_observation_notify
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    pub(crate) fn discussion_summary(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<SessionDiscussionSummary, SessionMessageError> {
        self.with_record_for_query(session_id, |record, _| {
            let limit = limit
                .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
                .clamp(0, MAX_MESSAGE_LIST_LIMIT);
            build_discussion_summary(record, limit)
        })
        .ok_or(SessionMessageError::UnknownSession)
    }

    pub(crate) fn inbox_hint(&self, session_id: &str) -> Option<SessionInboxHint> {
        self.with_record_for_query(session_id, |record, _| build_inbox_hint(record))
            .flatten()
    }
}

fn elapsed_millis(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn observation_outcome(
    session_id: &str,
    snapshot: (Vec<SessionMessage>, u64, bool, bool, bool),
    wait_outcome: &'static str,
    waited_ms: u64,
) -> Result<SessionMessageObservationOutcome, SessionMessageObservationError> {
    let (messages, next_revision, changed, history_lost, has_more) = snapshot;
    Ok(SessionMessageObservationOutcome {
        messages,
        observation_token: encode_observation_token(session_id, next_revision)?,
        changed,
        wait_outcome,
        waited_ms,
        history_lost,
        has_more,
    })
}

const MESSAGE_OBSERVATION_TOKEN_PREFIX: &str = "wsm1_";
const MESSAGE_OBSERVATION_BINDING_BYTES: usize = 16;
const MESSAGE_OBSERVATION_REVISION_BYTES: usize = 8;
const MESSAGE_OBSERVATION_TAG_BYTES: usize = 16;
const MESSAGE_OBSERVATION_PAYLOAD_BYTES: usize = MESSAGE_OBSERVATION_BINDING_BYTES
    + MESSAGE_OBSERVATION_REVISION_BYTES
    + MESSAGE_OBSERVATION_TAG_BYTES;

pub(super) fn encode_observation_token(
    session_id: &str,
    revision: u64,
) -> Result<String, SessionMessageObservationError> {
    let binding = observation_digest(
        b"webcodex.session-message-observation.binding.v1\0",
        session_id,
        &[],
    );
    let mask = observation_digest(
        b"webcodex.session-message-observation.mask.v1\0",
        session_id,
        &[],
    );
    let revision_bytes = revision.to_be_bytes();
    let mut masked_revision = [0_u8; MESSAGE_OBSERVATION_REVISION_BYTES];
    for (index, byte) in revision_bytes.iter().enumerate() {
        masked_revision[index] = byte ^ mask[index];
    }
    let tag = observation_digest(
        b"webcodex.session-message-observation.tag.v1\0",
        session_id,
        &masked_revision,
    );
    let mut payload = Vec::with_capacity(MESSAGE_OBSERVATION_PAYLOAD_BYTES);
    payload.extend_from_slice(&binding[..MESSAGE_OBSERVATION_BINDING_BYTES]);
    payload.extend_from_slice(&masked_revision);
    payload.extend_from_slice(&tag[..MESSAGE_OBSERVATION_TAG_BYTES]);
    let token = format!(
        "{MESSAGE_OBSERVATION_TOKEN_PREFIX}{}",
        general_purpose::URL_SAFE_NO_PAD.encode(payload)
    );
    if token.len() > MAX_SESSION_MESSAGE_OBSERVATION_TOKEN_LEN {
        return Err(SessionMessageObservationError::InvalidObservationState);
    }
    Ok(token)
}

fn parse_observation_token(
    value: &str,
    session_id: &str,
) -> Result<u64, SessionMessageObservationError> {
    if value.len() > MAX_SESSION_MESSAGE_OBSERVATION_TOKEN_LEN {
        return Err(SessionMessageObservationError::OversizedToken);
    }
    let encoded = value
        .strip_prefix(MESSAGE_OBSERVATION_TOKEN_PREFIX)
        .ok_or(SessionMessageObservationError::MalformedToken)?;
    if encoded.is_empty() || !encoded.is_ascii() {
        return Err(SessionMessageObservationError::MalformedToken);
    }
    let payload = general_purpose::URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map_err(|_| SessionMessageObservationError::MalformedToken)?;
    if payload.len() != MESSAGE_OBSERVATION_PAYLOAD_BYTES {
        return Err(SessionMessageObservationError::MalformedToken);
    }
    let expected_binding = observation_digest(
        b"webcodex.session-message-observation.binding.v1\0",
        session_id,
        &[],
    );
    if payload[..MESSAGE_OBSERVATION_BINDING_BYTES]
        != expected_binding[..MESSAGE_OBSERVATION_BINDING_BYTES]
    {
        return Err(SessionMessageObservationError::WrongSession);
    }
    let masked_start = MESSAGE_OBSERVATION_BINDING_BYTES;
    let masked_end = masked_start + MESSAGE_OBSERVATION_REVISION_BYTES;
    let masked_revision: [u8; MESSAGE_OBSERVATION_REVISION_BYTES] = payload
        [masked_start..masked_end]
        .try_into()
        .map_err(|_| SessionMessageObservationError::MalformedToken)?;
    let expected_tag = observation_digest(
        b"webcodex.session-message-observation.tag.v1\0",
        session_id,
        &masked_revision,
    );
    if payload[masked_end..] != expected_tag[..MESSAGE_OBSERVATION_TAG_BYTES] {
        return Err(SessionMessageObservationError::MalformedToken);
    }
    let mask = observation_digest(
        b"webcodex.session-message-observation.mask.v1\0",
        session_id,
        &[],
    );
    let mut revision_bytes = [0_u8; MESSAGE_OBSERVATION_REVISION_BYTES];
    for (index, byte) in masked_revision.iter().enumerate() {
        revision_bytes[index] = byte ^ mask[index];
    }
    Ok(u64::from_be_bytes(revision_bytes))
}

fn observation_digest(domain: &[u8], session_id: &str, extra: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(session_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(extra);
    hasher.finalize().into()
}

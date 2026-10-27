//! JSON session ledger load/save, sanitize-on-restore, and atomic writes.
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::io;
use std::sync::Arc;

use serde_json::Value;

use super::assignment::is_valid_assignment_fence_fingerprint;
use super::events::{
    context_result_summary_for_tool_result, exploration_tool_kind, is_valid_session_id,
    sanitize_failure_expectation_result, sanitize_observed_paths,
    sanitize_persisted_validation_output_summary, sanitize_persistent_shell_event_evidence,
    sanitize_tool_execution_state, session_input_summary_for_tool,
};
use super::model::{
    ColdSessionRecord, PersistedRetentionTombstoneRow, PersistedSessionRecord, SessionEvent,
    SessionGuards, SessionMessage, SessionRecord, SessionRetentionTombstone, EVENT_ID_PREFIX,
    MAX_CODING_INSTRUCTION_CHARS, MAX_INPUT_ARRAY_ITEMS, MAX_MATERIALIZED_VALIDATION_JOB_IDS,
    MAX_MESSAGE_CHARS, MAX_MESSAGE_RESOLUTION_CHARS, SESSION_LEDGER_VERSION,
};
use super::query::{is_valid_completion_id, validate_message_tags};
use super::util::{
    bound_chars, bound_event_error_summary, bound_summary_string, redact_and_bound_instruction,
};
use webcodex_core::project_instructions::ProjectInstructionsSummarySnapshot;
use webcodex_core::workflow_session_contract::is_safe_job_id;

#[cfg(test)]
use crate::model::StoredSession;

pub(crate) mod database;
mod resident;
mod stream;

fn v2_record_has_canonical_logical_invocation_shape(value: &Value) -> bool {
    value
        .get("events")
        .and_then(Value::as_array)
        .is_some_and(|events| {
            events.iter().all(|event| {
                event.as_object().is_some_and(|object| {
                    match (
                        object.get("logical_invocation_id"),
                        object.get("logical_invocation_role"),
                    ) {
                        (None, None) => true,
                        (Some(Value::String(id)), Some(Value::String(role))) => {
                            super::events::is_valid_logical_invocation_id(id)
                                && super::events::is_valid_logical_invocation_role(role)
                        }
                        _ => false,
                    }
                })
            })
        })
}

impl PersistedSessionRecord {
    pub fn from_record(record: &SessionRecord, recent_event_limit: usize) -> Self {
        Self::snapshot(record, recent_event_limit, None)
    }

    pub(crate) fn database_delta(record: &SessionRecord, after: u64) -> Self {
        Self::snapshot(record, 0, Some(after))
    }

    fn snapshot(record: &SessionRecord, recent_event_limit: usize, after: Option<u64>) -> Self {
        let changed = |id: &String| {
            after.is_none_or(|after| {
                record
                    .message_observation_revisions
                    .get(id)
                    .copied()
                    .unwrap_or(0)
                    > after
            })
        };
        let event_skip = record.events.len().saturating_sub(recent_event_limit);
        let events: Vec<_> = record.events.iter().skip(event_skip).cloned().collect();
        let messages: Vec<_> = record
            .messages
            .iter()
            .filter(|message| changed(&message.message_id))
            .cloned()
            .collect();
        debug_assert!(record
            .events
            .iter()
            .skip(event_skip)
            .zip(&events)
            .all(|(record, snapshot)| Arc::ptr_eq(record, snapshot)));
        debug_assert!(record
            .messages
            .iter()
            .filter(|message| changed(&message.message_id))
            .zip(&messages)
            .all(|(record, snapshot)| Arc::ptr_eq(record, snapshot)));
        Self {
            session_id: record.session_id.clone(),
            project: record.project.clone(),
            owner_authority_fingerprint: record.owner_authority_fingerprint.clone(),
            title: record.title.clone(),
            mode: record.mode,
            guards: record.guards,
            execution_context: record.execution_context.clone(),
            lifecycle: record.lifecycle,
            created_at: record.created_at,
            updated_at: record.updated_at,
            current_instruction: record.current_instruction.clone(),
            event_history_start: if record.history_is_durable {
                record.event_history_start
            } else {
                record
                    .events_observed
                    .saturating_sub(record.events.len() as u64)
            },
            events,
            messages,
            message_delivery_replays: record
                .message_delivery_replays
                .iter()
                .filter(|(_, replay)| changed(&replay.message_id))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            events_observed: record.events_observed,
            legacy_context_revision: None,
            git_baseline_tree: record.git_baseline_tree.clone(),
            repository_edit_observed: record.repository_edit_observed,
            materialized_validation_job_ids: record
                .materialized_validation_job_ids
                .iter()
                .cloned()
                .collect(),
            message_observation_revision: record.message_observation_revision,
            message_observation_floor: record.message_observation_floor,
            message_observation_revisions: record
                .message_observation_revisions
                .iter()
                .filter(|(id, _)| changed(id))
                .map(|(key, value)| (key.clone(), *value))
                .collect(),
            assignment_history_floors: record
                .assignment_history_floors
                .iter()
                .filter(|(id, _)| changed(id))
                .map(|(key, value)| (key.clone(), *value))
                .collect(),
            assignment_history_tracking_complete: record.assignment_history_tracking_complete,
            completion_assignment_fence_fingerprints: record
                .completion_assignment_fence_fingerprints
                .iter()
                .filter(|(id, _)| changed(id))
                .filter_map(|(todo_id, fingerprint)| {
                    fingerprint
                        .as_ref()
                        .map(|fingerprint| (todo_id.clone(), fingerprint.clone()))
                })
                .collect(),
            completion_assignment_fence_tracking_complete: record
                .completion_assignment_fence_tracking_complete,
        }
    }

    pub fn still_matches_record(&self, record: &SessionRecord) -> bool {
        self.session_id == record.session_id
            && self.project == record.project
            && self.owner_authority_fingerprint == record.owner_authority_fingerprint
            && self.title == record.title
            && self.mode == record.mode
            && self.guards == record.guards
            && self.execution_context == record.execution_context
            && self.lifecycle == record.lifecycle
            && self.created_at == record.created_at
            && self.updated_at == record.updated_at
            && self.current_instruction.as_ref().map(|e| &e.event_id)
                == record.current_instruction.as_ref().map(|e| &e.event_id)
            && self.events_observed == record.events_observed
            && self.git_baseline_tree == record.git_baseline_tree
            && self.repository_edit_observed == record.repository_edit_observed
            && self
                .materialized_validation_job_ids
                .iter()
                .eq(record.materialized_validation_job_ids.iter())
            && self.message_observation_revision == record.message_observation_revision
            && self.message_delivery_replays == record.message_delivery_replays
            && self.message_observation_floor == record.message_observation_floor
            && self.message_observation_revisions == record.message_observation_revisions
            && self.assignment_history_floors == record.assignment_history_floors
            && self.assignment_history_tracking_complete
                == record.assignment_history_tracking_complete
            && self.completion_assignment_fence_fingerprints.len()
                == record.completion_assignment_fence_fingerprints.len()
            && self
                .completion_assignment_fence_fingerprints
                .iter()
                .all(|(todo_id, fingerprint)| {
                    record
                        .completion_assignment_fence_fingerprints
                        .get(todo_id)
                        .and_then(Option::as_ref)
                        == Some(fingerprint)
                })
            && self.completion_assignment_fence_tracking_complete
                == record.completion_assignment_fence_tracking_complete
            && self.events.len() == record.events.len()
            && self.messages.len() == record.messages.len()
            && self
                .events
                .iter()
                .zip(&record.events)
                .all(|(snapshot, live)| Arc::ptr_eq(snapshot, live))
            && self
                .messages
                .iter()
                .zip(&record.messages)
                .all(|(snapshot, live)| Arc::ptr_eq(snapshot, live))
    }

    fn into_record(self, recent_event_limit: usize) -> Option<SessionRecord> {
        let session_id = self.session_id.trim().to_string();
        if !is_valid_session_id(&session_id) {
            return None;
        }
        // Canonical authority and lifecycle are required before any restored row
        // can enter the in-memory Session store.
        let owner_authority_fingerprint =
            sanitize_owner_authority_fingerprint(self.owner_authority_fingerprint)?;
        let lifecycle = self.lifecycle;
        let events: VecDeque<Arc<SessionEvent>> = self
            .events
            .into_iter()
            .map(|event| Arc::try_unwrap(event).unwrap_or_else(|event| (*event).clone()))
            .filter_map(|event| sanitize_persisted_event(event, &session_id))
            .map(Arc::new)
            .rev()
            .take(recent_event_limit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let persisted_message_count = self.messages.len();
        let mut messages: VecDeque<Arc<SessionMessage>> = self
            .messages
            .into_iter()
            .map(|message| Arc::try_unwrap(message).unwrap_or_else(|message| (*message).clone()))
            .filter_map(|message| sanitize_persisted_message(message, &session_id))
            .map(Arc::new)
            .rev()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        // Canonical writers mint one unique message_id per retained message. A
        // duplicate persisted id is structurally ambiguous for message mutation
        // and for revision-only pagination. Do not guess which conflicting body
        // is authoritative: discard every retained message carrying that id.
        let mut seen_message_ids = HashSet::new();
        let mut duplicate_message_ids = HashSet::new();
        for message in &messages {
            if !seen_message_ids.insert(message.message_id.clone()) {
                duplicate_message_ids.insert(message.message_id.clone());
            }
        }
        let duplicate_retained_message_ids = !duplicate_message_ids.is_empty();
        let all_persisted_messages_restored =
            !duplicate_retained_message_ids && messages.len() == persisted_message_count;
        if duplicate_retained_message_ids {
            messages.retain(|message| !duplicate_message_ids.contains(&message.message_id));
        }
        let retained_message_ids = messages
            .iter()
            .map(|message| message.message_id.clone())
            .collect::<HashSet<_>>();
        let message_delivery_replays = self
            .message_delivery_replays
            .into_iter()
            .filter(|(scope_key, replay)| {
                is_lower_hex_sha256(scope_key.split_once(':').map_or("", |(scope, _)| scope))
                    && is_lower_hex_sha256(scope_key.split_once(':').map_or("", |(_, key)| key))
                    && is_lower_hex_sha256(&replay.payload_fingerprint)
                    && retained_message_ids.contains(&replay.message_id)
            })
            .collect();
        let current_observation_revision = self.message_observation_revision;
        let mut observation_floor = self
            .message_observation_floor
            .min(current_observation_revision);
        let mut observation_revisions = BTreeMap::new();
        let mut assignment_history_floors = BTreeMap::new();
        let assignment_history_tracking_complete;
        if current_observation_revision > 0 {
            let mut inconsistent = duplicate_retained_message_ids;
            let mut observation_metadata_inconsistent = false;
            let mut retained_positive_revisions = HashSet::new();
            for (message_id, revision) in self.message_observation_revisions {
                if revision > current_observation_revision {
                    inconsistent = true;
                    continue;
                }
                if retained_message_ids.contains(&message_id) {
                    // Revision zero is a valid shared baseline for messages restored
                    // from a pre-observation ledger. Positive revisions, however,
                    // are assigned one-at-a-time by the canonical writer and must
                    // remain unique or a revision-only pagination token could skip
                    // a second retained message with the same revision.
                    if revision > 0 && !retained_positive_revisions.insert(revision) {
                        inconsistent = true;
                    }
                    observation_revisions.insert(message_id, revision);
                } else {
                    observation_floor = observation_floor.max(revision);
                }
            }
            for message_id in &retained_message_ids {
                if !observation_revisions.contains_key(message_id) {
                    inconsistent = true;
                    observation_revisions.insert(message_id.clone(), 0);
                }
            }
            let highest_known_revision = observation_revisions
                .values()
                .copied()
                .max()
                .unwrap_or(0)
                .max(observation_floor);
            if highest_known_revision != current_observation_revision {
                // The canonical current revision is always represented either
                // by the latest state of a retained message or by history already
                // represented by the floor. If the persisted high-water mark has
                // no such explanation, cursor continuity cannot be proven.
                inconsistent = true;
            }
            if inconsistent {
                observation_metadata_inconsistent = true;
                // Observation metadata is advisory bookkeeping around durable
                // messages. If it cannot prove the writer's unique ordering,
                // preserve the messages but fail closed on historical continuity:
                // old cursors report history_lost and no retained pre-restore
                // state is paginated under an ambiguous revision.
                observation_floor = current_observation_revision;
                observation_revisions.clear();
                observation_revisions.extend(
                    retained_message_ids
                        .iter()
                        .cloned()
                        .map(|message_id| (message_id, 0)),
                );
            }
            let retained_todo_ids = messages
                .iter()
                .filter(|message| message.kind == super::model::SessionMessageKind::Todo)
                .map(|message| message.message_id.clone())
                .collect::<HashSet<_>>();
            let mut assignment_metadata_inconsistent = observation_metadata_inconsistent;
            for (todo_id, revision) in self.assignment_history_floors {
                if revision == 0
                    || revision > current_observation_revision
                    || !retained_todo_ids.contains(&todo_id)
                {
                    assignment_metadata_inconsistent = true;
                    continue;
                }
                assignment_history_floors.insert(todo_id, revision);
            }
            assignment_history_tracking_complete = all_persisted_messages_restored
                && !assignment_metadata_inconsistent
                && self.assignment_history_tracking_complete;
        } else {
            observation_revisions.extend(
                retained_message_ids
                    .iter()
                    .cloned()
                    .map(|message_id| (message_id, 0)),
            );
            observation_floor = 0;
            assignment_history_tracking_complete =
                all_persisted_messages_restored && self.assignment_history_tracking_complete;
        }
        let retained_completed_todo_ids = messages
            .iter()
            .filter(|message| {
                message.kind == super::model::SessionMessageKind::Todo
                    && message.status == super::model::SessionMessageStatus::Resolved
                    && message.completion_id.is_some()
                    && message.resolved_by_message_id.is_some()
            })
            .map(|message| message.message_id.clone())
            .collect::<HashSet<_>>();
        let mut completion_assignment_fence_fingerprints = BTreeMap::new();
        let mut completion_fence_metadata_inconsistent = false;
        for (todo_id, fingerprint) in self.completion_assignment_fence_fingerprints {
            if !retained_completed_todo_ids.contains(&todo_id)
                || !is_valid_assignment_fence_fingerprint(&fingerprint)
            {
                completion_fence_metadata_inconsistent = true;
                continue;
            }
            completion_assignment_fence_fingerprints.insert(todo_id, Some(fingerprint));
        }
        let persisted_completion_tracking_complete =
            self.completion_assignment_fence_tracking_complete;
        if persisted_completion_tracking_complete
            && retained_completed_todo_ids
                .iter()
                .any(|todo_id| !completion_assignment_fence_fingerprints.contains_key(todo_id))
        {
            completion_fence_metadata_inconsistent = true;
        }
        let completion_assignment_fence_tracking_complete = all_persisted_messages_restored
            && persisted_completion_tracking_complete
            && !completion_fence_metadata_inconsistent;
        let materialized_validation_job_ids =
            sanitize_materialized_validation_job_ids(self.materialized_validation_job_ids);
        // On restore, `events_observed` is at least the count of events we just
        // retained. A live ledger that exceeded the cap has the true cumulative
        // count persisted.
        let retained_events = events.len() as u64;
        let project = self.project.map(|value| bound_summary_string(value.trim()));
        let execution_context = if project.is_some() {
            self.execution_context.sanitized_for_restore()
        } else {
            Default::default()
        };
        Some(SessionRecord {
            current_instruction: self
                .current_instruction
                .and_then(|event| sanitize_persisted_event((*event).clone(), &session_id))
                .filter(|event| event.kind == "task_instruction")
                .map(Arc::new)
                .or_else(|| {
                    events
                        .iter()
                        .rev()
                        .find(|event| event.kind == "task_instruction")
                        .cloned()
                }),
            history_is_durable: false,
            event_history_start: self.event_history_start,
            pending_events: VecDeque::new(),
            durable: None,
            session_id,
            project,
            owner_authority_fingerprint,
            title: self.title.map(|value| bound_summary_string(value.trim())),
            mode: self.mode,
            guards: SessionGuards::effective(self.mode, self.guards),
            execution_context,
            lifecycle,
            created_at: self.created_at,
            updated_at: self.updated_at.max(self.created_at),
            events,
            events_observed: self.events_observed.max(retained_events),
            git_baseline_tree: self.git_baseline_tree.filter(|tree| {
                matches!(tree.len(), 40 | 64) && tree.bytes().all(|byte| byte.is_ascii_hexdigit())
            }),
            repository_edit_observed: self.repository_edit_observed,
            materialized_validation_job_ids,
            messages,
            message_delivery_replays,
            project_instructions: None,
            message_observation_revision: current_observation_revision,
            message_observation_floor: observation_floor,
            message_observation_revisions: observation_revisions,
            assignment_history_floors,
            assignment_history_tracking_complete,
            completion_assignment_fence_fingerprints,
            completion_assignment_fence_tracking_complete,
        })
    }
}

fn sanitize_materialized_validation_job_ids(values: Vec<String>) -> VecDeque<String> {
    // Canonical writers keep this exact set bounded to the authoritative Runner
    // terminal inventory. Restore is deliberately tolerant of legacy/corrupt
    // semantic entries: malformed or duplicate ids never become suppression
    // authority, and an oversized list keeps only the newest valid identities.
    let mut seen = HashSet::new();
    let mut newest = values
        .into_iter()
        .rev()
        .filter_map(|value| {
            let trimmed = value.trim();
            if trimmed != value || !is_safe_job_id(trimmed) || !seen.insert(value.clone()) {
                return None;
            }
            Some(value)
        })
        .take(MAX_MATERIALIZED_VALIDATION_JOB_IDS)
        .collect::<Vec<_>>();
    newest.reverse();
    newest.into()
}

fn sanitize_owner_authority_fingerprint(value: String) -> Option<String> {
    is_lower_hex_sha256(&value).then_some(value)
}

pub fn cold_session_from_record(
    record: &SessionRecord,
    recent_event_limit: usize,
) -> Result<ColdSessionRecord, serde_json::Error> {
    let project_instructions = record
        .project_instructions
        .as_ref()
        .map(|snapshot| snapshot.to_summary());
    let persisted = PersistedSessionRecord::from_record(record, recent_event_limit);
    let mut cold = cold_session_from_persisted(&persisted, project_instructions)?;
    if record.lifecycle.allows_mutation() {
        cold.resident_instructions = record
            .project_instructions
            .as_ref()
            .map(|value| Arc::new(value.clone()));
        cold.unfenced_completion_ids = record
            .completion_assignment_fence_fingerprints
            .iter()
            .filter(|(_, fence)| fence.is_none())
            .map(|(id, _)| id.clone())
            .collect();
    }
    Ok(cold)
}

pub fn cold_session_from_persisted(
    persisted: &PersistedSessionRecord,
    project_instructions: Option<ProjectInstructionsSummarySnapshot>,
) -> Result<ColdSessionRecord, serde_json::Error> {
    let owner_authority_fingerprint =
        sanitize_owner_authority_fingerprint(persisted.owner_authority_fingerprint.clone())
            .ok_or_else(|| {
                serde_json::Error::io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "persisted Session has invalid canonical authority",
                ))
            })?;
    let lifecycle = persisted.lifecycle;
    let raw = Arc::from(serde_json::value::to_raw_value(persisted)?);
    let revision_raw = Arc::downgrade(&raw);
    Ok(ColdSessionRecord {
        database: None,
        database_revision: 0,
        session_id: persisted.session_id.clone(),
        project: persisted.project.clone(),
        owner_authority_fingerprint,
        mode: persisted.mode,
        guards: persisted.guards,
        lifecycle,
        updated_at: persisted.updated_at,
        created_at: persisted.created_at,
        project_instructions,
        raw,
        console_list_cache: Arc::new(std::sync::Mutex::new(None)),
        resident_instructions: None,
        unfenced_completion_ids: Vec::new(),
        revision_raw,
        retained_events_observed: persisted.events_observed.max(persisted.events.len() as u64),
        retained_message_revision: persisted.message_observation_revision,
    })
}

pub fn materialize_cold_session(
    record: &ColdSessionRecord,
    recent_event_limit: usize,
) -> Option<SessionRecord> {
    let persisted = if let Some(database) = &record.database {
        database.load(record, recent_event_limit).ok()?
    } else {
        serde_json::from_str::<PersistedSessionRecord>(record.raw.get()).ok()?
    };
    if persisted.session_id != record.session_id
        || persisted.project != record.project
        || persisted.owner_authority_fingerprint != record.owner_authority_fingerprint
        || persisted.lifecycle != record.lifecycle
        || persisted.mode != record.mode
        || persisted.guards != record.guards
        || persisted.updated_at != record.updated_at
    {
        return None;
    }
    // SQLite validates persisted input on load. Memory-only Cold bytes were
    // produced by our own canonical serializer. Both restore process-local
    // instruction/fence metadata separately without changing identity.
    Some(resident::materialize(persisted, record))
}

fn parse_retention_tombstone_row(value: &Value) -> Option<SessionRetentionTombstone> {
    let row = serde_json::from_value::<PersistedRetentionTombstoneRow>(value.clone()).ok()?;
    let tombstone = row.retention_tombstone;
    if !is_valid_session_id(&tombstone.session_id)
        || !is_lower_hex_sha256(&tombstone.owner_authority_fingerprint)
        || !is_lower_hex_sha256(&tombstone.incarnation_fingerprint)
    {
        return None;
    }
    Some(tombstone)
}

pub(super) fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

pub fn sanitize_persisted_event(mut event: SessionEvent, session_id: &str) -> Option<SessionEvent> {
    if event.session_id != session_id || !event.event_id.starts_with(EVENT_ID_PREFIX) {
        return None;
    }
    event.kind = bound_summary_string(event.kind.trim());
    let logical_id = event
        .logical_invocation_id
        .take()
        .filter(|value| super::events::is_valid_logical_invocation_id(value));
    let logical_role = event
        .logical_invocation_role
        .take()
        .filter(|value| super::events::is_valid_logical_invocation_role(value));
    if logical_id.is_some() && logical_role.is_some() {
        event.logical_invocation_id = logical_id;
        event.logical_invocation_role = logical_role;
    }
    event.transport = bound_summary_string(event.transport.trim());
    event.tool_name = bound_summary_string(event.tool_name.trim());
    event.project = event
        .project
        .map(|value| bound_summary_string(value.trim()));
    event.resolved_project = event
        .resolved_project
        .map(|value| bound_summary_string(value.trim()));
    event.risk_class = bound_summary_string(event.risk_class.trim());
    event.status = event.status.map(|value| bound_summary_string(value.trim()));
    event.failure_kind = event
        .failure_kind
        .map(|value| bound_summary_string(value.trim()));
    event.error_kind = event
        .error_kind
        .map(|value| bound_summary_string(value.trim()));
    event.expected_failure_kind = event
        .expected_failure_kind
        .map(|value| bound_summary_string(value.trim()))
        .filter(|value| !value.is_empty());
    event.result_expectation = event
        .result_expectation
        .filter(|value| matches!(value.as_str(), "failure" | "observe"));
    event.accepted_exit_codes.sort_unstable();
    event.accepted_exit_codes.dedup();
    event.accepted_exit_codes.truncate(32);
    event.assertion_name = event
        .assertion_name
        .map(|value| bound_summary_string(value.trim()))
        .filter(|value| !value.is_empty());
    event.actual_failure_kind = event
        .actual_failure_kind
        .map(|value| bound_summary_string(value.trim()))
        .filter(|value| !value.is_empty());
    event.failure_expectation_result = event
        .failure_expectation_result
        .map(|value| sanitize_failure_expectation_result(value.trim()));
    event.warning_kind = event
        .warning_kind
        .map(|value| bound_summary_string(value.trim()));
    event.session_project = event
        .session_project
        .map(|value| bound_summary_string(value.trim()));
    event.request_project = event
        .request_project
        .map(|value| bound_summary_string(value.trim()));
    event.error_message_summary = event
        .error_message_summary
        .map(|value| bound_event_error_summary(value.trim(), event.shell_like));
    event.changed_paths = event
        .changed_paths
        .into_iter()
        .take(MAX_INPUT_ARRAY_ITEMS)
        .map(|path| bound_summary_string(path.trim()))
        .filter(|path| !path.is_empty())
        .collect();
    event.observed_paths = if exploration_tool_kind(&event.tool_name).is_some()
        && (event.kind == "tool_call_started"
            || (event.kind == "tool_call_finished" && event.status.as_deref() == Some("succeeded")))
    {
        sanitize_observed_paths(event.observed_paths)
    } else {
        Vec::new()
    };
    event.job_id = event.job_id.map(|value| bound_summary_string(value.trim()));
    event.persistent_shell = event
        .persistent_shell
        .and_then(|evidence| sanitize_persistent_shell_event_evidence(&event.tool_name, evidence));
    event.effect_evidence = event.effect_evidence.map(|mut evidence| {
        evidence.execution_state = evidence
            .execution_state
            .as_deref()
            .and_then(sanitize_tool_execution_state);
        evidence
    });
    event.instruction = event
        .instruction
        .map(|value| redact_and_bound_instruction(value.trim(), MAX_CODING_INSTRUCTION_CHARS))
        .filter(|value| !value.is_empty());
    event.requested_mode = event
        .requested_mode
        .map(|value| bound_summary_string(value.trim()))
        .filter(|value| !value.is_empty());
    event.previous_mode = event
        .previous_mode
        .map(|value| bound_summary_string(value.trim()))
        .filter(|value| !value.is_empty());
    event.input_summary = event
        .input_summary
        .map(|value| session_input_summary_for_tool(&event.tool_name, &value));
    event.context_result_summary = event
        .context_result_summary
        .and_then(|value| context_result_summary_for_tool_result(&event.tool_name, &value));
    event.validation_output_summary = event
        .validation_output_summary
        .and_then(|value| sanitize_persisted_validation_output_summary(&event.tool_name, &value));
    event.execution_context = event
        .execution_context
        .map(|context| context.sanitized_for_restore());
    event.previous_execution_context = event
        .previous_execution_context
        .map(|context| context.sanitized_for_restore());
    Some(event)
}

fn is_valid_persisted_message_id(message_id: &str) -> bool {
    webcodex_core::workflow_session_contract::is_valid_session_message_id(message_id)
}

pub fn sanitize_persisted_message(
    mut message: SessionMessage,
    session_id: &str,
) -> Option<SessionMessage> {
    if message.session_id != session_id || !is_valid_persisted_message_id(&message.message_id) {
        return None;
    }
    message.message = bound_chars(message.message.trim(), MAX_MESSAGE_CHARS);
    message.tags = validate_message_tags(message.tags).unwrap_or_default();
    message.first_ack_observed_at = message
        .first_ack_observed_at
        .filter(|value| *value > 0 && message.requires_ack);
    message.reply_to = message.reply_to.and_then(|reply_to| {
        let reply_to = reply_to.trim().to_string();
        is_valid_persisted_message_id(&reply_to).then_some(reply_to)
    });
    message.author_session_id = message.author_session_id.and_then(|author_session_id| {
        let author_session_id = author_session_id.trim().to_string();
        is_valid_session_id(&author_session_id).then_some(author_session_id)
    });
    message.resolved_by_message_id = message.resolved_by_message_id.and_then(|message_id| {
        let message_id = message_id.trim().to_string();
        is_valid_persisted_message_id(&message_id).then_some(message_id)
    });
    message.superseded_by_message_id = message.superseded_by_message_id.and_then(|message_id| {
        let message_id = message_id.trim().to_string();
        is_valid_persisted_message_id(&message_id).then_some(message_id)
    });
    message.supersedes_message_id = message.supersedes_message_id.and_then(|message_id| {
        let message_id = message_id.trim().to_string();
        is_valid_persisted_message_id(&message_id).then_some(message_id)
    });
    if message.status != super::model::SessionMessageStatus::Resolved {
        message.closure_kind = None;
        message.superseded_by_message_id = None;
    } else {
        match message.closure_kind {
            Some(super::model::SessionMessageClosureKind::Withdrawn) => {
                message.superseded_by_message_id = None;
            }
            Some(super::model::SessionMessageClosureKind::Superseded) => {
                // A missing/malformed link is retained as a fail-closed historical
                // supersede marker, but can never authorize replacement replay.
            }
            None => {
                // A link without its machine-readable closure kind is not replay
                // authority. Cross-link targets themselves may legitimately have
                // been evicted, so no unbounded repair is attempted here.
                message.superseded_by_message_id = None;
            }
        }
    }
    message.completion_id = message.completion_id.map(|completion_id| {
        let completion_id = completion_id.trim().to_ascii_lowercase();
        if is_valid_completion_id(&completion_id) {
            completion_id
        } else {
            "invalid".to_string()
        }
    });
    message.resolution = message
        .resolution
        .map(|resolution| bound_chars(resolution.trim(), MAX_MESSAGE_RESOLUTION_CHARS));
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed_cold_ledger_fixture() -> (ColdSessionRecord, usize) {
        use crate::model::SessionTransport;
        use crate::store::SessionStore;
        use crate::SessionToolContract;
        let max_events = 2_000;
        let store = SessionStore::new(4, max_events);
        let id = store
            .start_session(Some("project".into()), Some("history".into()))
            .session_id;
        let contract = SessionToolContract {
            risk_class: "read_only",
            read_like: true,
            write_like: false,
            shell_like: false,
            git_like: false,
            change_summary_like: false,
            project_write: false,
            path_hint: crate::events::SessionPathHint::SinglePath,
        };
        for index in 0..800 {
            let start = store.record_tool_call_started(
                Some(&id),
                SessionTransport::Api,
                "read_files",
                &serde_json::json!({"project":"project", "items":[{"path":format!("file_{index}.rs")}]}),
                contract,
            );
            store.record_tool_call_finished(
                start,
                true,
                &serde_json::json!({"ok":true}),
                None,
                None,
            );
        }
        store.close_session(&id).unwrap();
        let cold = store
            .with_record_for_query(&id, |record, _| {
                cold_session_from_record(record, max_events).unwrap()
            })
            .unwrap();
        (cold, max_events)
    }

    #[test]
    fn closed_cold_canonical_read_matches_original_sanitizing_materialization() {
        let (cold, limit) = closed_cold_ledger_fixture();
        let old = serde_json::from_str::<PersistedSessionRecord>(cold.raw.get())
            .unwrap()
            .into_record(limit)
            .unwrap();
        let new = materialize_cold_session(&cold, limit).unwrap();
        assert_eq!(
            serde_json::to_value(PersistedSessionRecord::from_record(&old, limit)).unwrap(),
            serde_json::to_value(PersistedSessionRecord::from_record(&new, limit)).unwrap()
        );
    }

    #[test]
    #[ignore = "manual hot/cold materialization comparison, synthetic 1,600-event closed Session"]
    fn closed_cold_canonical_read_cost_comparison() {
        use std::time::Instant;
        let (cold, limit) = closed_cold_ledger_fixture();
        let rounds = 25_u128;
        let mut legacy_ns = 0_u128;
        let mut canonical_ns = 0_u128;
        for iteration in 0..rounds {
            if iteration % 2 == 0 {
                let begin = Instant::now();
                let record = serde_json::from_str::<PersistedSessionRecord>(cold.raw.get())
                    .unwrap()
                    .into_record(limit)
                    .unwrap();
                std::hint::black_box(record);
                legacy_ns += begin.elapsed().as_nanos();
            }
            let begin = Instant::now();
            std::hint::black_box(materialize_cold_session(&cold, limit).unwrap());
            canonical_ns += begin.elapsed().as_nanos();
            if iteration % 2 != 0 {
                let begin = Instant::now();
                let record = serde_json::from_str::<PersistedSessionRecord>(cold.raw.get())
                    .unwrap()
                    .into_record(limit)
                    .unwrap();
                std::hint::black_box(record);
                legacy_ns += begin.elapsed().as_nanos();
            }
        }
        println!(
            "closed_cold_materialization events=1600 legacy_avg_ms={:.3} canonical_avg_ms={:.3} speedup={:.2}x",
            legacy_ns as f64 / rounds as f64 / 1e6,
            canonical_ns as f64 / rounds as f64 / 1e6,
            legacy_ns as f64 / canonical_ns as f64
        );
    }

    #[test]
    fn tombstone_row_is_not_a_session_record() {
        let row = serde_json::json!({
            "retention_tombstone": {
                "session_id": "wc_sess_0123456789abcdef0123456789abcdef",
                "owner_authority_fingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "incarnation_fingerprint": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "expiry_ordinal": 1
            }
        });
        assert!(row.get("events").is_none());
        assert!(!v2_record_has_canonical_logical_invocation_shape(&row));
        assert!(serde_json::from_value::<PersistedSessionRecord>(row).is_err());
    }
}

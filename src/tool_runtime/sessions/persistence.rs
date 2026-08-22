//! JSON session ledger load/save, sanitize-on-restore, and atomic writes.
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;

use super::super::project_instructions::ProjectInstructionsSummarySnapshot;
use super::events::{
    exploration_tool_kind, is_valid_session_id, sanitize_failure_expectation_result,
    sanitize_observed_paths, sanitize_persisted_validation_output_summary,
    sanitize_persistent_shell_event_evidence, sanitize_tool_execution_state,
    session_input_summary_for_tool,
};
use super::model::{
    ColdSessionRecord, DurableCurrentBinding, PersistedCurrentBindings, PersistedSessionLedger,
    PersistedSessionRecord, SessionEvent, SessionGuards, SessionLifecycle, SessionMessage,
    SessionRecord, StoredSession, DEFAULT_MAX_MESSAGES_PER_SESSION, EVENT_ID_PREFIX,
    MAX_CODING_INSTRUCTION_CHARS, MAX_INPUT_ARRAY_ITEMS, MAX_MESSAGE_CHARS,
    MAX_MESSAGE_RESOLUTION_CHARS, MESSAGE_ID_PREFIX, SESSION_LEDGER_VERSION,
};
use super::query::{is_valid_completion_id, validate_message_tags};
use super::util::{
    bound_chars, bound_event_error_summary, bound_summary_string, redact_and_bound_instruction,
};

#[derive(Deserialize)]
struct LoadedSessionLedger {
    version: u32,
    sessions: Vec<PersistedSessionRecord>,
    #[serde(default)]
    durable_current_bindings: PersistedCurrentBindings,
}

impl PersistedSessionRecord {
    pub(super) fn from_record(record: &SessionRecord, max_events_per_session: usize) -> Self {
        let event_skip = record.events.len().saturating_sub(max_events_per_session);
        let message_skip = record
            .messages
            .len()
            .saturating_sub(DEFAULT_MAX_MESSAGES_PER_SESSION);
        let events: Vec<_> = record.events.iter().skip(event_skip).cloned().collect();
        let messages: Vec<_> = record.messages.iter().skip(message_skip).cloned().collect();
        debug_assert!(record
            .events
            .iter()
            .skip(event_skip)
            .zip(&events)
            .all(|(record, snapshot)| Arc::ptr_eq(record, snapshot)));
        debug_assert!(record
            .messages
            .iter()
            .skip(message_skip)
            .zip(&messages)
            .all(|(record, snapshot)| Arc::ptr_eq(record, snapshot)));
        Self {
            session_id: record.session_id.clone(),
            project: record.project.clone(),
            title: record.title.clone(),
            mode: record.mode,
            guards: record.guards,
            execution_context: record.execution_context.clone(),
            lifecycle: record.lifecycle,
            created_at: record.created_at,
            updated_at: record.updated_at,
            events,
            messages,
            events_observed: record.events_observed,
        }
    }

    pub(super) fn still_matches_record(&self, record: &SessionRecord) -> bool {
        self.session_id == record.session_id
            && self.project == record.project
            && self.title == record.title
            && self.mode == record.mode
            && self.guards == record.guards
            && self.execution_context == record.execution_context
            && self.lifecycle == record.lifecycle
            && self.created_at == record.created_at
            && self.updated_at == record.updated_at
            && self.events_observed == record.events_observed
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

    pub(super) fn into_record(self, max_events_per_session: usize) -> Option<SessionRecord> {
        let session_id = self.session_id.trim().to_string();
        if !is_valid_session_id(&session_id) {
            return None;
        }
        let events: VecDeque<Arc<SessionEvent>> = self
            .events
            .into_iter()
            .map(|event| Arc::try_unwrap(event).unwrap_or_else(|event| (*event).clone()))
            .filter_map(|event| sanitize_persisted_event(event, &session_id))
            .map(Arc::new)
            .rev()
            .take(max_events_per_session)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let messages: VecDeque<Arc<SessionMessage>> = self
            .messages
            .into_iter()
            .map(|message| Arc::try_unwrap(message).unwrap_or_else(|message| (*message).clone()))
            .filter_map(|message| sanitize_persisted_message(message, &session_id))
            .map(Arc::new)
            .rev()
            .take(DEFAULT_MAX_MESSAGES_PER_SESSION)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        // On restore, `events_observed` is at least the count of events we just
        // retained, so a freshly-restored legacy ledger does not falsely report
        // eviction. A live ledger that exceeded the cap has the true cumulative
        // count persisted.
        let retained_events = events.len() as u64;
        let project = self.project.map(|value| bound_summary_string(value.trim()));
        let execution_context = if project.is_some() {
            self.execution_context.sanitized_for_restore()
        } else {
            Default::default()
        };
        Some(SessionRecord {
            session_id,
            project,
            title: self.title.map(|value| bound_summary_string(value.trim())),
            mode: self.mode,
            guards: SessionGuards::effective(self.mode, self.guards),
            execution_context,
            // Missing ledger field deserializes via #[serde(default)] → Active.
            lifecycle: self.lifecycle,
            created_at: self.created_at,
            updated_at: self.updated_at.max(self.created_at),
            events,
            events_observed: self.events_observed.max(retained_events),
            messages,
            project_instructions: None,
        })
    }
}

pub(super) fn cold_session_from_record(
    record: &SessionRecord,
    max_events_per_session: usize,
) -> Result<ColdSessionRecord, serde_json::Error> {
    debug_assert!(!record.lifecycle.allows_mutation());
    let project_instructions = record
        .project_instructions
        .as_ref()
        .map(|snapshot| snapshot.to_summary());
    let persisted = PersistedSessionRecord::from_record(record, max_events_per_session);
    cold_session_from_persisted(&persisted, project_instructions)
}

pub(super) fn cold_session_from_persisted(
    persisted: &PersistedSessionRecord,
    project_instructions: Option<ProjectInstructionsSummarySnapshot>,
) -> Result<ColdSessionRecord, serde_json::Error> {
    let raw = Arc::from(serde_json::value::to_raw_value(persisted)?);
    Ok(ColdSessionRecord {
        session_id: persisted.session_id.clone(),
        project: persisted.project.clone(),
        mode: persisted.mode,
        guards: persisted.guards,
        lifecycle: persisted.lifecycle,
        updated_at: persisted.updated_at,
        project_instructions,
        raw,
    })
}

pub(super) fn materialize_cold_session(
    record: &ColdSessionRecord,
    max_events_per_session: usize,
) -> Option<SessionRecord> {
    serde_json::from_str::<PersistedSessionRecord>(record.raw.get())
        .ok()?
        .into_record(max_events_per_session)
}

pub(super) struct RestoredSessionLedger {
    pub(super) sessions: HashMap<String, StoredSession>,
    pub(super) durable_current_bindings: HashMap<String, DurableCurrentBinding>,
    pub(super) lru: VecDeque<String>,
    pub(super) restored_sessions: usize,
    pub(super) restored_binding_count: usize,
    pub(super) discarded_binding_count: usize,
    pub(super) last_persist_error: Option<String>,
}

impl RestoredSessionLedger {
    fn empty(last_persist_error: Option<String>) -> Self {
        Self {
            sessions: HashMap::new(),
            durable_current_bindings: HashMap::new(),
            lru: VecDeque::new(),
            restored_sessions: 0,
            restored_binding_count: 0,
            discarded_binding_count: 0,
            last_persist_error,
        }
    }
}

pub(super) fn load_persisted_ledger(
    path: &PathBuf,
    max_sessions: usize,
    max_events_per_session: usize,
    max_durable_bindings: usize,
) -> RestoredSessionLedger {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return RestoredSessionLedger::empty(None);
        }
        Err(err) => {
            let error = bound_summary_string(&format!("restore_failed: {}: {err}", path.display()));
            tracing::warn!("session ledger restore failed: {}", error);
            return RestoredSessionLedger::empty(Some(error));
        }
    };
    let ledger = match serde_json::from_str::<LoadedSessionLedger>(&content) {
        Ok(ledger) => ledger,
        Err(err) => {
            let error = bound_summary_string(&format!(
                "restore_failed: invalid session ledger JSON: {err}"
            ));
            tracing::warn!("session ledger restore failed: {}", error);
            return RestoredSessionLedger::empty(Some(error));
        }
    };
    if ledger.version != SESSION_LEDGER_VERSION {
        let error = format!(
            "restore_failed: unsupported session ledger version {}",
            ledger.version
        );
        tracing::warn!("session ledger restore failed: {}", error);
        return RestoredSessionLedger::empty(Some(error));
    }
    let mut discarded_binding_count = ledger.durable_current_bindings.malformed_count;
    let mut records: Vec<StoredSession> = ledger
        .sessions
        .into_iter()
        .filter_map(|record| record.into_record(max_events_per_session))
        .map(|record| {
            if record.lifecycle.allows_mutation() {
                StoredSession::Hot(record)
            } else {
                match cold_session_from_record(&record, max_events_per_session) {
                    Ok(cold) => StoredSession::Cold(cold),
                    Err(err) => {
                        tracing::warn!("session cold restore serialization failed: {err}");
                        StoredSession::Hot(record)
                    }
                }
            }
        })
        .collect();
    records.sort_by_key(StoredSession::updated_at);
    while records.len() > max_sessions {
        records.remove(0);
    }
    let mut sessions = HashMap::new();
    let mut lru = VecDeque::new();
    for record in records {
        let session_id = record.session_id().to_string();
        lru.push_back(session_id.clone());
        sessions.insert(session_id, record);
    }
    let restored_sessions = sessions.len();

    let mut durable_current_bindings = HashMap::<String, DurableCurrentBinding>::new();
    for binding in ledger.durable_current_bindings.records {
        let session_id = binding.session_id.trim();
        let Some(session) = sessions.get(session_id) else {
            discarded_binding_count = discarded_binding_count.saturating_add(1);
            continue;
        };
        if !is_lower_hex_sha256(&binding.binding_key_sha256)
            || !is_valid_session_id(session_id)
            || binding.updated_at < 0
            || session.lifecycle() != SessionLifecycle::Active
        {
            discarded_binding_count = discarded_binding_count.saturating_add(1);
            continue;
        }

        let candidate = DurableCurrentBinding {
            session_id: session_id.to_string(),
            updated_at: binding.updated_at,
        };
        match durable_current_bindings.entry(binding.binding_key_sha256) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(candidate);
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                discarded_binding_count = discarded_binding_count.saturating_add(1);
                if candidate.updated_at >= entry.get().updated_at {
                    entry.insert(candidate);
                }
            }
        }
    }

    if durable_current_bindings.len() > max_durable_bindings {
        let remove_count = durable_current_bindings.len() - max_durable_bindings;
        let mut oldest: Vec<(String, i64)> = durable_current_bindings
            .iter()
            .map(|(key, binding)| (key.clone(), binding.updated_at))
            .collect();
        oldest.sort_by(|(left_key, left_updated), (right_key, right_updated)| {
            left_updated
                .cmp(right_updated)
                .then_with(|| left_key.cmp(right_key))
        });
        for (binding_key, _) in oldest.into_iter().take(remove_count) {
            durable_current_bindings.remove(&binding_key);
            discarded_binding_count = discarded_binding_count.saturating_add(1);
        }
    }
    let restored_binding_count = durable_current_bindings.len();

    RestoredSessionLedger {
        sessions,
        durable_current_bindings,
        lru,
        restored_sessions,
        restored_binding_count,
        discarded_binding_count,
        last_persist_error: None,
    }
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

pub(super) fn write_ledger_atomic(
    path: &PathBuf,
    ledger: &PersistedSessionLedger,
) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("sessions.json");
    let tmp_path = path.with_file_name(format!(
        ".{file_name}.tmp-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let result = (|| {
        let file = fs::File::create(&tmp_path)?;
        let mut writer = io::BufWriter::new(file);
        serde_json::to_writer(&mut writer, ledger).map_err(io::Error::other)?;
        writer.flush()?;
        drop(writer);
        fs::rename(&tmp_path, path)
    })();
    if let Err(err) = result {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

pub(super) fn sanitize_persisted_event(
    mut event: SessionEvent,
    session_id: &str,
) -> Option<SessionEvent> {
    if event.session_id != session_id || !event.event_id.starts_with(EVENT_ID_PREFIX) {
        return None;
    }
    event.kind = bound_summary_string(event.kind.trim());
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

pub(super) fn sanitize_persisted_message(
    mut message: SessionMessage,
    session_id: &str,
) -> Option<SessionMessage> {
    if message.session_id != session_id || !message.message_id.starts_with(MESSAGE_ID_PREFIX) {
        return None;
    }
    message.message = bound_chars(message.message.trim(), MAX_MESSAGE_CHARS);
    message.tags = validate_message_tags(message.tags).unwrap_or_default();
    message.reply_to = message.reply_to.and_then(|reply_to| {
        let reply_to = reply_to.trim().to_string();
        if reply_to.starts_with(MESSAGE_ID_PREFIX) {
            Some(reply_to)
        } else {
            None
        }
    });
    message.author_session_id = message.author_session_id.and_then(|author_session_id| {
        let author_session_id = author_session_id.trim().to_string();
        is_valid_session_id(&author_session_id).then_some(author_session_id)
    });
    message.resolved_by_message_id = message.resolved_by_message_id.and_then(|message_id| {
        let message_id = message_id.trim().to_string();
        message_id
            .starts_with(MESSAGE_ID_PREFIX)
            .then_some(message_id)
    });
    message.completion_id = message.completion_id.and_then(|completion_id| {
        let completion_id = completion_id.trim().to_ascii_lowercase();
        if is_valid_completion_id(&completion_id) {
            Some(completion_id)
        } else {
            Some("invalid".to_string())
        }
    });
    message.resolution = message
        .resolution
        .map(|resolution| bound_chars(resolution.trim(), MAX_MESSAGE_RESOLUTION_CHARS));
    Some(message)
}

//! JSON session ledger load/save, sanitize-on-restore, and atomic writes.
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::path::PathBuf;

use super::events::{
    is_valid_session_id, sanitize_failure_expectation_result,
    sanitize_persisted_validation_output_summary,
};
use super::model::{
    PersistedSessionLedger, PersistedSessionRecord, SessionEvent, SessionGuards, SessionMessage,
    SessionRecord, DEFAULT_MAX_MESSAGES_PER_SESSION, EVENT_ID_PREFIX, MAX_INPUT_ARRAY_ITEMS,
    MAX_MESSAGE_CHARS, MAX_MESSAGE_RESOLUTION_CHARS, MESSAGE_ID_PREFIX, SESSION_LEDGER_VERSION,
};
use super::query::validate_message_tags;
use super::util::{
    bound_chars, bound_event_error_summary, bound_summary_string, redact_and_bound_value,
};

impl PersistedSessionRecord {
    pub(super) fn from_record(record: &SessionRecord, max_events_per_session: usize) -> Self {
        let event_skip = record.events.len().saturating_sub(max_events_per_session);
        let message_skip = record
            .messages
            .len()
            .saturating_sub(DEFAULT_MAX_MESSAGES_PER_SESSION);
        Self {
            session_id: record.session_id.clone(),
            project: record.project.clone(),
            title: record.title.clone(),
            mode: record.mode,
            guards: record.guards,
            lifecycle: record.lifecycle,
            created_at: record.created_at,
            updated_at: record.updated_at,
            events: record.events.iter().skip(event_skip).cloned().collect(),
            messages: record.messages.iter().skip(message_skip).cloned().collect(),
        }
    }

    pub(super) fn into_record(self, max_events_per_session: usize) -> Option<SessionRecord> {
        let session_id = self.session_id.trim().to_string();
        if !is_valid_session_id(&session_id) {
            return None;
        }
        let events: VecDeque<SessionEvent> = self
            .events
            .into_iter()
            .filter_map(|event| sanitize_persisted_event(event, &session_id))
            .rev()
            .take(max_events_per_session)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let messages: VecDeque<SessionMessage> = self
            .messages
            .into_iter()
            .filter_map(|message| sanitize_persisted_message(message, &session_id))
            .rev()
            .take(DEFAULT_MAX_MESSAGES_PER_SESSION)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Some(SessionRecord {
            session_id,
            project: self.project.map(|value| bound_summary_string(value.trim())),
            title: self.title.map(|value| bound_summary_string(value.trim())),
            mode: self.mode,
            guards: SessionGuards::effective(self.mode, self.guards),
            // Missing ledger field deserializes via #[serde(default)] → Active.
            lifecycle: self.lifecycle,
            created_at: self.created_at,
            updated_at: self.updated_at.max(self.created_at),
            events,
            messages,
            project_instructions: None,
        })
    }
}

pub(super) fn load_persisted_sessions(
    path: &PathBuf,
    max_sessions: usize,
    max_events_per_session: usize,
) -> (
    HashMap<String, SessionRecord>,
    VecDeque<String>,
    usize,
    Option<String>,
) {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return (HashMap::new(), VecDeque::new(), 0, None);
        }
        Err(err) => {
            let error = bound_summary_string(&format!("restore_failed: {}: {err}", path.display()));
            tracing::warn!("session ledger restore failed: {}", error);
            return (HashMap::new(), VecDeque::new(), 0, Some(error));
        }
    };
    let ledger = match serde_json::from_str::<PersistedSessionLedger>(&content) {
        Ok(ledger) => ledger,
        Err(err) => {
            let error = bound_summary_string(&format!(
                "restore_failed: invalid session ledger JSON: {err}"
            ));
            tracing::warn!("session ledger restore failed: {}", error);
            return (HashMap::new(), VecDeque::new(), 0, Some(error));
        }
    };
    if ledger.version != SESSION_LEDGER_VERSION {
        let error = format!(
            "restore_failed: unsupported session ledger version {}",
            ledger.version
        );
        tracing::warn!("session ledger restore failed: {}", error);
        return (HashMap::new(), VecDeque::new(), 0, Some(error));
    }
    let mut records: Vec<SessionRecord> = ledger
        .sessions
        .into_iter()
        .filter_map(|record| record.into_record(max_events_per_session))
        .collect();
    records.sort_by_key(|record| record.updated_at);
    while records.len() > max_sessions {
        records.remove(0);
    }
    let mut sessions = HashMap::new();
    let mut lru = VecDeque::new();
    for record in records {
        lru.push_back(record.session_id.clone());
        sessions.insert(record.session_id.clone(), record);
    }
    let restored_sessions = sessions.len();
    (sessions, lru, restored_sessions, None)
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
    let data = serde_json::to_vec_pretty(ledger).map_err(io::Error::other)?;
    if let Err(err) = fs::write(&tmp_path, data).and_then(|_| fs::rename(&tmp_path, path)) {
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
    event.job_id = event.job_id.map(|value| bound_summary_string(value.trim()));
    event.input_summary = event
        .input_summary
        .map(|value| redact_and_bound_value(&value));
    event.validation_output_summary = event
        .validation_output_summary
        .and_then(|value| sanitize_persisted_validation_output_summary(&event.tool_name, &value));
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
    message.resolution = message
        .resolution
        .map(|resolution| bound_chars(resolution.trim(), MAX_MESSAGE_RESOLUTION_CHARS));
    Some(message)
}

//! In-memory SessionStore: create sessions, record events, and trigger persistence.
use super::super::permissions::PermissionDecision;
use super::super::tool_inputs::SessionMode;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::events::{
    actual_failure_kind_for_tool_result, changed_paths_for_tool, classify_failure_expectation,
    diff_review_like_for_tool, extract_job_id, extract_project, is_valid_session_id,
    validation_output_summary_for_tool_result, SessionToolClassification,
};
use super::model::{
    CurrentSessionKey, PersistedSessionLedger, PersistedSessionRecord, SessionCounts,
    SessionCreateOptions, SessionEvent, SessionGuardDenial, SessionGuards, SessionRecord,
    SessionStoreStatus, SessionSummary, SessionTransport, ToolCallRecorderMetadata, ToolCallStart,
    DEFAULT_MAX_EVENTS_PER_SESSION, DEFAULT_MAX_MESSAGES_PER_SESSION, DEFAULT_MAX_SESSIONS,
    DEFAULT_SUMMARY_LIMIT, EVENT_ID_PREFIX, MAX_SUMMARY_LIMIT, SESSION_ID_PREFIX,
    SESSION_LEDGER_VERSION,
};
use super::persistence::{load_persisted_sessions, write_ledger_atomic};
use super::query::build_messages_summary;
use super::util::{
    bound_event_error_summary, bound_summary_string, now_ts, redact_and_bound_value,
};

#[derive(Debug, Clone)]
pub(crate) struct SessionStore {
    /// Shared session map, bindings, and LRU metadata.
    /// `pub(super)` so `bindings` / `messages` can implement focused `impl` blocks.
    pub(super) inner: Arc<Mutex<SessionStoreInner>>,
    persistence_write_mutex: Arc<Mutex<()>>,
}

#[derive(Debug)]
pub(super) struct SessionStoreInner {
    pub(super) sessions: HashMap<String, SessionRecord>,
    pub(super) current_sessions: HashMap<CurrentSessionKey, String>,
    lru: VecDeque<String>,
    max_sessions: usize,
    max_events_per_session: usize,
    persistence: Option<SessionPersistence>,
}

#[derive(Debug, Clone)]
pub(super) struct SessionPersistence {
    path: PathBuf,
    restored_sessions: usize,
    last_persist_error: Option<String>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SESSIONS, DEFAULT_MAX_EVENTS_PER_SESSION)
    }
}

impl SessionStore {
    pub(crate) fn new(max_sessions: usize, max_events_per_session: usize) -> Self {
        Self::new_in_memory(max_sessions, max_events_per_session)
    }

    pub(crate) fn new_in_memory(max_sessions: usize, max_events_per_session: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SessionStoreInner {
                sessions: HashMap::new(),
                current_sessions: HashMap::new(),
                lru: VecDeque::new(),
                max_sessions,
                max_events_per_session,
                persistence: None,
            })),
            persistence_write_mutex: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn with_persistence(
        path: impl Into<PathBuf>,
        max_sessions: usize,
        max_events_per_session: usize,
    ) -> Self {
        let path = path.into();
        let (sessions, lru, restored_sessions, last_persist_error) =
            load_persisted_sessions(&path, max_sessions, max_events_per_session);
        Self {
            inner: Arc::new(Mutex::new(SessionStoreInner {
                sessions,
                current_sessions: HashMap::new(),
                lru,
                max_sessions,
                max_events_per_session,
                persistence: Some(SessionPersistence {
                    path,
                    restored_sessions,
                    last_persist_error,
                }),
            })),
            persistence_write_mutex: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn status(&self) -> SessionStoreStatus {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        let (persistence, restored_sessions, last_persist_error) = match &inner.persistence {
            Some(persistence) => (
                "enabled".to_string(),
                persistence.restored_sessions,
                persistence.last_persist_error.clone(),
            ),
            None => ("disabled".to_string(), 0, None),
        };
        SessionStoreStatus {
            persistence,
            restored_sessions,
            max_sessions: inner.max_sessions,
            max_events_per_session: inner.max_events_per_session,
            max_messages_per_session: DEFAULT_MAX_MESSAGES_PER_SESSION,
            last_persist_error,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn start_session(
        &self,
        project: Option<String>,
        title: Option<String>,
    ) -> SessionSummary {
        self.start_session_with_guards(
            project,
            title,
            SessionMode::Normal,
            SessionGuards::default(),
        )
    }

    pub(crate) fn start_session_with_guards(
        &self,
        project: Option<String>,
        title: Option<String>,
        mode: SessionMode,
        guards: SessionGuards,
    ) -> SessionSummary {
        self.start_session_with_options(SessionCreateOptions {
            project,
            title,
            mode,
            guards,
            project_instructions: None,
        })
    }

    /// Create a new session from a full options struct. This is the single
    /// entry point that stores session-creation inputs (including project
    /// instructions) on the `SessionRecord`. `start_session_with_guards`
    /// delegates here with `project_instructions: None`.
    pub(crate) fn start_session_with_options(&self, opts: SessionCreateOptions) -> SessionSummary {
        let session_id = format!("{SESSION_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let now = now_ts();
        let guards = SessionGuards::effective(opts.mode, opts.guards);
        let record = SessionRecord {
            session_id: session_id.clone(),
            project: opts.project,
            title: opts.title,
            mode: opts.mode,
            guards,
            created_at: now,
            updated_at: now,
            messages: VecDeque::new(),
            events: VecDeque::new(),
            project_instructions: opts.project_instructions,
        };
        let summary = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.sessions.insert(session_id.clone(), record);
            inner.touch(&session_id);
            inner.enforce_session_bound();
            inner
                .summary(&session_id, Some(DEFAULT_SUMMARY_LIMIT))
                .expect("newly inserted session must summarize")
        };
        self.persist_after_mutation();
        summary
    }

    pub(crate) fn summary(&self, session_id: &str, limit: Option<usize>) -> Option<SessionSummary> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        inner.summary(session_id, limit)
    }

    pub(crate) fn contains_session(&self, session_id: &str) -> bool {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner.sessions.contains_key(session_id)
    }

    pub(crate) fn session_project(&self, session_id: &str) -> Option<Option<String>> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner
            .sessions
            .get(session_id)
            .map(|record| record.project.clone())
    }

    pub(crate) fn guard_state(&self, session_id: &str) -> Option<(SessionMode, SessionGuards)> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner
            .sessions
            .get(session_id)
            .map(|record| (record.mode, record.guards))
    }

    pub(crate) fn guard_denial(
        &self,
        session_id: &str,
        tool_name: &str,
    ) -> Option<SessionGuardDenial> {
        let (mode, guards) = self.guard_state(session_id)?;
        let classification = SessionToolClassification::for_tool(tool_name);
        if guards.deny_write_tools && classification.write_like {
            return Some(SessionGuardDenial {
                mode,
                guard: "deny_write_tools",
            });
        }
        if guards.deny_shell_tools && classification.shell_like {
            return Some(SessionGuardDenial {
                mode,
                guard: "deny_shell_tools",
            });
        }
        None
    }

    #[allow(dead_code)]
    pub(crate) fn record_tool_call_started(
        &self,
        session_id: Option<&str>,
        transport: SessionTransport,
        tool_name: &str,
        arguments: &Value,
    ) -> Option<ToolCallStart> {
        self.record_tool_call_started_with_options(
            session_id, transport, tool_name, arguments, None,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn record_tool_call_started_with_options(
        &self,
        session_id: Option<&str>,
        transport: SessionTransport,
        tool_name: &str,
        arguments: &Value,
        resolved_project: Option<String>,
    ) -> Option<ToolCallStart> {
        self.record_tool_call_started_with_metadata(
            session_id,
            transport,
            tool_name,
            arguments,
            resolved_project,
            ToolCallRecorderMetadata::from_arguments(arguments),
        )
    }

    pub(crate) fn record_tool_call_started_with_metadata(
        &self,
        session_id: Option<&str>,
        transport: SessionTransport,
        tool_name: &str,
        arguments: &Value,
        resolved_project: Option<String>,
        metadata: ToolCallRecorderMetadata,
    ) -> Option<ToolCallStart> {
        let session_id = session_id?.trim();
        if !is_valid_session_id(session_id) || !self.contains_session(session_id) {
            return None;
        }
        let now = now_ts();
        let event_id = format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let project = extract_project(arguments);
        let classification = SessionToolClassification::for_tool(tool_name);
        let risk_class = classification.risk_class.to_string();
        let changed_paths = changed_paths_for_tool(tool_name, arguments);
        let diff_review_like = diff_review_like_for_tool(tool_name, arguments);
        let input_summary = Some(redact_and_bound_value(arguments));
        let expectation = metadata.expectation;
        let start = ToolCallStart {
            event_id: event_id.clone(),
            session_id: session_id.to_string(),
            transport,
            tool_name: tool_name.to_string(),
            project: project.clone(),
            resolved_project: resolved_project.clone(),
            risk_class: risk_class.clone(),
            read_like: classification.read_like,
            write_like: classification.write_like,
            shell_like: classification.shell_like,
            git_like: classification.git_like,
            change_summary_like: classification.change_summary_like,
            diff_review_like,
            changed_paths: changed_paths.clone(),
            started_at: now,
            started_instant: Instant::now(),
            permission: None,
            expectation: expectation.clone(),
        };
        self.push_event(SessionEvent {
            event_id,
            session_id: session_id.to_string(),
            kind: "tool_call_started".to_string(),
            timestamp: now,
            transport: transport.as_str().to_string(),
            tool_name: tool_name.to_string(),
            project,
            resolved_project,
            risk_class,
            read_like: classification.read_like,
            write_like: classification.write_like,
            shell_like: classification.shell_like,
            git_like: classification.git_like,
            change_summary_like: classification.change_summary_like,
            diff_review_like,
            started_at: Some(now),
            finished_at: None,
            duration_ms: None,
            status: None,
            exit_code: None,
            failure_kind: None,
            error_kind: None,
            expected_failure: expectation.expected_failure.then_some(true),
            expected_failure_kind: expectation.expected_failure_kind.clone(),
            assertion_name: expectation.assertion_name.clone(),
            actual_failure_kind: None,
            failure_expectation_result: None,
            warning_kind: None,
            session_project: None,
            request_project: None,
            allow_cross_project_session_required: None,
            allow_cross_project_session: None,
            error_message_summary: None,
            changed_paths,
            job_id: None,
            input_summary,
            validation_output_summary: None,
            permission: None,
        });
        Some(start)
    }

    pub(crate) fn record_permission_decision(
        &self,
        start: &mut ToolCallStart,
        permission: PermissionDecision,
    ) {
        start.permission = Some(permission.clone());
        let persisted = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let Some(record) = inner.sessions.get_mut(&start.session_id) else {
                return;
            };
            if let Some(event) = record
                .events
                .iter_mut()
                .rev()
                .find(|event| event.event_id == start.event_id)
            {
                event.permission = Some(permission);
                true
            } else {
                false
            }
        };
        if persisted {
            self.persist_after_mutation();
        }
    }

    pub(crate) fn record_tool_call_finished(
        &self,
        start: Option<ToolCallStart>,
        success: bool,
        output: &Value,
        error: Option<&str>,
        error_kind: Option<&str>,
    ) -> Option<String> {
        let Some(start) = start else {
            return None;
        };
        let finished_at = now_ts();
        let duration_ms = start
            .started_instant
            .elapsed()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let event_id = format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let failure_kind = output
            .get("failure_kind")
            .and_then(Value::as_str)
            .map(str::to_string);
        let error_kind = error_kind
            .or_else(|| error.and_then(|_| output.get("failure_kind").and_then(Value::as_str)))
            .or_else(|| error.map(|_| "runtime_error"));
        let actual_failure_kind = actual_failure_kind_for_tool_result(output, error, error_kind);
        let failure_expectation_result = classify_failure_expectation(
            success,
            &start.expectation,
            actual_failure_kind.as_deref(),
        );
        let warning_kind = output
            .get("warning_kind")
            .and_then(Value::as_str)
            .map(str::to_string);
        let session_project = output
            .get("session_project")
            .and_then(Value::as_str)
            .map(str::to_string);
        let request_project = output
            .get("request_project")
            .and_then(Value::as_str)
            .map(str::to_string);
        let allow_cross_project_session_required = output
            .get("allow_cross_project_session_required")
            .and_then(Value::as_bool);
        let allow_cross_project_session = output
            .get("allow_cross_project_session")
            .and_then(Value::as_bool);
        let error_message_summary =
            error.map(|message| bound_event_error_summary(message, start.shell_like));
        let validation_output_summary =
            validation_output_summary_for_tool_result(&start.tool_name, output);
        self.push_event(SessionEvent {
            event_id: event_id.clone(),
            session_id: start.session_id,
            kind: "tool_call_finished".to_string(),
            timestamp: finished_at,
            transport: start.transport.as_str().to_string(),
            tool_name: start.tool_name,
            project: start.project,
            resolved_project: start.resolved_project,
            risk_class: start.risk_class,
            read_like: start.read_like,
            write_like: start.write_like,
            shell_like: start.shell_like,
            git_like: start.git_like,
            change_summary_like: start.change_summary_like,
            diff_review_like: start.diff_review_like,
            started_at: Some(start.started_at),
            finished_at: Some(finished_at),
            duration_ms: Some(duration_ms),
            status: Some(if success { "succeeded" } else { "failed" }.to_string()),
            exit_code: output.get("exit_code").and_then(Value::as_i64),
            failure_kind,
            error_kind: error.map(|_| error_kind.unwrap_or("runtime_error").to_string()),
            expected_failure: start.expectation.expected_failure.then_some(true),
            expected_failure_kind: start.expectation.expected_failure_kind,
            assertion_name: start.expectation.assertion_name,
            actual_failure_kind,
            failure_expectation_result: Some(failure_expectation_result.to_string()),
            warning_kind,
            session_project,
            request_project,
            allow_cross_project_session_required,
            allow_cross_project_session,
            error_message_summary,
            changed_paths: start.changed_paths,
            job_id: extract_job_id(output),
            input_summary: None,
            validation_output_summary,
            permission: start.permission,
        });
        Some(event_id)
    }

    fn push_event(&self, event: SessionEvent) {
        let persisted = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let max_events_per_session = inner.max_events_per_session;
            if let Some(record) = inner.sessions.get_mut(&event.session_id) {
                record.updated_at = now_ts();
                record.events.push_back(event);
                while record.events.len() > max_events_per_session {
                    record.events.pop_front();
                }
                let session_id = record.session_id.clone();
                inner.touch(&session_id);
                true
            } else {
                false
            }
        };
        if persisted {
            self.persist_after_mutation();
        }
    }

    pub(super) fn persist_after_mutation(&self) {
        self.persist_after_mutation_with(write_ledger_atomic);
    }

    pub(super) fn persist_after_mutation_with(
        &self,
        write_ledger: impl FnOnce(&PathBuf, &PersistedSessionLedger) -> io::Result<()>,
    ) {
        let _write_guard = self
            .persistence_write_mutex
            .lock()
            .expect("session persistence mutex poisoned");
        let Some((path, ledger)) = ({
            let inner = self.inner.lock().expect("session store mutex poisoned");
            let path = inner
                .persistence
                .as_ref()
                .map(|persistence| persistence.path.clone());
            path.map(|path| (path, inner.to_persisted_ledger()))
        }) else {
            return;
        };
        let result = write_ledger(&path, &ledger).map_err(|err| {
            bound_summary_string(&format!("persist_failed: {}: {err}", path.display()))
        });
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        let Some(persistence) = inner.persistence.as_mut() else {
            return;
        };
        match result {
            Ok(()) => persistence.last_persist_error = None,
            Err(error) => {
                tracing::warn!("session ledger persistence failed: {}", error);
                persistence.last_persist_error = Some(error);
            }
        }
    }
}

impl SessionStoreInner {
    fn to_persisted_ledger(&self) -> PersistedSessionLedger {
        let sessions = self
            .lru
            .iter()
            .filter_map(|session_id| self.sessions.get(session_id))
            .map(|record| PersistedSessionRecord::from_record(record, self.max_events_per_session))
            .collect();
        PersistedSessionLedger {
            version: SESSION_LEDGER_VERSION,
            sessions,
        }
    }

    pub(super) fn touch(&mut self, session_id: &str) {
        self.lru.retain(|id| id != session_id);
        if self.sessions.contains_key(session_id) {
            self.lru.push_back(session_id.to_string());
        }
    }

    fn enforce_session_bound(&mut self) {
        while self.sessions.len() > self.max_sessions {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            self.sessions.remove(&oldest);
        }
    }

    pub(super) fn summary(&self, session_id: &str, limit: Option<usize>) -> Option<SessionSummary> {
        let record = self.sessions.get(session_id)?;
        let limit = limit
            .unwrap_or(DEFAULT_SUMMARY_LIMIT)
            .clamp(0, MAX_SUMMARY_LIMIT);
        let finished_events: Vec<&SessionEvent> = record
            .events
            .iter()
            .filter(|event| event.kind == "tool_call_finished")
            .collect();
        let counts = SessionCounts {
            tool_calls: finished_events.len(),
            succeeded: finished_events
                .iter()
                .filter(|event| event.status.as_deref() == Some("succeeded"))
                .count(),
            failed: finished_events
                .iter()
                .filter(|event| event.status.as_deref() == Some("failed"))
                .count(),
            read_like: finished_events
                .iter()
                .filter(|event| event.read_like)
                .count(),
            write_like: finished_events
                .iter()
                .filter(|event| event.write_like)
                .count(),
            shell_like: finished_events
                .iter()
                .filter(|event| event.shell_like)
                .count(),
            git_like: finished_events
                .iter()
                .filter(|event| event.git_like)
                .count(),
            change_summary_like: finished_events
                .iter()
                .filter(|event| event.change_summary_like)
                .count(),
        };
        let skip = record.events.len().saturating_sub(limit);
        let project_instructions = record
            .project_instructions
            .as_ref()
            .map(|snapshot| snapshot.to_summary());
        Some(SessionSummary {
            session_id: record.session_id.clone(),
            project: record.project.clone(),
            title: record.title.clone(),
            mode: record.mode,
            guards: record.guards,
            created_at: record.created_at,
            updated_at: record.updated_at,
            counts,
            events: record.events.iter().skip(skip).cloned().collect(),
            project_instructions,
            messages: build_messages_summary(record),
        })
    }
}

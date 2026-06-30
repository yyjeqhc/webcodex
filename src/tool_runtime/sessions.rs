use super::metadata::{tool_metadata, ToolPathHint, ToolRisk};
use super::project_instructions::{
    ProjectInstructionsSnapshot, ProjectInstructionsSummarySnapshot,
};
use super::types::SessionMode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(crate) const SESSION_ID_PREFIX: &str = "wc_sess_";
const EVENT_ID_PREFIX: &str = "evt_";
const DEFAULT_MAX_SESSIONS: usize = 100;
const DEFAULT_MAX_EVENTS_PER_SESSION: usize = 200;
const DEFAULT_SUMMARY_LIMIT: usize = 50;
const MAX_SUMMARY_LIMIT: usize = 200;
const MAX_SUMMARY_STRING_CHARS: usize = 240;
const MAX_INPUT_STRING_CHARS: usize = 120;
const MAX_INPUT_OBJECT_KEYS: usize = 16;
const MAX_INPUT_ARRAY_ITEMS: usize = 8;
pub(crate) const MESSAGE_ID_PREFIX: &str = "wc_msg_";
pub(crate) const DEFAULT_MAX_MESSAGES_PER_SESSION: usize = 200;
pub(crate) const DEFAULT_MESSAGE_LIST_LIMIT: usize = 50;
pub(crate) const MAX_MESSAGE_LIST_LIMIT: usize = 100;
pub(crate) const MAX_MESSAGE_CHARS: usize = 8000;
pub(crate) const MAX_MESSAGE_TAGS: usize = 16;
pub(crate) const MAX_MESSAGE_TAG_CHARS: usize = 64;
pub(crate) const MAX_MESSAGE_RESOLUTION_CHARS: usize = 8000;
const MAX_MESSAGE_SUMMARY_CHARS: usize = 240;
const SUMMARY_MESSAGE_GROUP_LIMIT: usize = 5;

#[derive(Debug, Clone)]
pub(crate) struct SessionStore {
    inner: Arc<Mutex<SessionStoreInner>>,
}

#[derive(Debug)]
struct SessionStoreInner {
    sessions: HashMap<String, SessionRecord>,
    current_sessions: HashMap<CurrentSessionKey, String>,
    lru: VecDeque<String>,
    max_sessions: usize,
    max_events_per_session: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CurrentSessionKey {
    pub(crate) principal_kind: String,
    pub(crate) principal_id: String,
    pub(crate) transport: String,
    pub(crate) resolved_project: String,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    session_id: String,
    project: Option<String>,
    title: Option<String>,
    mode: SessionMode,
    guards: SessionGuards,
    created_at: i64,
    updated_at: i64,
    events: VecDeque<SessionEvent>,
    messages: VecDeque<SessionMessage>,
    project_instructions: Option<ProjectInstructionsSnapshot>,
}

/// Options for creating a new session. Using a struct keeps the
/// `start_session*` family stable as new session-creation inputs (such as
/// project instructions) are added.
#[derive(Debug, Clone)]
pub(crate) struct SessionCreateOptions {
    pub(crate) project: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) mode: SessionMode,
    pub(crate) guards: SessionGuards,
    pub(crate) project_instructions: Option<ProjectInstructionsSnapshot>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct SessionGuards {
    pub(crate) deny_write_tools: bool,
    pub(crate) deny_shell_tools: bool,
}

impl SessionGuards {
    pub(crate) fn effective(mode: SessionMode, guards: Self) -> Self {
        match mode {
            SessionMode::Normal => guards,
            SessionMode::ReadOnly => Self {
                deny_write_tools: true,
                deny_shell_tools: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SessionGuardDenial {
    pub(crate) mode: SessionMode,
    pub(crate) guard: &'static str,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolCallStart {
    pub(crate) session_id: String,
    pub(crate) transport: SessionTransport,
    pub(crate) tool_name: String,
    pub(crate) project: Option<String>,
    pub(crate) resolved_project: Option<String>,
    pub(crate) risk_class: String,
    pub(crate) read_like: bool,
    pub(crate) write_like: bool,
    pub(crate) shell_like: bool,
    pub(crate) git_like: bool,
    pub(crate) change_summary_like: bool,
    pub(crate) changed_paths: Vec<String>,
    pub(crate) started_at: i64,
    pub(crate) started_instant: Instant,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SessionTransport {
    Api,
    Mcp,
}

impl SessionTransport {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Mcp => "mcp",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionEvent {
    pub(crate) event_id: String,
    pub(crate) session_id: String,
    pub(crate) kind: String,
    pub(crate) timestamp: i64,
    pub(crate) transport: String,
    pub(crate) tool_name: String,
    pub(crate) project: Option<String>,
    pub(crate) resolved_project: Option<String>,
    pub(crate) risk_class: String,
    pub(crate) read_like: bool,
    pub(crate) write_like: bool,
    pub(crate) shell_like: bool,
    pub(crate) git_like: bool,
    pub(crate) change_summary_like: bool,
    pub(crate) started_at: Option<i64>,
    pub(crate) finished_at: Option<i64>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) status: Option<String>,
    pub(crate) exit_code: Option<i64>,
    pub(crate) failure_kind: Option<String>,
    pub(crate) error_kind: Option<String>,
    pub(crate) error_message_summary: Option<String>,
    pub(crate) changed_paths: Vec<String>,
    pub(crate) job_id: Option<String>,
    pub(crate) input_summary: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionMessageKind {
    Note,
    Proposal,
    Question,
    Answer,
    Decision,
    Risk,
    Progress,
    Guidance,
    Todo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionMessageStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionMessagePriority {
    Low,
    Normal,
    High,
}

impl Default for SessionMessagePriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionMessage {
    pub(crate) message_id: String,
    pub(crate) session_id: String,
    pub(crate) created_at: i64,
    pub(crate) kind: SessionMessageKind,
    pub(crate) status: SessionMessageStatus,
    pub(crate) priority: SessionMessagePriority,
    pub(crate) message: String,
    pub(crate) tags: Vec<String>,
    pub(crate) reply_to: Option<String>,
    pub(crate) resolved_at: Option<i64>,
    pub(crate) resolution: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PostSessionMessageInput {
    pub(crate) session_id: String,
    pub(crate) kind: SessionMessageKind,
    pub(crate) message: String,
    pub(crate) tags: Vec<String>,
    pub(crate) reply_to: Option<String>,
    pub(crate) priority: SessionMessagePriority,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ListSessionMessagesFilter {
    pub(crate) kind: Option<SessionMessageKind>,
    pub(crate) status: Option<SessionMessageStatus>,
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionMessagesSummary {
    pub(crate) total: usize,
    pub(crate) open: usize,
    pub(crate) resolved: usize,
    pub(crate) pending_guidance: usize,
    pub(crate) open_questions: usize,
    pub(crate) open_risks: usize,
    pub(crate) open_todos: usize,
    pub(crate) recent_progress: Vec<SessionMessage>,
    pub(crate) guidance: usize,
    pub(crate) progress: usize,
    pub(crate) risk: usize,
    pub(crate) todo: usize,
    pub(crate) question: usize,
    pub(crate) decision: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionDiscussionCounts {
    pub(crate) total: usize,
    pub(crate) open: usize,
    pub(crate) resolved: usize,
    pub(crate) guidance: usize,
    pub(crate) progress: usize,
    pub(crate) risk: usize,
    pub(crate) todo: usize,
    pub(crate) question: usize,
    pub(crate) decision: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionDiscussionSummary {
    pub(crate) counts: SessionDiscussionCounts,
    pub(crate) open_guidance: Vec<SessionMessage>,
    pub(crate) open_questions: Vec<SessionMessage>,
    pub(crate) open_risks: Vec<SessionMessage>,
    pub(crate) open_todos: Vec<SessionMessage>,
    pub(crate) recent_progress: Vec<SessionMessage>,
    pub(crate) recent_decisions: Vec<SessionMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionMessageError {
    UnknownSession,
    UnknownMessage,
    InvalidInput(String),
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionCounts {
    pub(crate) tool_calls: usize,
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) read_like: usize,
    pub(crate) write_like: usize,
    pub(crate) shell_like: usize,
    pub(crate) git_like: usize,
    pub(crate) change_summary_like: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionSummary {
    pub(crate) session_id: String,
    pub(crate) project: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) mode: SessionMode,
    pub(crate) guards: SessionGuards,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) counts: SessionCounts,
    pub(crate) events: Vec<SessionEvent>,
    pub(crate) messages: SessionMessagesSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_instructions: Option<ProjectInstructionsSummarySnapshot>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SESSIONS, DEFAULT_MAX_EVENTS_PER_SESSION)
    }
}

impl SessionStore {
    pub(crate) fn new(max_sessions: usize, max_events_per_session: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SessionStoreInner {
                sessions: HashMap::new(),
                current_sessions: HashMap::new(),
                lru: VecDeque::new(),
                max_sessions,
                max_events_per_session,
            })),
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
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.sessions.insert(session_id.clone(), record);
        inner.touch(&session_id);
        inner.enforce_session_bound();
        inner
            .summary(&session_id, Some(DEFAULT_SUMMARY_LIMIT))
            .expect("newly inserted session must summarize")
    }

    pub(crate) fn summary(&self, session_id: &str, limit: Option<usize>) -> Option<SessionSummary> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        inner.summary(session_id, limit)
    }

    pub(crate) fn bind_current_session(
        &self,
        key: CurrentSessionKey,
        session_id: &str,
    ) -> Option<SessionSummary> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        let summary = inner.summary(session_id, Some(DEFAULT_SUMMARY_LIMIT))?;
        inner
            .current_sessions
            .insert(key, session_id.trim().to_string());
        Some(summary)
    }

    pub(crate) fn current_session(&self, key: &CurrentSessionKey) -> Option<SessionSummary> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        let session_id = inner.current_sessions.get(key).cloned()?;
        inner.touch(&session_id);
        match inner.summary(&session_id, Some(DEFAULT_SUMMARY_LIMIT)) {
            Some(summary) => Some(summary),
            None => {
                inner.current_sessions.remove(key);
                None
            }
        }
    }

    pub(crate) fn current_session_id(&self, key: &CurrentSessionKey) -> Option<String> {
        self.current_session(key).map(|summary| summary.session_id)
    }

    pub(crate) fn unbind_current_session(&self, key: &CurrentSessionKey) -> bool {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.current_sessions.remove(key).is_some()
    }

    pub(crate) fn contains_session(&self, session_id: &str) -> bool {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner.sessions.contains_key(session_id)
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
        if guards.deny_write_tools && is_write_like_tool(tool_name) {
            return Some(SessionGuardDenial {
                mode,
                guard: "deny_write_tools",
            });
        }
        if guards.deny_shell_tools && is_shell_like_tool(tool_name) {
            return Some(SessionGuardDenial {
                mode,
                guard: "deny_shell_tools",
            });
        }
        None
    }

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

    pub(crate) fn record_tool_call_started_with_options(
        &self,
        session_id: Option<&str>,
        transport: SessionTransport,
        tool_name: &str,
        arguments: &Value,
        resolved_project: Option<String>,
    ) -> Option<ToolCallStart> {
        let session_id = session_id?.trim();
        if !is_valid_session_id(session_id) || !self.contains_session(session_id) {
            return None;
        }
        let now = now_ts();
        let event_id = format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let project = extract_project(arguments);
        let risk_class = risk_class_for_tool(tool_name).to_string();
        let read_like = is_read_like_tool(tool_name);
        let write_like = is_write_like_tool(tool_name);
        let shell_like = is_shell_like_tool(tool_name);
        let git_like = is_git_like_tool(tool_name);
        let change_summary_like = is_change_summary_like_tool(tool_name);
        let changed_paths = changed_paths_for_tool(tool_name, arguments);
        let input_summary = Some(redact_and_bound_value(arguments));
        let start = ToolCallStart {
            session_id: session_id.to_string(),
            transport,
            tool_name: tool_name.to_string(),
            project: project.clone(),
            resolved_project: resolved_project.clone(),
            risk_class: risk_class.clone(),
            read_like,
            write_like,
            shell_like,
            git_like,
            change_summary_like,
            changed_paths: changed_paths.clone(),
            started_at: now,
            started_instant: Instant::now(),
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
            read_like,
            write_like,
            shell_like,
            git_like,
            change_summary_like,
            started_at: Some(now),
            finished_at: None,
            duration_ms: None,
            status: None,
            exit_code: None,
            failure_kind: None,
            error_kind: None,
            error_message_summary: None,
            changed_paths,
            job_id: None,
            input_summary,
        });
        Some(start)
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
        let error_message_summary =
            error.map(|message| bound_event_error_summary(message, start.shell_like));
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
            started_at: Some(start.started_at),
            finished_at: Some(finished_at),
            duration_ms: Some(duration_ms),
            status: Some(if success { "succeeded" } else { "failed" }.to_string()),
            exit_code: output.get("exit_code").and_then(Value::as_i64),
            failure_kind,
            error_kind: error.map(|_| error_kind.unwrap_or("runtime_error").to_string()),
            error_message_summary,
            changed_paths: start.changed_paths,
            job_id: extract_job_id(output),
            input_summary: None,
        });
        Some(event_id)
    }

    pub(crate) fn post_message(
        &self,
        input: PostSessionMessageInput,
    ) -> Result<SessionMessage, SessionMessageError> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(&input.session_id);
        let Some(record) = inner.sessions.get_mut(&input.session_id) else {
            return Err(SessionMessageError::UnknownSession);
        };
        let message = validate_message_text(input.message)?;
        let tags = validate_message_tags(input.tags)?;
        if let Some(reply_to) = input.reply_to.as_deref() {
            let found = record
                .messages
                .iter()
                .any(|message| message.message_id == reply_to);
            if !found {
                return Err(SessionMessageError::UnknownMessage);
            }
        }
        let now = now_ts();
        let message = SessionMessage {
            message_id: format!("{MESSAGE_ID_PREFIX}{}", uuid::Uuid::new_v4().simple()),
            session_id: input.session_id.clone(),
            created_at: now,
            kind: input.kind,
            status: SessionMessageStatus::Open,
            priority: input.priority,
            message,
            tags,
            reply_to: input.reply_to,
            resolved_at: None,
            resolution: None,
        };
        record.updated_at = now;
        record.messages.push_back(message.clone());
        while record.messages.len() > DEFAULT_MAX_MESSAGES_PER_SESSION {
            record.messages.pop_front();
        }
        Ok(message)
    }

    pub(crate) fn list_messages(
        &self,
        session_id: &str,
        filter: ListSessionMessagesFilter,
    ) -> Result<Vec<SessionMessage>, SessionMessageError> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        let Some(record) = inner.sessions.get(session_id) else {
            return Err(SessionMessageError::UnknownSession);
        };
        let limit = filter
            .limit
            .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
            .clamp(0, MAX_MESSAGE_LIST_LIMIT);
        Ok(record
            .messages
            .iter()
            .filter(|message| filter.kind.is_none_or(|kind| message.kind == kind))
            .filter(|message| filter.status.is_none_or(|status| message.status == status))
            .rev()
            .take(limit)
            .cloned()
            .collect())
    }

    pub(crate) fn resolve_message(
        &self,
        session_id: &str,
        message_id: &str,
        resolution: Option<String>,
    ) -> Result<SessionMessage, SessionMessageError> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        let Some(record) = inner.sessions.get_mut(session_id) else {
            return Err(SessionMessageError::UnknownSession);
        };
        let Some(message) = record
            .messages
            .iter_mut()
            .find(|message| message.message_id == message_id)
        else {
            return Err(SessionMessageError::UnknownMessage);
        };
        let resolution = match resolution {
            Some(value) => Some(validate_resolution_text(value)?),
            None => None,
        };
        if message.status == SessionMessageStatus::Open {
            message.status = SessionMessageStatus::Resolved;
            message.resolved_at = Some(now_ts());
        }
        if resolution.is_some() {
            message.resolution = resolution;
        }
        record.updated_at = now_ts();
        Ok(message.clone())
    }

    pub(crate) fn discussion_summary(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<SessionDiscussionSummary, SessionMessageError> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        let Some(record) = inner.sessions.get(session_id) else {
            return Err(SessionMessageError::UnknownSession);
        };
        let limit = limit
            .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
            .clamp(0, MAX_MESSAGE_LIST_LIMIT);
        Ok(build_discussion_summary(record, limit))
    }

    fn push_event(&self, event: SessionEvent) {
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
        }
    }
}

impl SessionStoreInner {
    fn touch(&mut self, session_id: &str) {
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

    fn summary(&self, session_id: &str, limit: Option<usize>) -> Option<SessionSummary> {
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

fn validate_message_text(value: String) -> Result<String, SessionMessageError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(SessionMessageError::InvalidInput(
            "message must not be empty".to_string(),
        ));
    }
    if value.chars().count() > MAX_MESSAGE_CHARS {
        return Err(SessionMessageError::InvalidInput(format!(
            "message exceeds {MAX_MESSAGE_CHARS} chars"
        )));
    }
    Ok(value)
}

fn validate_resolution_text(value: String) -> Result<String, SessionMessageError> {
    let value = value.trim().to_string();
    if value.chars().count() > MAX_MESSAGE_RESOLUTION_CHARS {
        return Err(SessionMessageError::InvalidInput(format!(
            "resolution exceeds {MAX_MESSAGE_RESOLUTION_CHARS} chars"
        )));
    }
    Ok(value)
}

fn validate_message_tags(values: Vec<String>) -> Result<Vec<String>, SessionMessageError> {
    if values.len() > MAX_MESSAGE_TAGS {
        return Err(SessionMessageError::InvalidInput(format!(
            "tags exceed {MAX_MESSAGE_TAGS} items"
        )));
    }
    let mut tags = Vec::new();
    for value in values {
        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > MAX_MESSAGE_TAG_CHARS {
            return Err(SessionMessageError::InvalidInput(format!(
                "tag exceeds {MAX_MESSAGE_TAG_CHARS} chars"
            )));
        }
        if !tags.iter().any(|tag| tag == &value) {
            tags.push(value);
        }
    }
    Ok(tags)
}

fn build_messages_summary(record: &SessionRecord) -> SessionMessagesSummary {
    let total = record.messages.len();
    let open = record
        .messages
        .iter()
        .filter(|message| message.status == SessionMessageStatus::Open)
        .count();
    SessionMessagesSummary {
        total,
        open,
        resolved: total.saturating_sub(open),
        pending_guidance: count_open_kind(record, SessionMessageKind::Guidance),
        open_questions: count_open_kind(record, SessionMessageKind::Question),
        open_risks: count_open_kind(record, SessionMessageKind::Risk),
        open_todos: count_open_kind(record, SessionMessageKind::Todo),
        recent_progress: take_recent_kind(
            record,
            SessionMessageKind::Progress,
            None,
            SUMMARY_MESSAGE_GROUP_LIMIT,
        ),
        guidance: count_kind(record, SessionMessageKind::Guidance),
        progress: count_kind(record, SessionMessageKind::Progress),
        risk: count_kind(record, SessionMessageKind::Risk),
        todo: count_kind(record, SessionMessageKind::Todo),
        question: count_kind(record, SessionMessageKind::Question),
        decision: count_kind(record, SessionMessageKind::Decision),
    }
}

fn build_discussion_counts(record: &SessionRecord) -> SessionDiscussionCounts {
    let total = record.messages.len();
    let open = record
        .messages
        .iter()
        .filter(|message| message.status == SessionMessageStatus::Open)
        .count();
    SessionDiscussionCounts {
        total,
        open,
        resolved: total.saturating_sub(open),
        guidance: count_kind(record, SessionMessageKind::Guidance),
        progress: count_kind(record, SessionMessageKind::Progress),
        risk: count_kind(record, SessionMessageKind::Risk),
        todo: count_kind(record, SessionMessageKind::Todo),
        question: count_kind(record, SessionMessageKind::Question),
        decision: count_kind(record, SessionMessageKind::Decision),
    }
}

fn build_discussion_summary(record: &SessionRecord, limit: usize) -> SessionDiscussionSummary {
    SessionDiscussionSummary {
        counts: build_discussion_counts(record),
        open_guidance: take_recent_kind(
            record,
            SessionMessageKind::Guidance,
            Some(SessionMessageStatus::Open),
            limit.min(SUMMARY_MESSAGE_GROUP_LIMIT),
        ),
        open_questions: take_recent_kind(
            record,
            SessionMessageKind::Question,
            Some(SessionMessageStatus::Open),
            limit.min(SUMMARY_MESSAGE_GROUP_LIMIT),
        ),
        open_risks: take_recent_kind(
            record,
            SessionMessageKind::Risk,
            Some(SessionMessageStatus::Open),
            limit.min(SUMMARY_MESSAGE_GROUP_LIMIT),
        ),
        open_todos: take_recent_kind(
            record,
            SessionMessageKind::Todo,
            Some(SessionMessageStatus::Open),
            limit.min(SUMMARY_MESSAGE_GROUP_LIMIT),
        ),
        recent_progress: take_recent_kind(record, SessionMessageKind::Progress, None, limit),
        recent_decisions: take_recent_kind(record, SessionMessageKind::Decision, None, limit),
    }
}

fn count_kind(record: &SessionRecord, kind: SessionMessageKind) -> usize {
    record
        .messages
        .iter()
        .filter(|message| message.kind == kind)
        .count()
}

fn count_open_kind(record: &SessionRecord, kind: SessionMessageKind) -> usize {
    record
        .messages
        .iter()
        .filter(|message| message.kind == kind && message.status == SessionMessageStatus::Open)
        .count()
}

fn take_recent_kind(
    record: &SessionRecord,
    kind: SessionMessageKind,
    status: Option<SessionMessageStatus>,
    limit: usize,
) -> Vec<SessionMessage> {
    record
        .messages
        .iter()
        .rev()
        .filter(|message| message.kind == kind)
        .filter(|message| status.is_none_or(|status| message.status == status))
        .take(limit)
        .cloned()
        .map(bound_message_for_summary)
        .collect()
}

fn bound_message_for_summary(mut message: SessionMessage) -> SessionMessage {
    message.message = bound_chars(&message.message, MAX_MESSAGE_SUMMARY_CHARS);
    if let Some(resolution) = message.resolution.as_mut() {
        *resolution = bound_chars(resolution, MAX_MESSAGE_SUMMARY_CHARS);
    }
    message
}

pub(crate) fn is_valid_session_id(session_id: &str) -> bool {
    session_id.starts_with(SESSION_ID_PREFIX)
        && session_id.len() > SESSION_ID_PREFIX.len()
        && session_id
            .as_bytes()
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

pub(crate) fn extract_project(value: &Value) -> Option<String> {
    value
        .as_object()
        .and_then(|obj| obj.get("project"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn risk_class_for_tool(tool_name: &str) -> &'static str {
    tool_metadata(tool_name).risk.session_risk_class()
}

fn is_read_like_tool(tool_name: &str) -> bool {
    tool_metadata(tool_name).read_only
}

fn is_write_like_tool(tool_name: &str) -> bool {
    tool_metadata(tool_name).risk == ToolRisk::ProjectWrite
}

fn is_shell_like_tool(tool_name: &str) -> bool {
    let metadata = tool_metadata(tool_name);
    metadata.shell_like || metadata.risk == ToolRisk::JobRun
}

fn is_git_like_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "git_status"
            | "git_diff"
            | "git_diff_hunks"
            | "git_diff_summary"
            | "git_log"
            | "show_changes"
            | "git_restore_paths"
            | "discard_untracked"
    )
}

fn is_change_summary_like_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "show_changes" | "git_diff_summary" | "git_diff_hunks"
    )
}

pub(crate) fn changed_paths_for_tool(tool_name: &str, arguments: &Value) -> Vec<String> {
    let metadata = tool_metadata(tool_name);
    if metadata.risk != ToolRisk::ProjectWrite {
        return Vec::new();
    }
    let Some(obj) = arguments.as_object() else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    match metadata.path_hint {
        ToolPathHint::SinglePath => {
            if let Some(path) = obj.get("path").and_then(Value::as_str) {
                push_path(&mut paths, path);
            }
        }
        ToolPathHint::PathList => {
            if let Some(values) = obj.get("paths").and_then(Value::as_array) {
                for path in values.iter().filter_map(Value::as_str) {
                    push_path(&mut paths, path);
                }
            }
        }
        ToolPathHint::Artifact => {
            for key in ["path", "output_path", "target_path"] {
                if let Some(path) = obj.get(key).and_then(Value::as_str) {
                    push_path(&mut paths, path);
                }
            }
        }
        ToolPathHint::Patch | ToolPathHint::None => {}
    }
    paths
}

fn push_path(paths: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if path.is_empty() || paths.iter().any(|p| p == path) {
        return;
    }
    paths.push(path.to_string());
}

fn extract_job_id(output: &Value) -> Option<String> {
    output
        .as_object()
        .and_then(|obj| obj.get("job_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn redact_and_bound_value(value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut redacted = serde_json::Map::new();
            for (key, value) in obj.iter().take(MAX_INPUT_OBJECT_KEYS) {
                if is_sensitive_key(key) {
                    redacted.insert(key.clone(), Value::String("[redacted]".to_string()));
                } else {
                    redacted.insert(key.clone(), redact_and_bound_value(value));
                }
            }
            if obj.len() > MAX_INPUT_OBJECT_KEYS {
                redacted.insert("_truncated".to_string(), json!(true));
            }
            Value::Object(redacted)
        }
        Value::Array(values) => {
            let mut redacted: Vec<Value> = values
                .iter()
                .take(MAX_INPUT_ARRAY_ITEMS)
                .map(redact_and_bound_value)
                .collect();
            if values.len() > MAX_INPUT_ARRAY_ITEMS {
                redacted.push(json!({"_truncated": true}));
            }
            Value::Array(redacted)
        }
        Value::String(s) if looks_like_secret_string(s) => Value::String("[redacted]".to_string()),
        Value::String(s) => Value::String(bound_chars(s, MAX_INPUT_STRING_CHARS)),
        _ => value.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key == "authorization"
        || key == "auth"
        || key == "client_secret"
        || key == "pat"
        || key == "bearer"
}

fn looks_like_secret_string(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("bearer ")
        || value.contains("wc_pat_")
        || value.contains("wc_oat_")
        || value.contains("wc_ort_")
        || value.contains("wc_agent_")
        || value.contains("wc_acct_")
        || value.contains("wc_pair_")
        || value.contains("wc_csec_")
        || value.contains("client_secret")
}

fn bound_summary_string(value: &str) -> String {
    bound_chars(value, MAX_SUMMARY_STRING_CHARS)
}

fn bound_event_error_summary(value: &str, shell_like: bool) -> String {
    if !shell_like {
        return bound_summary_string(value);
    }
    let summary = value
        .lines()
        .take_while(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("stdout_tail:")
                && !trimmed.starts_with("stderr_tail:")
                && !trimmed.starts_with("stdout:")
                && !trimmed.starts_with("stderr:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let summary = summary.trim();
    if summary.is_empty() {
        "shell command failed; stdout/stderr omitted from session event".to_string()
    } else {
        bound_summary_string(summary)
    }
}

fn bound_chars(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_risk_class_uses_metadata() {
        for (tool, risk_class) in [
            ("show_changes", "read_only"),
            ("start_session", "read_only"),
            ("write_project_file", "project_write"),
            ("apply_patch_checked", "project_write"),
            ("run_shell", "job_run"),
            ("cargo_test", "job_run"),
            ("definitely_not_a_tool", "unknown"),
        ] {
            assert_eq!(risk_class_for_tool(tool), risk_class, "{tool}");
        }
    }

    #[test]
    fn changed_paths_single_path_and_path_list_from_metadata() {
        assert_eq!(
            changed_paths_for_tool(
                "write_project_file",
                &json!({"project": "demo", "path": " src/lib.rs "}),
            ),
            vec!["src/lib.rs".to_string()]
        );
        assert_eq!(
            changed_paths_for_tool(
                "delete_project_files",
                &json!({"project": "demo", "paths": ["src/lib.rs", "", "src/lib.rs", "README.md"]}),
            ),
            vec!["src/lib.rs".to_string(), "README.md".to_string()]
        );
        assert_eq!(
            changed_paths_for_tool(
                "save_project_artifact",
                &json!({"project": "demo", "path": "out/image.png"}),
            ),
            vec!["out/image.png".to_string()]
        );
        assert!(changed_paths_for_tool(
            "read_file",
            &json!({"project": "demo", "path": "src/lib.rs"}),
        )
        .is_empty());
        assert!(changed_paths_for_tool(
            "apply_patch_checked",
            &json!({"project": "demo", "patch": "--- a/src/lib.rs\n+++ b/src/lib.rs\n"}),
        )
        .is_empty());
    }

    #[test]
    fn session_store_bounds_event_limit() {
        let store = SessionStore::new(10, 3);
        let summary = store.start_session(None, None);
        for idx in 0..5 {
            let args = json!({"project": "demo", "path": format!("file{idx}.rs")});
            let start = store.record_tool_call_started(
                Some(&summary.session_id),
                SessionTransport::Api,
                "write_project_file",
                &args,
            );
            store.record_tool_call_finished(start, true, &json!({}), None, None);
        }
        let summary = store.summary(&summary.session_id, Some(50)).unwrap();
        assert_eq!(summary.events.len(), 3);
        assert_eq!(summary.counts.tool_calls, 2);
    }

    #[test]
    fn input_summary_redacts_sensitive_keys() {
        let store = SessionStore::default();
        let summary = store.start_session(None, None);
        store.record_tool_call_started(
            Some(&summary.session_id),
            SessionTransport::Api,
            "read_file",
            &json!({
                "project": "demo",
                "token": "super-secret-token",
                "command": "curl -H 'Authorization: Bearer wc_pat_never_store'"
            }),
        );
        let summary = store.summary(&summary.session_id, Some(10)).unwrap();
        assert_eq!(
            summary.events[0].input_summary.as_ref().unwrap()["token"],
            "[redacted]"
        );
        assert_eq!(
            summary.events[0].input_summary.as_ref().unwrap()["command"],
            "[redacted]"
        );
    }

    fn post_message(
        store: &SessionStore,
        session_id: &str,
        kind: SessionMessageKind,
        message: &str,
    ) -> SessionMessage {
        store
            .post_message(PostSessionMessageInput {
                session_id: session_id.to_string(),
                kind,
                message: message.to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap()
    }

    #[test]
    fn post_session_message_creates_message() {
        let store = SessionStore::default();
        let session = store.start_session(None, None);
        let message = store
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Guidance,
                message: "Keep this behind callRuntimeTool.".to_string(),
                tags: vec!["openapi".to_string(), "constraint".to_string()],
                reply_to: None,
                priority: SessionMessagePriority::High,
            })
            .unwrap();

        assert!(message.message_id.starts_with(MESSAGE_ID_PREFIX));
        assert_eq!(message.session_id, session.session_id);
        assert_eq!(message.kind, SessionMessageKind::Guidance);
        assert_eq!(message.status, SessionMessageStatus::Open);
        assert_eq!(message.priority, SessionMessagePriority::High);
        assert_eq!(message.message, "Keep this behind callRuntimeTool.");
        assert_eq!(message.tags, vec!["openapi", "constraint"]);
    }

    #[test]
    fn list_session_messages_filters_and_clamps_limit() {
        let store = SessionStore::default();
        let session = store.start_session(None, None);
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Guidance,
            "g1",
        );
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Progress,
            "p1",
        );
        post_message(&store, &session.session_id, SessionMessageKind::Risk, "r1");

        let guidance = store
            .list_messages(
                &session.session_id,
                ListSessionMessagesFilter {
                    kind: Some(SessionMessageKind::Guidance),
                    status: None,
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(guidance.len(), 1);
        assert_eq!(guidance[0].kind, SessionMessageKind::Guidance);

        let open = store
            .list_messages(
                &session.session_id,
                ListSessionMessagesFilter {
                    kind: None,
                    status: Some(SessionMessageStatus::Open),
                    limit: Some(usize::MAX),
                },
            )
            .unwrap();
        assert_eq!(open.len(), 3);
        assert_eq!(open[0].message, "r1");
    }

    #[test]
    fn resolve_session_message_is_idempotent() {
        let store = SessionStore::default();
        let session = store.start_session(None, None);
        let message = post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Todo,
            "fix it",
        );

        let resolved = store
            .resolve_message(
                &session.session_id,
                &message.message_id,
                Some("Done".to_string()),
            )
            .unwrap();
        assert_eq!(resolved.status, SessionMessageStatus::Resolved);
        assert!(resolved.resolved_at.is_some());
        assert_eq!(resolved.resolution.as_deref(), Some("Done"));

        let resolved_again = store
            .resolve_message(&session.session_id, &message.message_id, None)
            .unwrap();
        assert_eq!(resolved_again.status, SessionMessageStatus::Resolved);
        assert_eq!(resolved_again.resolution.as_deref(), Some("Done"));
    }

    #[test]
    fn session_message_unknown_errors_are_explicit() {
        let store = SessionStore::default();
        let session = store.start_session(None, None);
        let unknown_session = store.post_message(PostSessionMessageInput {
            session_id: "wc_sess_missing".to_string(),
            kind: SessionMessageKind::Note,
            message: "hello".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        });
        assert!(matches!(
            unknown_session,
            Err(SessionMessageError::UnknownSession)
        ));

        let unknown_message = store.resolve_message(&session.session_id, "wc_msg_missing", None);
        assert!(matches!(
            unknown_message,
            Err(SessionMessageError::UnknownMessage)
        ));
    }

    #[test]
    fn session_summary_includes_bounded_message_summary() {
        let store = SessionStore::default();
        let session = store.start_session(None, None);
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Guidance,
            "g1",
        );
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Progress,
            "p1",
        );
        post_message(&store, &session.session_id, SessionMessageKind::Risk, "r1");
        post_message(&store, &session.session_id, SessionMessageKind::Todo, "t1");

        let summary = store.summary(&session.session_id, Some(50)).unwrap();
        assert_eq!(summary.messages.total, 4);
        assert_eq!(summary.messages.open, 4);
        assert_eq!(summary.messages.pending_guidance, 1);
        assert_eq!(summary.messages.open_risks, 1);
        assert_eq!(summary.messages.open_todos, 1);
        assert_eq!(summary.messages.recent_progress.len(), 1);
        assert!(serde_json::to_value(summary)
            .unwrap()
            .get("messages")
            .is_some());
    }
}

use super::metadata::{ToolPathHint, ToolRisk};
use super::permissions::PermissionDecision;
use super::project_instructions::{
    ProjectInstructionsSnapshot, ProjectInstructionsSummarySnapshot,
};
use super::tool_definition::{
    runtime_tool_captures_validation_output, runtime_tool_is_change_summary_like,
    runtime_tool_is_git_like, runtime_tool_is_read_like, runtime_tool_is_shell_like,
    runtime_tool_is_write_like, runtime_tool_metadata, runtime_tool_session_risk_class,
};
use super::tool_inputs::SessionMode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(crate) const SESSION_ID_PREFIX: &str = "wc_sess_";
const EVENT_ID_PREFIX: &str = "evt_";
pub(crate) const DEFAULT_MAX_SESSIONS: usize = 100;
pub(crate) const DEFAULT_MAX_EVENTS_PER_SESSION: usize = 200;
const DEFAULT_SUMMARY_LIMIT: usize = 50;
const MAX_SUMMARY_LIMIT: usize = 200;
const MAX_SUMMARY_STRING_CHARS: usize = 240;
const MAX_INPUT_STRING_CHARS: usize = 120;
const MAX_INPUT_OBJECT_KEYS: usize = 16;
const MAX_INPUT_ARRAY_ITEMS: usize = 8;
pub(crate) const MAX_VALIDATION_EXCERPT_CHARS: usize = 800;
const SESSION_LEDGER_VERSION: u32 = 1;
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
pub(crate) const TOOL_EXPECTATION_RESULT_NONE: &str = "none";
pub(crate) const TOOL_EXPECTATION_RESULT_MATCHED: &str = "matched_expected_failure";
pub(crate) const TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE: &str = "unexpected_failure";
pub(crate) const TOOL_EXPECTATION_RESULT_MISMATCH: &str = "expectation_mismatch";
pub(crate) const TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS: &str = "unexpected_success";
pub(crate) const TOOL_CALL_RECORDING_SESSION_ID_FIELD: &str = "recording_session_id";
pub(crate) const TOOL_EXPECTED_FAILURE_FIELD: &str = "expected_failure";
pub(crate) const TOOL_EXPECTED_FAILURE_KIND_FIELD: &str = "expected_failure_kind";
pub(crate) const TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD: &str = "test_expect_failure_kind";
pub(crate) const TOOL_ASSERTION_NAME_FIELD: &str = "assertion_name";
pub(crate) const TOOL_CALL_EXPECTATION_METADATA_FIELDS: &[&str] = &[
    TOOL_EXPECTED_FAILURE_FIELD,
    TOOL_EXPECTED_FAILURE_KIND_FIELD,
    TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD,
    TOOL_ASSERTION_NAME_FIELD,
];

#[derive(Debug, Clone)]
pub(crate) struct SessionStore {
    inner: Arc<Mutex<SessionStoreInner>>,
    persistence_write_mutex: Arc<Mutex<()>>,
}

#[derive(Debug)]
struct SessionStoreInner {
    sessions: HashMap<String, SessionRecord>,
    current_sessions: HashMap<CurrentSessionKey, String>,
    lru: VecDeque<String>,
    max_sessions: usize,
    max_events_per_session: usize,
    persistence: Option<SessionPersistence>,
}

#[derive(Debug, Clone)]
struct SessionPersistence {
    path: PathBuf,
    restored_sessions: usize,
    last_persist_error: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionStoreStatus {
    pub(crate) persistence: String,
    pub(crate) restored_sessions: usize,
    pub(crate) max_sessions: usize,
    pub(crate) max_events_per_session: usize,
    pub(crate) max_messages_per_session: usize,
    pub(crate) last_persist_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSessionLedger {
    version: u32,
    sessions: Vec<PersistedSessionRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSessionRecord {
    session_id: String,
    project: Option<String>,
    title: Option<String>,
    mode: SessionMode,
    guards: SessionGuards,
    created_at: i64,
    updated_at: i64,
    events: Vec<SessionEvent>,
    messages: Vec<SessionMessage>,
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
    pub(crate) event_id: String,
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
    pub(crate) permission: Option<PermissionDecision>,
    pub(crate) expectation: ToolCallExpectation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ToolCallExpectation {
    pub(crate) expected_failure: bool,
    pub(crate) expected_failure_kind: Option<String>,
    pub(crate) assertion_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ToolCallRecorderMetadata {
    pub(crate) expectation: ToolCallExpectation,
}

impl ToolCallRecorderMetadata {
    pub(crate) fn from_arguments(arguments: &Value) -> Self {
        Self {
            expectation: tool_call_expectation_from_arguments(arguments),
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expected_failure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expected_failure_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) assertion_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) actual_failure_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) failure_expectation_result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) warning_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session_project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) request_project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) allow_cross_project_session_required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) allow_cross_project_session: Option<bool>,
    pub(crate) error_message_summary: Option<String>,
    pub(crate) changed_paths: Vec<String>,
    pub(crate) job_id: Option<String>,
    pub(crate) input_summary: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) validation_output_summary: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) permission: Option<PermissionDecision>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct SessionInboxOpenCounts {
    pub(crate) guidance: usize,
    pub(crate) question: usize,
    pub(crate) todo: usize,
    pub(crate) risk: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionInboxHint {
    pub(crate) has_open_messages: bool,
    pub(crate) open_counts: SessionInboxOpenCounts,
    pub(crate) highest_priority: SessionMessagePriority,
    pub(crate) suggested_next_tool: &'static str,
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

    pub(crate) fn post_message(
        &self,
        input: PostSessionMessageInput,
    ) -> Result<SessionMessage, SessionMessageError> {
        let message = {
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
            message
        };
        self.persist_after_mutation();
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
        let message = {
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
            message.clone()
        };
        self.persist_after_mutation();
        Ok(message)
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

    pub(crate) fn inbox_hint(&self, session_id: &str) -> Option<SessionInboxHint> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        let record = inner.sessions.get(session_id)?;
        build_inbox_hint(record)
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

    fn persist_after_mutation(&self) {
        self.persist_after_mutation_with(write_ledger_atomic);
    }

    fn persist_after_mutation_with(
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

impl PersistedSessionRecord {
    fn from_record(record: &SessionRecord, max_events_per_session: usize) -> Self {
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
            created_at: record.created_at,
            updated_at: record.updated_at,
            events: record.events.iter().skip(event_skip).cloned().collect(),
            messages: record.messages.iter().skip(message_skip).cloned().collect(),
        }
    }

    fn into_record(self, max_events_per_session: usize) -> Option<SessionRecord> {
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
            created_at: self.created_at,
            updated_at: self.updated_at.max(self.created_at),
            events,
            messages,
            project_instructions: None,
        })
    }
}

fn load_persisted_sessions(
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

fn write_ledger_atomic(path: &PathBuf, ledger: &PersistedSessionLedger) -> io::Result<()> {
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

fn sanitize_persisted_event(mut event: SessionEvent, session_id: &str) -> Option<SessionEvent> {
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

fn sanitize_persisted_message(
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

fn build_inbox_hint(record: &SessionRecord) -> Option<SessionInboxHint> {
    let mut counts = SessionInboxOpenCounts::default();
    let mut highest_priority = None;

    for message in record
        .messages
        .iter()
        .filter(|message| message.status == SessionMessageStatus::Open)
    {
        match message.kind {
            SessionMessageKind::Guidance => counts.guidance += 1,
            SessionMessageKind::Question => counts.question += 1,
            SessionMessageKind::Todo => counts.todo += 1,
            SessionMessageKind::Risk => counts.risk += 1,
            _ => continue,
        }
        if highest_priority
            .is_none_or(|priority| priority_rank(message.priority) > priority_rank(priority))
        {
            highest_priority = Some(message.priority);
        }
    }

    highest_priority.map(|priority| SessionInboxHint {
        has_open_messages: true,
        open_counts: counts,
        highest_priority: priority,
        suggested_next_tool: "session_discussion_summary",
    })
}

fn priority_rank(priority: SessionMessagePriority) -> u8 {
    match priority {
        SessionMessagePriority::Low => 0,
        SessionMessagePriority::Normal => 1,
        SessionMessagePriority::High => 2,
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

pub(crate) fn tool_call_expectation_from_arguments(arguments: &Value) -> ToolCallExpectation {
    let Some(obj) = arguments.as_object() else {
        return ToolCallExpectation::default();
    };
    let expected_failure = obj
        .get(TOOL_EXPECTED_FAILURE_FIELD)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let expected_failure_kind = obj
        .get(TOOL_EXPECTED_FAILURE_KIND_FIELD)
        .or_else(|| obj.get(TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(bound_summary_string);
    let assertion_name = obj
        .get(TOOL_ASSERTION_NAME_FIELD)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(bound_summary_string);

    ToolCallExpectation {
        expected_failure,
        expected_failure_kind,
        assertion_name,
    }
}

pub(crate) fn strip_tool_call_expectation_metadata(arguments: Value) -> Value {
    let Value::Object(mut obj) = arguments else {
        return arguments;
    };
    for &key in TOOL_CALL_EXPECTATION_METADATA_FIELDS {
        obj.remove(key);
    }
    Value::Object(obj)
}

pub(crate) fn tool_failure_summary_from_events(events: &[SessionEvent], limit: usize) -> Value {
    let limit = limit.min(20);
    let mut expected_count = 0usize;
    let mut unexpected_count = 0usize;
    let mut expectation_mismatch_count = 0usize;
    let mut unexpected_success_count = 0usize;
    let mut recent_expected = Vec::new();
    let mut recent_unexpected = Vec::new();
    let mut recent_mismatches = Vec::new();
    let mut recent_unexpected_successes = Vec::new();

    for event in events
        .iter()
        .rev()
        .filter(|event| event.kind == "tool_call_finished")
    {
        match event
            .failure_expectation_result
            .as_deref()
            .unwrap_or_else(|| legacy_failure_expectation_result(event))
        {
            TOOL_EXPECTATION_RESULT_MATCHED => {
                expected_count += 1;
                if recent_expected.len() < limit {
                    recent_expected.push(tool_failure_event_summary(event));
                }
            }
            TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE => {
                unexpected_count += 1;
                if recent_unexpected.len() < limit {
                    recent_unexpected.push(tool_failure_event_summary(event));
                }
            }
            TOOL_EXPECTATION_RESULT_MISMATCH => {
                expectation_mismatch_count += 1;
                if recent_mismatches.len() < limit {
                    recent_mismatches.push(tool_failure_event_summary(event));
                }
            }
            TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS => {
                unexpected_success_count += 1;
                if recent_unexpected_successes.len() < limit {
                    recent_unexpected_successes.push(tool_failure_event_summary(event));
                }
            }
            _ => {}
        }
    }

    json!({
        "expected_count": expected_count,
        "unexpected_count": unexpected_count,
        "expectation_mismatch_count": expectation_mismatch_count,
        "unexpected_success_count": unexpected_success_count,
        "recent_expected": recent_expected,
        "recent_unexpected": recent_unexpected,
        "recent_mismatches": recent_mismatches,
        "recent_unexpected_successes": recent_unexpected_successes,
    })
}

fn actual_failure_kind_for_tool_result(
    output: &Value,
    error: Option<&str>,
    error_kind: Option<&str>,
) -> Option<String> {
    let structured_kind = output
        .get("failure_kind")
        .and_then(Value::as_str)
        .or_else(|| output.get("error_kind").and_then(Value::as_str))
        .or_else(|| error_kind.filter(|kind| *kind != "runtime_error"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(bound_summary_string);
    structured_kind
        .or_else(|| error.map(classify_error_message))
        .or_else(|| error_kind.map(bound_summary_string))
}

fn classify_failure_expectation(
    success: bool,
    expectation: &ToolCallExpectation,
    actual_failure_kind: Option<&str>,
) -> &'static str {
    if expectation.expected_failure {
        if success {
            return TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS;
        }
        let Some(expected_kind) = expectation.expected_failure_kind.as_deref() else {
            return TOOL_EXPECTATION_RESULT_MATCHED;
        };
        if Some(expected_kind) == actual_failure_kind {
            TOOL_EXPECTATION_RESULT_MATCHED
        } else {
            TOOL_EXPECTATION_RESULT_MISMATCH
        }
    } else if success {
        TOOL_EXPECTATION_RESULT_NONE
    } else {
        TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE
    }
}

fn classify_error_message(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("session_project_mismatch") {
        "session_project_mismatch"
    } else if lower.contains("unknown_session_id") {
        "unknown_session_id"
    } else if lower.contains("confirmation_required")
        || (lower.contains("confirm") && lower.contains("required"))
    {
        "confirmation_required"
    } else if lower.contains("invalid arguments") || lower.contains("missing field") {
        "invalid_arguments"
    } else if lower.contains("insufficient scope") || lower.contains("missing required scope") {
        "insufficient_scope"
    } else if lower.contains("policy_rejected") || lower.contains("policy rejected") {
        "policy_rejected"
    } else if lower.contains("job_not_found")
        || lower.contains("unknown job")
        || (lower.contains("job") && lower.contains("not found"))
    {
        "job_not_found"
    } else {
        "runtime_error"
    };
    kind.to_string()
}

fn sanitize_failure_expectation_result(value: &str) -> String {
    match value {
        TOOL_EXPECTATION_RESULT_MATCHED
        | TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE
        | TOOL_EXPECTATION_RESULT_MISMATCH
        | TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS
        | TOOL_EXPECTATION_RESULT_NONE => value.to_string(),
        _ => TOOL_EXPECTATION_RESULT_NONE.to_string(),
    }
}

fn legacy_failure_expectation_result(event: &SessionEvent) -> &'static str {
    match event.status.as_deref() {
        Some("failed") => TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE,
        _ => TOOL_EXPECTATION_RESULT_NONE,
    }
}

fn tool_failure_event_summary(event: &SessionEvent) -> Value {
    let success = event.status.as_deref() == Some("succeeded");
    json!({
        "event_id": event.event_id.clone(),
        "tool_name": event.tool_name.clone(),
        "project": event.resolved_project.as_ref().or(event.project.as_ref()).cloned(),
        "assertion_name": event.assertion_name.clone(),
        "expected_failure_kind": event.expected_failure_kind.clone(),
        "actual_failure_kind": event.actual_failure_kind.clone(),
        "status": event.status.clone(),
        "success": success,
        "created_at": event.timestamp,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionToolClassification {
    risk_class: &'static str,
    read_like: bool,
    write_like: bool,
    shell_like: bool,
    git_like: bool,
    change_summary_like: bool,
}

impl SessionToolClassification {
    fn for_tool(tool_name: &str) -> Self {
        Self {
            risk_class: runtime_tool_session_risk_class(tool_name),
            read_like: runtime_tool_is_read_like(tool_name),
            write_like: runtime_tool_is_write_like(tool_name),
            shell_like: runtime_tool_is_shell_like(tool_name),
            git_like: runtime_tool_is_git_like(tool_name),
            change_summary_like: runtime_tool_is_change_summary_like(tool_name),
        }
    }
}

pub(crate) fn changed_paths_for_tool(tool_name: &str, arguments: &Value) -> Vec<String> {
    let metadata = runtime_tool_metadata(tool_name);
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

fn validation_output_summary_for_tool_result(tool_name: &str, output: &Value) -> Option<Value> {
    if !is_cargo_validation_tool(tool_name) {
        return None;
    }
    let stdout_value = output.get("stdout_tail")?;
    let stderr_value = output.get("stderr_tail")?;
    let stdout = stdout_value.as_str()?;
    let stderr = stderr_value.as_str()?;
    let stdout_excerpt = validation_excerpt(stdout);
    let stderr_excerpt = validation_excerpt(stderr);
    let stdout_truncated = output
        .get("stdout_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || stdout_excerpt.filtered;
    let stderr_truncated = output
        .get("stderr_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || stderr_excerpt.filtered;

    let mut summary = json!({
        "tool_name": tool_name,
        "stdout_tail_excerpt": stdout_excerpt.text,
        "stderr_tail_excerpt": stderr_excerpt.text,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "max_excerpt_chars": MAX_VALIDATION_EXCERPT_CHARS,
    });
    if tool_name == "cargo_test" {
        summary["tests_detected"] = cargo_test_tests_detected(output);
        summary["tests_run_count"] = cargo_test_tests_run_count(output);
        summary["zero_tests_run"] = cargo_test_zero_tests_run(output);
    }
    Some(summary)
}

fn sanitize_persisted_validation_output_summary(tool_name: &str, value: &Value) -> Option<Value> {
    if !is_cargo_validation_tool(tool_name) {
        return None;
    }
    let object = value.as_object()?;
    let stdout = object
        .get("stdout_tail_excerpt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stderr = object
        .get("stderr_tail_excerpt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stdout_excerpt = validation_excerpt(stdout);
    let stderr_excerpt = validation_excerpt(stderr);
    let stdout_truncated = object
        .get("stdout_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || stdout_excerpt.filtered;
    let stderr_truncated = object
        .get("stderr_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || stderr_excerpt.filtered;

    let mut summary = json!({
        "tool_name": tool_name,
        "stdout_tail_excerpt": stdout_excerpt.text,
        "stderr_tail_excerpt": stderr_excerpt.text,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "max_excerpt_chars": MAX_VALIDATION_EXCERPT_CHARS,
    });
    if tool_name == "cargo_test" {
        summary["tests_detected"] = persisted_cargo_test_tests_detected(object);
        summary["tests_run_count"] = persisted_cargo_test_tests_run_count(object);
        summary["zero_tests_run"] = persisted_cargo_test_zero_tests_run(object);
    }
    Some(summary)
}

fn is_cargo_validation_tool(tool_name: &str) -> bool {
    runtime_tool_captures_validation_output(tool_name)
}

fn cargo_test_tests_detected(output: &Value) -> Value {
    output
        .get("tests_detected")
        .and_then(Value::as_bool)
        .map_or(Value::Null, Value::Bool)
}

fn cargo_test_tests_run_count(output: &Value) -> Value {
    output
        .get("tests_run_count")
        .and_then(Value::as_u64)
        .map_or(Value::Null, |count| json!(count))
}

fn cargo_test_zero_tests_run(output: &Value) -> Value {
    output
        .get("zero_tests_run")
        .and_then(Value::as_bool)
        .map_or(Value::Null, Value::Bool)
}

fn persisted_cargo_test_tests_detected(object: &serde_json::Map<String, Value>) -> Value {
    object
        .get("tests_detected")
        .and_then(Value::as_bool)
        .map_or(Value::Null, Value::Bool)
}

fn persisted_cargo_test_tests_run_count(object: &serde_json::Map<String, Value>) -> Value {
    object
        .get("tests_run_count")
        .and_then(Value::as_u64)
        .map_or(Value::Null, |count| json!(count))
}

fn persisted_cargo_test_zero_tests_run(object: &serde_json::Map<String, Value>) -> Value {
    object
        .get("zero_tests_run")
        .and_then(Value::as_bool)
        .map_or(Value::Null, Value::Bool)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidationExcerpt {
    text: String,
    filtered: bool,
}

fn validation_excerpt(value: &str) -> ValidationExcerpt {
    let mut filtered = false;
    let mut lines = Vec::new();
    for line in value.lines() {
        let sanitized = sanitize_validation_line(line.trim_end_matches('\r'));
        if sanitized != line {
            filtered = true;
        }
        if validation_line_is_suspicious(&sanitized) {
            filtered = true;
            continue;
        }
        lines.push(sanitized);
    }
    let mut text = lines.join("\n");
    if value.ends_with('\n') && !text.is_empty() {
        text.push('\n');
    }
    let bounded = bound_validation_excerpt(&text);
    if bounded != text {
        filtered = true;
    }
    ValidationExcerpt {
        text: bounded,
        filtered,
    }
}

fn sanitize_validation_line(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_control() || *ch == '\t')
        .collect()
}

fn validation_line_is_suspicious(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let compact: String = lower
        .chars()
        .filter(|ch| !matches!(*ch, '_' | '-') && !ch.is_whitespace())
        .collect();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("authorization")
        || lower.contains("bearer")
        || compact.contains("apikey")
        || compact.contains("accesskey")
        || compact.contains("privatekey")
}

fn bound_validation_excerpt(value: &str) -> String {
    let count = value.chars().count();
    if count <= MAX_VALIDATION_EXCERPT_CHARS {
        return value.to_string();
    }
    if MAX_VALIDATION_EXCERPT_CHARS <= 3 {
        return ".".repeat(MAX_VALIDATION_EXCERPT_CHARS);
    }
    let keep = MAX_VALIDATION_EXCERPT_CHARS - 3;
    let suffix: String = value
        .chars()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{suffix}")
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
    fn session_tool_classification_uses_definition_policy() {
        for (tool, risk_class) in [
            ("show_changes", "read_only"),
            ("start_session", "read_only"),
            ("write_project_file", "project_write"),
            ("apply_patch_checked", "project_write"),
            ("run_shell", "job_run"),
            ("cargo_test", "job_run"),
            ("definitely_not_a_tool", "unknown"),
        ] {
            assert_eq!(
                SessionToolClassification::for_tool(tool).risk_class,
                risk_class,
                "{tool}"
            );
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

    fn persistent_store(path: PathBuf) -> SessionStore {
        SessionStore::with_persistence(path, 10, 10)
    }

    #[test]
    fn session_store_persists_and_restores_basic_session() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let store = persistent_store(ledger.clone());
        let session = store.start_session(
            Some("agent:oe:private-drop".to_string()),
            Some("persistent work".to_string()),
        );

        let restored = persistent_store(ledger);
        let status = restored.status();
        assert_eq!(status.persistence, "enabled");
        assert_eq!(status.restored_sessions, 1);
        assert_eq!(status.last_persist_error, None);
        let summary = restored.summary(&session.session_id, Some(10)).unwrap();
        assert_eq!(summary.session_id, session.session_id);
        assert_eq!(summary.project.as_deref(), Some("agent:oe:private-drop"));
        assert_eq!(summary.title.as_deref(), Some("persistent work"));
    }

    #[test]
    fn session_messages_survive_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let store = persistent_store(ledger.clone());
        let session = store.start_session(None, Some("discussion".to_string()));
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Guidance,
            "keep OpenAPI operation count stable",
        );
        post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Progress,
            "ledger snapshot wired",
        );

        let restored = persistent_store(ledger);
        let messages = restored
            .list_messages(&session.session_id, ListSessionMessagesFilter::default())
            .unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].message, "ledger snapshot wired");
        assert_eq!(messages[1].kind, SessionMessageKind::Guidance);
        let discussion = restored
            .discussion_summary(&session.session_id, Some(10))
            .unwrap();
        assert_eq!(discussion.counts.total, 2);
        assert_eq!(discussion.counts.guidance, 1);
        assert_eq!(discussion.counts.progress, 1);
    }

    #[test]
    fn session_events_survive_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let store = persistent_store(ledger.clone());
        let session = store.start_session(None, Some("events".to_string()));
        let start = store.record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            "git_log",
            &json!({"project": "agent:oe:private-drop", "limit": 1}),
        );
        store.record_tool_call_finished(start, true, &json!({}), None, None);

        let restored = persistent_store(ledger);
        let summary = restored.summary(&session.session_id, Some(10)).unwrap();
        assert_eq!(summary.events.len(), 2);
        assert_eq!(summary.counts.tool_calls, 1);
        assert_eq!(summary.counts.succeeded, 1);
        assert_eq!(summary.counts.git_like, 1);
        assert_eq!(summary.events[1].tool_name, "git_log");
    }

    #[test]
    fn validation_output_summary_survives_restore_sanitized() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let store = persistent_store(ledger.clone());
        let session = store.start_session(None, Some("validation output".to_string()));
        let start = store.record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            "cargo_check",
            &json!({"project": "agent:eval:demo"}),
        );
        store.record_tool_call_finished(
            start,
            false,
            &json!({
                "exit_code": 101,
                "stdout": "full stdout body must not persist",
                "stderr": "full stderr body must not persist",
                "stdout_tail": "token=supersecret\nsafe stdout line\n",
                "stderr_tail": "Authorization: Bearer supersecret\nerror[E0308]: mismatched types\n --> src/lib.rs:12:5\n",
                "stdout_truncated": false,
                "stderr_truncated": false,
            }),
            Some("tool failed"),
            None,
        );

        let restored = persistent_store(ledger);
        let summary = restored.summary(&session.session_id, Some(10)).unwrap();
        let finished = summary
            .events
            .iter()
            .find(|event| event.kind == "tool_call_finished")
            .unwrap();
        let output_summary = finished.validation_output_summary.as_ref().unwrap();
        let stdout_excerpt = output_summary["stdout_tail_excerpt"].as_str().unwrap();
        let stderr_excerpt = output_summary["stderr_tail_excerpt"].as_str().unwrap();

        assert_eq!(output_summary["tool_name"], "cargo_check");
        assert!(stdout_excerpt.contains("safe stdout line"));
        assert!(stderr_excerpt.contains("error[E0308]"));
        assert!(stderr_excerpt.contains("--> src/lib.rs:12:5"));
        for leaked in [
            "full stdout body must not persist",
            "full stderr body must not persist",
            "token=supersecret",
            "Authorization: Bearer supersecret",
        ] {
            assert!(
                !serde_json::to_string(output_summary)
                    .unwrap()
                    .contains(leaked),
                "restored validation_output_summary leaked {leaked}: {output_summary}"
            );
        }
        assert!(stdout_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
        assert!(stderr_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
        assert_eq!(output_summary["stdout_truncated"], true);
        assert_eq!(output_summary["stderr_truncated"], true);
    }

    #[test]
    fn malicious_persisted_validation_output_summary_is_resanitized_on_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let store = persistent_store(ledger.clone());
        let session = store.start_session(None, Some("malicious validation".to_string()));
        for tool_name in ["cargo_check", "run_shell"] {
            let start = store.record_tool_call_started(
                Some(&session.session_id),
                SessionTransport::Api,
                tool_name,
                &json!({"project": "agent:eval:demo"}),
            );
            store.record_tool_call_finished(
                start,
                false,
                &json!({"exit_code": 101}),
                Some("tool failed"),
                None,
            );
        }

        let mut ledger_value: Value =
            serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
        let events = ledger_value["sessions"][0]["events"]
            .as_array_mut()
            .unwrap();
        for event in events {
            if event["kind"] != "tool_call_finished" {
                continue;
            }
            let tool_name = event["tool_name"].clone();
            event["validation_output_summary"] = json!({
                "tool_name": tool_name,
                "stdout_tail_excerpt": format!(
                    "token=abc\nsecret=abc\npassword=abc\napi_key=abc\n{}STDOUT_SAFE_END",
                    "x".repeat(MAX_VALIDATION_EXCERPT_CHARS + 64)
                ),
                "stderr_tail_excerpt": format!(
                    "authorization: basic abc\nbearer abc\nprivate key abc\naccess key abc\n{}STDERR_SAFE_END",
                    "y".repeat(MAX_VALIDATION_EXCERPT_CHARS + 64)
                ),
                "stdout_truncated": false,
                "stderr_truncated": false,
                "max_excerpt_chars": 999999,
            });
        }
        std::fs::write(&ledger, serde_json::to_vec_pretty(&ledger_value).unwrap()).unwrap();

        let restored = persistent_store(ledger);
        let summary = restored.summary(&session.session_id, Some(10)).unwrap();
        let cargo_finished = summary
            .events
            .iter()
            .find(|event| event.kind == "tool_call_finished" && event.tool_name == "cargo_check")
            .unwrap();
        let run_shell_finished = summary
            .events
            .iter()
            .find(|event| event.kind == "tool_call_finished" && event.tool_name == "run_shell")
            .unwrap();
        let output_summary = cargo_finished.validation_output_summary.as_ref().unwrap();
        let stdout_excerpt = output_summary["stdout_tail_excerpt"].as_str().unwrap();
        let stderr_excerpt = output_summary["stderr_tail_excerpt"].as_str().unwrap();
        let serialized = serde_json::to_string(output_summary).unwrap();

        assert!(stdout_excerpt.contains("STDOUT_SAFE_END"));
        assert!(stderr_excerpt.contains("STDERR_SAFE_END"));
        assert!(stdout_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
        assert!(stderr_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
        assert_eq!(
            output_summary["max_excerpt_chars"],
            MAX_VALIDATION_EXCERPT_CHARS
        );
        assert_eq!(output_summary["stdout_truncated"], true);
        assert_eq!(output_summary["stderr_truncated"], true);
        for leaked in [
            "token=abc",
            "secret=abc",
            "password=abc",
            "api_key=abc",
            "authorization: basic abc",
            "bearer abc",
            "private key abc",
            "access key abc",
        ] {
            assert!(
                !serialized.contains(leaked),
                "restored validation_output_summary leaked {leaked}: {serialized}"
            );
        }
        assert!(
            run_shell_finished.validation_output_summary.is_none(),
            "non-cargo tool validation_output_summary must be discarded"
        );
    }

    #[test]
    fn legacy_session_events_without_validation_output_summary_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let store = persistent_store(ledger.clone());
        let session = store.start_session(None, Some("legacy validation".to_string()));
        let start = store.record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            "cargo_check",
            &json!({"project": "agent:eval:demo"}),
        );
        store.record_tool_call_finished(start, true, &json!({"exit_code": 0}), None, None);

        let ledger_text = std::fs::read_to_string(&ledger).unwrap();
        assert!(
            !ledger_text.contains("validation_output_summary"),
            "legacy fixture should omit validation_output_summary: {ledger_text}"
        );
        let restored = persistent_store(ledger);
        let summary = restored.summary(&session.session_id, Some(10)).unwrap();
        let finished = summary
            .events
            .iter()
            .find(|event| event.kind == "tool_call_finished")
            .unwrap();

        assert_eq!(summary.counts.tool_calls, 1);
        assert_eq!(finished.tool_name, "cargo_check");
        assert!(finished.validation_output_summary.is_none());
    }

    #[test]
    fn resolved_message_survives_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let store = persistent_store(ledger.clone());
        let session = store.start_session(None, None);
        let message = post_message(
            &store,
            &session.session_id,
            SessionMessageKind::Todo,
            "finish persistence tests",
        );
        store
            .resolve_message(
                &session.session_id,
                &message.message_id,
                Some("covered".to_string()),
            )
            .unwrap();

        let restored = persistent_store(ledger);
        let messages = restored
            .list_messages(
                &session.session_id,
                ListSessionMessagesFilter {
                    kind: Some(SessionMessageKind::Todo),
                    status: Some(SessionMessageStatus::Resolved),
                    limit: Some(10),
                },
            )
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].status, SessionMessageStatus::Resolved);
        assert_eq!(messages[0].resolution.as_deref(), Some("covered"));
        assert!(messages[0].resolved_at.is_some());
    }

    #[test]
    fn corrupted_ledger_does_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        std::fs::write(&ledger, "{not valid json").unwrap();

        let store = persistent_store(ledger);
        let status = store.status();
        assert_eq!(status.persistence, "enabled");
        assert_eq!(status.restored_sessions, 0);
        assert!(status
            .last_persist_error
            .as_deref()
            .unwrap()
            .contains("restore_failed"));
        assert!(store.summary("wc_sess_missing", None).is_none());
    }

    #[test]
    fn concurrent_persistence_reloads_current_snapshot_before_write() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let store = persistent_store(ledger.clone());
        let session = store.start_session(None, Some("ordered writes".to_string()));
        let (old_snapshot_ready_tx, old_snapshot_ready_rx) = std::sync::mpsc::channel();
        let (allow_old_write_tx, allow_old_write_rx) = std::sync::mpsc::channel();

        let delayed_store = store.clone();
        let delayed_write = std::thread::spawn(move || {
            delayed_store.persist_after_mutation_with(|path, ledger| {
                old_snapshot_ready_tx.send(()).unwrap();
                allow_old_write_rx.recv().unwrap();
                write_ledger_atomic(path, ledger)
            });
        });
        old_snapshot_ready_rx.recv().unwrap();

        let newer_store = store.clone();
        let newer_session_id = session.session_id.clone();
        let newer_mutation = std::thread::spawn(move || {
            post_message(
                &newer_store,
                &newer_session_id,
                SessionMessageKind::Progress,
                "newer mutation",
            );
        });

        let mut newer_message_visible = false;
        for _ in 0..100 {
            let messages = store
                .list_messages(&session.session_id, ListSessionMessagesFilter::default())
                .unwrap();
            if messages
                .iter()
                .any(|message| message.message == "newer mutation")
            {
                newer_message_visible = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(newer_message_visible);

        allow_old_write_tx.send(()).unwrap();
        delayed_write.join().unwrap();
        newer_mutation.join().unwrap();

        let restored = persistent_store(ledger);
        let messages = restored
            .list_messages(&session.session_id, ListSessionMessagesFilter::default())
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message, "newer mutation");
    }

    #[test]
    fn project_instructions_content_not_persisted_or_leaked_after_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = tmp.path().join("sessions.json");
        let secret_body = "secret project rule that must not persist";
        let store = persistent_store(ledger.clone());
        let session = store.start_session_with_options(SessionCreateOptions {
            project: Some("agent:oe:private-drop".to_string()),
            title: Some("instructions".to_string()),
            mode: SessionMode::Normal,
            guards: SessionGuards::default(),
            project_instructions: Some(ProjectInstructionsSnapshot::from_single_file(
                "AGENTS.md",
                secret_body.to_string(),
                1,
            )),
        });

        let serialized = std::fs::read_to_string(&ledger).unwrap();
        assert!(!serialized.contains(secret_body));
        assert!(!serialized.contains("project_instructions"));
        let restored = persistent_store(ledger);
        let summary = restored.summary(&session.session_id, Some(10)).unwrap();
        assert!(summary.project_instructions.is_none());
        let summary_json = serde_json::to_string(&summary).unwrap();
        assert!(!summary_json.contains(secret_body));
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

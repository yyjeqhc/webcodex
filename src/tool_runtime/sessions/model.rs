//! Session data model: IDs, records, events, messages, and summary types.
use super::super::permissions::PermissionDecision;
use super::super::project_instructions::{
    ProjectInstructionsSnapshot, ProjectInstructionsSummarySnapshot,
};
use super::super::tool_inputs::SessionMode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::time::Instant;

pub(crate) const SESSION_ID_PREFIX: &str = "wc_sess_";
pub(super) const EVENT_ID_PREFIX: &str = "evt_";
pub(crate) const DEFAULT_MAX_SESSIONS: usize = 100;
pub(crate) const DEFAULT_MAX_EVENTS_PER_SESSION: usize = 200;
pub(super) const DEFAULT_SUMMARY_LIMIT: usize = 50;
pub(super) const MAX_SUMMARY_LIMIT: usize = 200;
pub(super) const MAX_SUMMARY_STRING_CHARS: usize = 240;
pub(super) const MAX_INPUT_STRING_CHARS: usize = 120;
pub(super) const MAX_INPUT_OBJECT_KEYS: usize = 16;
pub(super) const MAX_INPUT_ARRAY_ITEMS: usize = 8;
pub(crate) const MAX_VALIDATION_EXCERPT_CHARS: usize = 800;
pub(super) const SESSION_LEDGER_VERSION: u32 = 1;
pub(crate) const MESSAGE_ID_PREFIX: &str = "wc_msg_";
pub(crate) const DEFAULT_MAX_MESSAGES_PER_SESSION: usize = 200;
pub(crate) const DEFAULT_MESSAGE_LIST_LIMIT: usize = 50;
pub(crate) const MAX_MESSAGE_LIST_LIMIT: usize = 100;
pub(crate) const MAX_MESSAGE_CHARS: usize = 8000;
pub(crate) const MAX_MESSAGE_TAGS: usize = 16;
pub(crate) const MAX_MESSAGE_TAG_CHARS: usize = 64;
pub(crate) const MAX_MESSAGE_RESOLUTION_CHARS: usize = 8000;
pub(super) const MAX_MESSAGE_SUMMARY_CHARS: usize = 240;
pub(super) const SUMMARY_MESSAGE_GROUP_LIMIT: usize = 5;
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CurrentSessionKey {
    pub(crate) principal_kind: String,
    pub(crate) principal_id: String,
    pub(crate) transport: String,
    pub(crate) resolved_project: String,
}

/// Workflow session lifecycle state.
///
/// Wire values use snake_case (`"active"`, `"closed"`, `"archived"`). Missing
/// ledger fields default to [`SessionLifecycle::Active`] so pre-lifecycle JSON
/// remains readable without migration.
///
/// Transitions (Phase 2):
/// - Create always yields [`SessionLifecycle::Active`].
/// - Explicit `close_session` may transition `Active → Closed`.
/// - `Closed → Active` is not allowed (no reopen in this phase).
/// - `Archived` is reserved for Phase 3+ and is never produced yet.
///
/// LRU eviction remains capacity management, not a lifecycle transition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionLifecycle {
    #[default]
    Active,
    /// Explicitly closed; query remains allowed, mutations are denied.
    Closed,
    /// Reserved — Phase 3+. Not produced yet; treated like Closed for denial.
    Archived,
}

impl SessionLifecycle {
    /// True when the session still accepts work mutations (tools / messages).
    pub(crate) fn allows_mutation(self) -> bool {
        matches!(self, Self::Active)
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Closed => "closed",
            Self::Archived => "archived",
        }
    }
}

/// Result of an explicit close attempt on a known session.
#[derive(Debug, Clone)]
pub(crate) struct SessionCloseOutcome {
    pub(crate) summary: SessionSummary,
    /// True when the session was already `Closed` (or `Archived`); no new
    /// transition event was recorded.
    pub(crate) already_closed: bool,
}

/// Explicit close failures. Unknown ids never create a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionCloseError {
    UnknownSession,
}

/// Lifecycle-based tool denial (orthogonal to mode/guards).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionLifecycleDenial {
    pub(crate) lifecycle: SessionLifecycle,
}

#[derive(Debug, Clone)]
pub(super) struct SessionRecord {
    pub(super) session_id: String,
    pub(super) project: Option<String>,
    pub(super) title: Option<String>,
    pub(super) mode: SessionMode,
    pub(super) guards: SessionGuards,
    /// Explicit lifecycle; always set in memory. Default on load: Active.
    pub(super) lifecycle: SessionLifecycle,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
    pub(super) events: VecDeque<SessionEvent>,
    pub(super) messages: VecDeque<SessionMessage>,
    pub(super) project_instructions: Option<ProjectInstructionsSnapshot>,
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
pub(super) struct PersistedSessionLedger {
    pub(super) version: u32,
    pub(super) sessions: Vec<PersistedSessionRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PersistedSessionRecord {
    pub(super) session_id: String,
    pub(super) project: Option<String>,
    pub(super) title: Option<String>,
    pub(super) mode: SessionMode,
    pub(super) guards: SessionGuards,
    /// Optional on disk for ledger compatibility; missing → Active.
    #[serde(default)]
    pub(super) lifecycle: SessionLifecycle,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
    pub(super) events: Vec<SessionEvent>,
    pub(super) messages: Vec<SessionMessage>,
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
    /// Safe boolean metadata: true when this call contributes to
    /// `review_evidence.diff_review_count` (git diff tools, or
    /// `show_changes(include_diff=true)`). Never stores raw input or diffs.
    pub(crate) diff_review_like: bool,
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
    /// Safe boolean: git diff tools, or `show_changes` with `include_diff=true`.
    /// Defaults to false for legacy ledger rows that omit the field.
    #[serde(default)]
    pub(crate) diff_review_like: bool,
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
    /// Message-board mutation denied because the workflow session is closed
    /// (or archived). Query tools remain available.
    SessionClosed {
        lifecycle: SessionLifecycle,
    },
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
    pub(crate) lifecycle: SessionLifecycle,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) counts: SessionCounts,
    pub(crate) events: Vec<SessionEvent>,
    pub(crate) messages: SessionMessagesSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_instructions: Option<ProjectInstructionsSummarySnapshot>,
}

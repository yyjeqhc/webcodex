//! Session data model: IDs, records, events, messages, and summary types.
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use webcodex_core::project_instructions::{
    ProjectInstructionsSnapshot, ProjectInstructionsSummarySnapshot,
};
use webcodex_core::workflow_session_contract::PermissionDecision;
pub use webcodex_core::workflow_session_contract::{
    SessionExecutionContext, SessionMessageKind, SessionMessagePriority, SessionMessageStatus,
    SessionMode,
};
pub use webcodex_core::workflow_session_contract::{
    MAX_MODEL_VALIDATION_ASSERTION_NAME_CHARS, MAX_TOOL_CALL_ACK_MESSAGE_IDS,
    MAX_TOOL_CALL_ACK_REF_CHARS, SESSION_ID_PREFIX,
    SESSION_INBOX_ACK_REQUIRED_ATTENTION_INSTRUCTION, SESSION_INBOX_ACK_REQUIRED_ATTENTION_REASON,
    TOOL_ACCEPTED_EXIT_CODES_FIELD, TOOL_ASSERTION_NAME_FIELD, TOOL_CALL_ACK_REF_FIELD,
    TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD, TOOL_CALL_RECORDING_SESSION_ID_FIELD,
    TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD, TOOL_EXPECTED_FAILURE_FIELD,
    TOOL_EXPECTED_FAILURE_KIND_FIELD, TOOL_RESULT_EXPECTATION_FIELD,
};

pub const EVENT_ID_PREFIX: &str = "evt_";
pub const CALL_ID_PREFIX: &str = "wc_call_";
pub const LOGICAL_INVOCATION_ID_PREFIX: &str = "wc_inv_";
pub const LOGICAL_INVOCATION_ROLE_RECORDER: &str = "recorder";
pub const LOGICAL_INVOCATION_ROLE_BUSINESS: &str = "business";
/// Backward-compatible default for the hot/materialized Session capacity target.
/// Active canonical Workflow Sessions are authoritative durable identities and are never
/// deleted merely because this advisory target is exceeded.
pub const DEFAULT_MAX_SESSIONS: usize = 100;
/// Independent retained Closed-Session history limit. Active Sessions do not count toward it.
pub const DEFAULT_MAX_RETAINED_CLOSED_SESSIONS: usize = 100;
/// Durable per-Session event retention. This is intentionally larger than the
/// model-facing summary ceiling: long-running coding Sessions keep forensic and
/// recovery evidence without forcing that history into one model response.
pub const DEFAULT_MAX_EVENTS_PER_SESSION: usize = 2000;
/// Exact terminal-validation Job identities retained per Workflow Session. This
/// matches the Runner's authoritative terminal Job inventory bound: while a
/// terminal Job can still be a reconciliation candidate, one of these bounded
/// identities can represent it without turning the Session ledger into an
/// unbounded Job-id history.
pub const MAX_MATERIALIZED_VALIDATION_JOB_IDS: usize =
    webcodex_core::runner_protocol::JOB_INVENTORY_MAX_TERMINAL_JOBS;
/// Maximum project-relative exploration paths retained on one ledger event.
/// This covers the largest currently supported structured search/LSP result
/// while keeping every event independently bounded.
pub const MAX_OBSERVED_PATHS_PER_EVENT: usize = 201;
pub const DEFAULT_SUMMARY_LIMIT: usize = 50;
/// Maximum event tail projected by one model-facing Session summary. Durable
/// retention is independently bounded by `DEFAULT_MAX_EVENTS_PER_SESSION`.
pub const MAX_SUMMARY_LIMIT: usize = 200;
pub const MAX_SUMMARY_STRING_CHARS: usize = 240;
pub const MAX_INPUT_STRING_CHARS: usize = 120;
pub const MAX_INPUT_OBJECT_KEYS: usize = 16;
pub const MAX_INPUT_ARRAY_ITEMS: usize = 8;
pub const MAX_VALIDATION_EXCERPT_CHARS: usize = 800;
pub const SESSION_LEDGER_VERSION: u32 = 2;
pub const MESSAGE_ID_PREFIX: &str = "wc_msg_";
pub const DEFAULT_MAX_MESSAGES_PER_SESSION: usize = 200;
pub const MAX_CODING_INSTRUCTION_CHARS: usize = 4000;
pub const DEFAULT_MESSAGE_LIST_LIMIT: usize = 50;
pub const MAX_MESSAGE_LIST_LIMIT: usize = 100;
pub const MAX_SESSION_MESSAGE_OBSERVATION_TOKEN_LEN: usize = 37;
pub const MAX_MESSAGE_CHARS: usize = 8000;
pub const MAX_MESSAGE_TAGS: usize = 16;
pub const MAX_MESSAGE_TAG_CHARS: usize = 64;
pub const MAX_MESSAGE_RESOLUTION_CHARS: usize = 8000;
pub const MAX_MESSAGE_COMPLETION_KEY_CHARS: usize = 128;
pub const MAX_MESSAGE_DELIVERY_KEY_CHARS: usize = 128;
pub const MESSAGE_COMPLETION_FINGERPRINT_HEX_CHARS: usize = 64;
pub const MAX_MESSAGE_SUMMARY_CHARS: usize = 240;
pub const SUMMARY_MESSAGE_GROUP_LIMIT: usize = 5;
pub const TOOL_EXPECTATION_RESULT_NONE: &str = "none";
pub const TOOL_EXPECTATION_RESULT_MATCHED: &str = "matched_expected_failure";
pub const TOOL_EXPECTATION_RESULT_MATCHED_RESULT: &str = "matched_expected_result";
pub const TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE: &str = "unexpected_failure";
pub const TOOL_EXPECTATION_RESULT_MISMATCH: &str = "expectation_mismatch";
pub const TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS: &str = "unexpected_success";
/// Workflow session lifecycle state.
///
/// Canonical wire values are `"active"` and `"closed"`. Lifecycle is explicit
/// persisted authority: missing or unknown persisted values are rejected row-local
/// during restore and never become an active mutable Session.
///
/// Transitions:
/// - Create always yields [`SessionLifecycle::Active`].
/// - Explicit `close_session` may transition `Active → Closed`.
/// - `Closed → Active` is not allowed.
///
/// Residency / historical-retention pressure remains capacity management, not a lifecycle
/// transition. In particular, an Active canonical Session is never deleted by capacity pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycle {
    Active,
    /// Explicitly closed; query remains allowed, mutations are denied.
    Closed,
}

impl SessionLifecycle {
    /// True when the session still accepts work mutations (tools / messages).
    pub fn allows_mutation(self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Closed => "closed",
        }
    }
}

/// Result of an explicit close attempt on a known session.
#[derive(Debug, Clone)]
pub struct SessionCloseOutcome {
    pub summary: SessionSummary,
    /// True when the session was already `Closed`; no new transition event was
    /// recorded.
    pub already_closed: bool,
}

/// Explicit close failures. Unknown ids never create a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCloseError {
    UnknownSession,
}

/// Lifecycle-based tool denial (orthogonal to mode/guards).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLifecycleDenial {
    pub lifecycle: SessionLifecycle,
}

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub project: Option<String>,
    /// Domain-separated SHA-256 of the canonical creation-time authority group.
    /// The historical field name is retained only in persistence; in-memory
    /// mutable/queryable Sessions always carry a canonical fingerprint and never
    /// retain raw authority identity material.
    pub owner_authority_fingerprint: String,
    pub title: Option<String>,
    pub mode: SessionMode,
    pub guards: SessionGuards,
    pub execution_context: SessionExecutionContext,
    /// Explicit canonical lifecycle; always set in memory.
    pub lifecycle: SessionLifecycle,
    pub created_at: i64,
    pub updated_at: i64,
    pub events: VecDeque<Arc<SessionEvent>>,
    /// Cumulative number of events ever appended to this session ledger,
    /// including events the per-session event cap has since evicted. This is
    /// the source of truth for "did the durable ledger ever hold more events
    /// than are retained now". The persisted counterpart carries the additive
    /// serde default; the in-memory record is always constructed explicitly.
    pub events_observed: u64,
    /// Git tree captured exactly once when a fresh coding Workflow Session is
    /// created. `None` means startup was not a Git repository; continuation
    /// never retroactively creates or replaces this baseline.
    pub git_baseline_tree: Option<String>,
    /// Sticky monotonic fact: this exact Workflow Session has observed at least
    /// one successful canonical first-class repository Edit whose ToolResult
    /// reported `state_changed=true`. It is independent of retained event history.
    pub repository_edit_observed: bool,
    /// Bounded durable exact identities for terminal structured-validation Jobs
    /// already synthesized into this Session. Independent of the retained event
    /// deque so event FIFO eviction cannot resurrect an authoritative Job.
    pub materialized_validation_job_ids: VecDeque<String>,
    pub messages: VecDeque<Arc<SessionMessage>>,
    /// Durable replay identity for optional message delivery keys. Both map keys
    /// and payload fingerprints are domain-separated SHA-256 digests; raw keys,
    /// principals, Window metadata, and message bodies are never retained here.
    pub message_delivery_replays: BTreeMap<String, SessionMessageDeliveryReplay>,
    /// Durable Session-local monotonic message-state revision. This is never
    /// exposed as a public cursor; callers receive an opaque Session-bound token.
    pub message_observation_revision: u64,
    /// Highest last-message revision no longer recoverable because that message
    /// was evicted or sanitized from retained state.
    pub message_observation_floor: u64,
    /// Last observable mutation revision for each currently retained message.
    pub message_observation_revisions: BTreeMap<String, u64>,
    /// Highest evicted direct-reply revision for each retained exact todo.
    /// This separates assignment-local retention loss from unrelated message
    /// traffic so a fence is not invalidated merely because another thread was
    /// evicted.
    pub assignment_history_floors: BTreeMap<String, u64>,
    /// True only when the retained per-todo history floors are known complete.
    /// Corrupt/legacy restore paths may clear this; assignment reads then remain
    /// fail-closed for that restored Session rather than guessing at lost history.
    pub assignment_history_tracking_complete: bool,
    /// Exact-fence replay metadata keyed by completed todo. Canonical live
    /// completions store `Some(SHA-256 fingerprint)`; `None` is retained only to
    /// represent historical no-fence completion metadata without inventing a fence.
    pub completion_assignment_fence_fingerprints: BTreeMap<String, Option<String>>,
    /// True when every retained completion in this Session has known fence
    /// metadata. A historical `None` remains known no-fence history and is never
    /// admissible for live completion replay.
    pub completion_assignment_fence_tracking_complete: bool,
    pub project_instructions: Option<ProjectInstructionsSnapshot>,
}

/// Internal residency state is deliberately orthogonal to business lifecycle.
/// Active sessions currently stay hot even when the advisory hot target is exceeded; they are
/// never deleted to enforce that target. Historical closed sessions may keep only one compact,
/// immutable durable JSON object plus the small metadata required for lifecycle/authorization
/// checks and retention bookkeeping.
#[derive(Debug)]
pub enum StoredSession {
    Hot(SessionRecord),
    Cold(ColdSessionRecord),
}

#[derive(Debug, Clone)]
pub struct ColdSessionRecord {
    pub session_id: String,
    pub project: Option<String>,
    pub owner_authority_fingerprint: String,
    pub mode: SessionMode,
    pub guards: SessionGuards,
    pub lifecycle: SessionLifecycle,
    pub updated_at: i64,
    pub project_instructions: Option<ProjectInstructionsSummarySnapshot>,
    pub raw: Arc<RawValue>,
}

impl StoredSession {
    pub fn session_id(&self) -> &str {
        match self {
            Self::Hot(record) => &record.session_id,
            Self::Cold(record) => &record.session_id,
        }
    }

    pub fn project(&self) -> Option<&str> {
        match self {
            Self::Hot(record) => record.project.as_deref(),
            Self::Cold(record) => record.project.as_deref(),
        }
    }

    pub fn owner_authority_fingerprint(&self) -> &str {
        match self {
            Self::Hot(record) => record.owner_authority_fingerprint.as_str(),
            Self::Cold(record) => record.owner_authority_fingerprint.as_str(),
        }
    }

    pub fn lifecycle(&self) -> SessionLifecycle {
        match self {
            Self::Hot(record) => record.lifecycle,
            Self::Cold(record) => record.lifecycle,
        }
    }

    pub fn mode_guards(&self) -> (SessionMode, SessionGuards) {
        match self {
            Self::Hot(record) => (record.mode, record.guards),
            Self::Cold(record) => (record.mode, record.guards),
        }
    }

    pub fn updated_at(&self) -> i64 {
        match self {
            Self::Hot(record) => record.updated_at,
            Self::Cold(record) => record.updated_at,
        }
    }

    pub fn hot(&self) -> Option<&SessionRecord> {
        match self {
            Self::Hot(record) => Some(record),
            Self::Cold(_) => None,
        }
    }

    pub fn hot_mut(&mut self) -> Option<&mut SessionRecord> {
        match self {
            Self::Hot(record) => Some(record),
            Self::Cold(_) => None,
        }
    }
}

/// Options for creating a new session. Using a struct keeps the
/// `start_session*` family stable as new session-creation inputs (such as
/// project instructions) are added.
#[derive(Debug, Clone)]
pub struct SessionCreateOptions {
    pub project: Option<String>,
    pub owner_authority_fingerprint: Option<String>,
    pub title: Option<String>,
    pub mode: SessionMode,
    pub guards: SessionGuards,
    pub project_instructions: Option<ProjectInstructionsSnapshot>,
    pub execution_context: SessionExecutionContext,
}

impl SessionCreateOptions {
    pub fn new(
        project: Option<String>,
        title: Option<String>,
        mode: SessionMode,
        guards: SessionGuards,
    ) -> Self {
        Self {
            project,
            owner_authority_fingerprint: None,
            title,
            mode,
            guards,
            project_instructions: None,
            execution_context: SessionExecutionContext::default(),
        }
    }

    pub fn with_project_instructions(
        mut self,
        project_instructions: Option<ProjectInstructionsSnapshot>,
    ) -> Self {
        self.project_instructions = project_instructions;
        self
    }

    pub fn with_owner_authority_fingerprint(
        mut self,
        owner_authority_fingerprint: Option<String>,
    ) -> Self {
        self.owner_authority_fingerprint = owner_authority_fingerprint;
        self
    }

    pub fn with_execution_context(mut self, execution_context: SessionExecutionContext) -> Self {
        self.execution_context = execution_context;
        self
    }
}

/// Atomic create-or-explicit-resume request used by coding workflow startup.
///
/// The Workflow Session, instruction event, and capability transition are
/// committed under one store lock. This is deliberately an internal Workflow
/// Session primitive, not another public task model.
#[derive(Debug, Clone)]
pub struct CodingSessionRequest {
    pub project: String,
    /// Canonical creation-time authority fence for every caller, including the
    /// canonical local/dev authority group.
    pub authority_fingerprint: String,
    pub resume_session_id: Option<String>,
    pub instruction: Option<String>,
    pub mode: SessionMode,
    pub guards: SessionGuards,
    /// `None` preserves an existing Session value during continuation.
    /// `Some({})` explicitly clears all execution defaults.
    pub execution_context: Option<SessionExecutionContext>,
    pub project_instructions: Option<ProjectInstructionsSnapshot>,
    pub transport: SessionTransport,
    pub context_refreshed: bool,
    pub write_scope_verified: bool,
}

#[derive(Debug, Clone)]
pub struct CodingSessionOutcome {
    /// In-memory observation after independent scope retention and stale fencing.
    pub project_instructions: Option<ProjectInstructionsSnapshot>,
    pub summary: SessionSummary,
    /// For a reused/resumed session, a bounded summary taken *before* the new
    /// `task_instruction` was appended. Continuation feedback projects over this
    /// so it describes the previous attempt's work rather than the empty new
    /// attempt. `None` for a freshly created session (no previous attempt).
    pub pre_instruction_summary: Option<SessionSummary>,
    pub reused: bool,
    pub previous_mode: Option<SessionMode>,
    pub previous_guards: Option<SessionGuards>,
    pub capability_changed: bool,
    pub execution_context_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodingSessionError {
    InvalidResumeSessionId,
    UnknownResumeSession {
        session_id: String,
    },
    ResumeSessionNotActive {
        session_id: String,
        lifecycle: SessionLifecycle,
    },
    ResumeProjectMismatch {
        session_id: String,
        session_project: Option<String>,
        request_project: String,
    },
    ResumeAuthorityMismatch {
        session_id: String,
    },
    WriteScopeRequired,
    InvalidExecutionContext(String),
    CommitFailed,
}

#[derive(Debug, Clone)]
pub struct SessionExecutionContextUpdateOutcome {
    pub summary: SessionSummary,
    pub previous_execution_context: SessionExecutionContext,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionExecutionContextUpdateError {
    UnknownSession,
    SessionNotActive { lifecycle: SessionLifecycle },
    SessionHasNoProject,
    InvalidExecutionContext(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStoreStatus {
    pub persistence: String,
    pub restored_sessions: usize,
    /// Backward-compatible alias for `hot_session_capacity_target`. This is not a durable
    /// Session deletion bound.
    pub max_sessions: usize,
    pub retained_sessions: usize,
    pub active_sessions: usize,
    pub closed_sessions: usize,
    pub hot_sessions: usize,
    pub cold_sessions: usize,
    /// Advisory materialized working-set target. Active Sessions may exceed it rather than be
    /// destructively evicted.
    pub hot_session_capacity_target: usize,
    /// Independent bound for retained Closed historical records. Active Sessions do not count
    /// toward this limit.
    pub historical_session_retention_limit: usize,
    /// Process-local count of Closed historical records dropped by the explicit retention policy,
    /// including restore-time pruning.
    pub capacity_evictions: u64,
    pub max_events_per_session: usize,
    pub max_messages_per_session: usize,
    pub last_persist_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedSessionLedger {
    pub version: u32,
    pub sessions: Vec<PersistedSessionSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PersistedSessionSnapshot {
    Hot(PersistedSessionRecord),
    Cold(Arc<RawValue>),
}

impl PersistedSessionSnapshot {
    #[cfg(any(test, feature = "root-test-support"))]
    #[allow(dead_code)]
    pub fn hot(&self) -> Option<&PersistedSessionRecord> {
        match self {
            Self::Hot(record) => Some(record),
            Self::Cold(_) => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedSessionRecord {
    pub session_id: String,
    pub project: Option<String>,
    /// Historical field name retained on disk, but v2 requires the canonical
    /// domain-separated authority-group fingerprint on every persisted row.
    pub owner_authority_fingerprint: String,
    pub title: Option<String>,
    pub mode: SessionMode,
    pub guards: SessionGuards,
    pub execution_context: SessionExecutionContext,
    /// Optional Git baseline tree for coding Sessions. Legacy rows default to
    /// `None` and therefore cannot retroactively gain a Changes card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_baseline_tree: Option<String>,
    /// Sticky first-class repository Edit evidence, durable across event eviction.
    #[serde(default)]
    pub repository_edit_observed: bool,
    pub lifecycle: SessionLifecycle,
    pub created_at: i64,
    pub updated_at: i64,
    pub events: Vec<Arc<SessionEvent>>,
    pub messages: Vec<Arc<SessionMessage>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub message_delivery_replays: BTreeMap<String, SessionMessageDeliveryReplay>,
    pub message_observation_revision: u64,
    pub message_observation_floor: u64,
    pub message_observation_revisions: BTreeMap<String, u64>,
    /// Exact per-todo retention metadata required by the current ledger format.
    pub assignment_history_floors: BTreeMap<String, u64>,
    pub assignment_history_tracking_complete: bool,
    /// Raw assignment fences are never persisted. Only canonical SHA-256
    /// fingerprints are stored.
    pub completion_assignment_fence_fingerprints: BTreeMap<String, String>,
    pub completion_assignment_fence_tracking_complete: bool,
    pub events_observed: u64,
    /// Persistence compatibility only: accepts the retired current-v2 field.
    /// The value is inert, never restored into live Session semantics, and current
    /// writers always omit it.
    #[allow(dead_code)]
    #[serde(default, rename = "context_revision", skip_serializing)]
    pub legacy_context_revision: Option<u64>,
    /// Omit this bounded list when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materialized_validation_job_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct SessionGuards {
    pub deny_write_tools: bool,
    pub deny_shell_tools: bool,
}

impl SessionGuards {
    pub fn effective(mode: SessionMode, guards: Self) -> Self {
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
pub struct SessionGuardDenial {
    pub mode: SessionMode,
    pub guard: &'static str,
}

#[derive(Debug, Clone)]
pub struct ToolCallStart {
    pub event_id: String,
    pub call_id: String,
    /// Trusted correlation for one real kernel request across recorder/business
    /// event pairs. It is accounting metadata only, never execution authority.
    pub logical_invocation_id: Option<String>,
    pub logical_invocation_role: Option<String>,
    pub session_id: String,
    pub transport: SessionTransport,
    pub tool_name: String,
    pub project: Option<String>,
    pub resolved_project: Option<String>,
    pub risk_class: String,
    pub read_like: bool,
    pub write_like: bool,
    pub shell_like: bool,
    pub git_like: bool,
    pub change_summary_like: bool,
    /// Safe boolean metadata: true when this call contributes to
    /// `review_evidence.diff_review_count` (git diff tools, or
    /// `show_changes(include_diff=true)`). Never stores raw input or diffs.
    pub diff_review_like: bool,
    pub changed_paths: Vec<String>,
    /// Validated project-relative paths that the call may establish as
    /// exploration evidence if and only if it finishes successfully.
    pub observed_paths: Vec<String>,
    pub started_at: i64,
    pub started_instant: Instant,
    pub permission: Option<PermissionDecision>,
    pub expectation: ToolCallExpectation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallExpectation {
    pub expected_failure: bool,
    pub expected_failure_kind: Option<String>,
    pub result_expectation: Option<String>,
    pub accepted_exit_codes: Vec<i64>,
    pub assertion_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallSessionMessageResolution {
    pub message_id: String,
    pub resolution: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolCallRecorderMetadata {
    /// Explicit generic wrapper recorder provenance. It is internal metadata,
    /// never concrete tool business input or execution authority.
    pub recording_session_id: Option<String>,
    /// Canonical project of an already-authorized explicit recorder. Populated
    /// only by the kernel after Session authorization; never parsed from public
    /// tool arguments and never persisted as independent authority.
    pub recording_session_project: Option<String>,
    /// True only when the kernel has authorized recording_session_id for the
    /// current caller before dispatch.
    pub recording_session_authorized: bool,
    /// Runtime-only correlation. Public/model arguments never populate these.
    pub logical_invocation_id: Option<String>,
    pub logical_invocation_role: Option<String>,
    pub expectation: ToolCallExpectation,
    pub ack_session_message_ids: Vec<String>,
    pub session_message_resolution: Option<ToolCallSessionMessageResolution>,
}

#[derive(Debug, Clone, Copy)]
pub enum SessionTransport {
    Api,
    Mcp,
}

impl SessionTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Mcp => "mcp",
        }
    }
}

/// Bounded, non-secret persistent-shell evidence retained in the Session
/// ledger. Command text, output, environment, and internal shell state are
/// deliberately excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentShellEventEvidence {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_started: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_completed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_closed: Option<bool>,
}

/// Minimal, bounded effect evidence copied from a structured ToolResult into a
/// finished Session event. It deliberately retains only safe booleans and a
/// closed execution-state atom; raw commands, output, paths, and secrets are
/// never persisted here. Legacy ledger rows deserialize this as `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolEffectEventEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_changed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_started: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_completed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionEvent {
    pub event_id: String,
    /// Additive correlation only: it joins one tool-call start/finish pair and
    /// never participates in authority, retry, outcome, or lifecycle semantics.
    /// Legacy ledger rows omit it and restore as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    /// Additive correlation for one real kernel request. It is generated by the
    /// trusted runtime and is never a retry/idempotency/permission/authority key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_invocation_id: Option<String>,
    /// Closed role discriminator used only to choose canonical semantic evidence
    /// when recorder and business event pairs land in the same Workflow Session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_invocation_role: Option<String>,
    pub session_id: String,
    pub kind: String,
    /// Persistence compatibility only: accepts the retired current-v2 event field.
    /// The value is inert and current writers always omit it.
    #[serde(default, rename = "context_revision", skip_serializing)]
    pub legacy_context_revision: Option<u64>,
    /// Closed, bounded durable consequence evidence for diagnostic recovery. It never stores arbitrary ToolResult bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_result_summary: Option<Value>,
    pub timestamp: i64,
    pub transport: String,
    pub tool_name: String,
    pub project: Option<String>,
    pub resolved_project: Option<String>,
    pub risk_class: String,
    pub read_like: bool,
    pub write_like: bool,
    pub shell_like: bool,
    pub git_like: bool,
    pub change_summary_like: bool,
    /// Safe boolean: git diff tools, or `show_changes` with `include_diff=true`.
    /// Defaults to false for legacy ledger rows that omit the field.
    #[serde(default)]
    pub diff_review_like: bool,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<u64>,
    pub status: Option<String>,
    pub exit_code: Option<i64>,
    pub failure_kind: Option<String>,
    pub error_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_failure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_failure_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_expectation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_exit_codes: Vec<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_failure_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_expectation_result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_project: Option<String>,
    pub error_message_summary: Option<String>,
    pub changed_paths: Vec<String>,
    /// Additive ledger-v1 field. Only validated project-relative paths are
    /// retained; older ledgers deserialize it as an empty workset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observed_paths: Vec<String>,
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_shell: Option<PersistentShellEventEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_evidence: Option<ToolEffectEventEvidence>,
    pub input_summary: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_output_summary: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionDecision>,
    /// Full bounded user instruction for `task_instruction` events. Ordinary
    /// tool-call events leave this unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_guards: Option<SessionGuards>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_guards: Option<SessionGuards>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_changed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_refreshed: Option<bool>,
    /// Safe, strongly typed context supplied by a creation/resume/update call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_context: Option<SessionExecutionContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_execution_context: Option<SessionExecutionContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_context_changed: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessageClosureKind {
    Withdrawn,
    Superseded,
}

fn deserialize_optional_session_message_closure_kind<'de, D>(
    deserializer: D,
) -> Result<Option<SessionMessageClosureKind>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value.as_ref().and_then(Value::as_str) {
        Some("withdrawn") => Some(SessionMessageClosureKind::Withdrawn),
        Some("superseded") => Some(SessionMessageClosureKind::Superseded),
        _ => None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMessage {
    pub message_id: String,
    pub session_id: String,
    pub created_at: i64,
    pub kind: SessionMessageKind,
    pub status: SessionMessageStatus,
    pub priority: SessionMessagePriority,
    pub message: String,
    pub tags: Vec<String>,
    pub reply_to: Option<String>,
    #[serde(default)]
    pub requires_ack: bool,
    #[serde(default)]
    pub first_ack_observed_at: Option<i64>,
    #[serde(default)]
    pub author_session_id: Option<String>,
    pub resolved_at: Option<i64>,
    pub resolution: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_session_message_closure_kind"
    )]
    pub closure_kind: Option<SessionMessageClosureKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_message_id: Option<String>,
    #[serde(default)]
    pub resolved_by_message_id: Option<String>,
    #[serde(default)]
    pub completion_id: Option<String>,
}

/// Maximum direct replies returned by the atomic assignment read. If a retained
/// todo thread exceeds this bound the assignment read fails closed rather than
/// returning an incomplete fence.
pub const MAX_SESSION_ASSIGNMENT_DIRECT_REPLIES: usize = 16;
/// `wsa2_` plus a base64url-no-pad 128-bit scoped semantic snapshot fingerprint.
pub const MAX_SESSION_ASSIGNMENT_FENCE_LEN: usize = 27;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionAssignmentSnapshot {
    pub todo: SessionMessage,
    /// Oldest-first direct replies whose reply_to is exactly the todo id.
    pub direct_replies: Vec<SessionMessage>,
    pub assignment_fence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionAssignmentCurrentState {
    pub todo: SessionMessage,
    pub direct_replies: Vec<SessionMessage>,
    pub direct_replies_truncated: bool,
}

#[derive(Debug, Clone)]
pub struct PostSessionMessageInput {
    pub session_id: String,
    pub kind: SessionMessageKind,
    pub message: String,
    pub tags: Vec<String>,
    pub reply_to: Option<String>,
    pub priority: SessionMessagePriority,
}

#[derive(Debug, Clone)]
pub struct SessionMessageDelivery {
    /// Runtime-derived, domain-separated stable sender-scope digest.
    pub sender_scope: String,
    /// Caller replay key. Persistence retains only its domain-separated digest.
    pub delivery_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMessageDeliveryReplay {
    pub payload_fingerprint: String,
    pub message_id: String,
}

#[derive(Debug, Clone)]
pub struct SessionMessageDeliveryOutcome {
    pub message: SessionMessage,
    pub replayed: bool,
    pub state_changed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SessionAckObservation {
    pub accepted_ids: Vec<String>,
    pub accepted_count: usize,
    pub ignored_count: usize,
    pub first_observed_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SessionAttentionSnapshot {
    pub messages: Vec<SessionMessage>,
    pub total_open_requires_ack: usize,
}

#[derive(Debug, Clone)]
pub struct CompleteSessionMessageInput {
    pub session_id: String,
    pub message_id: String,
    pub answer: String,
    pub tags: Vec<String>,
    pub priority: SessionMessagePriority,
    pub completion_id: String,
    pub author_session_id: Option<String>,
    /// Required opaque semantic snapshot fence returned by get_session_assignment.
    /// It is independent of completion idempotency, observation, and collaboration ACKs.
    pub expected_assignment_fence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompleteSessionMessageOutcome {
    pub todo: SessionMessage,
    pub answer: SessionMessage,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WithdrawSessionMessageOutcome {
    pub message: SessionMessage,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct ReplaceSessionMessageInput {
    pub session_id: String,
    pub message_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplaceSessionMessageOutcome {
    pub original: SessionMessage,
    pub replacement: SessionMessage,
    pub replayed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ListSessionMessagesFilter {
    pub kind: Option<SessionMessageKind>,
    pub status: Option<SessionMessageStatus>,
    pub message_id: Option<String>,
    pub reply_to: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMessageObservationOutcome {
    pub messages: Vec<SessionMessage>,
    pub observation_token: String,
    pub changed: bool,
    pub wait_outcome: &'static str,
    pub waited_ms: u64,
    pub history_lost: bool,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMessageObservationError {
    UnknownSession,
    MalformedToken,
    OversizedToken,
    FutureRevision,
    InvalidObservationState,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMessagesSummary {
    pub total: usize,
    pub open: usize,
    pub resolved: usize,
    pub pending_guidance: usize,
    pub open_questions: usize,
    pub open_risks: usize,
    pub open_todos: usize,
    pub recent_progress: Vec<SessionMessage>,
    pub guidance: usize,
    pub progress: usize,
    pub risk: usize,
    pub todo: usize,
    pub question: usize,
    pub decision: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionDiscussionCounts {
    pub total: usize,
    pub open: usize,
    pub resolved: usize,
    pub guidance: usize,
    pub progress: usize,
    pub risk: usize,
    pub todo: usize,
    pub question: usize,
    pub answer: usize,
    pub decision: usize,
    pub open_guidance: usize,
    pub open_questions: usize,
    pub open_risks: usize,
    pub open_todos: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMessageCompletionSummary {
    pub todo_message_id: String,
    pub answer_message_id: String,
    pub author_session_id: Option<String>,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionDiscussionSummary {
    pub counts: SessionDiscussionCounts,
    pub open_guidance: Vec<SessionMessage>,
    pub open_questions: Vec<SessionMessage>,
    pub open_risks: Vec<SessionMessage>,
    pub open_todos: Vec<SessionMessage>,
    pub high_priority_open_todos: Vec<SessionMessage>,
    pub recent_answers: Vec<SessionMessage>,
    pub recent_completions: Vec<SessionMessageCompletionSummary>,
    pub recent_progress: Vec<SessionMessage>,
    pub recent_decisions: Vec<SessionMessage>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionInboxOpenCounts {
    pub guidance: usize,
    pub question: usize,
    pub todo: usize,
    pub risk: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInboxHint {
    pub has_open_messages: bool,
    pub open_counts: SessionInboxOpenCounts,
    pub highest_priority: SessionMessagePriority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention_reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention_instruction: Option<&'static str>,
    pub suggested_next_tool: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionMessageError {
    UnknownSession,
    UnknownMessage,
    MessageNotOpen,
    NotTodo,
    IdempotencyConflict,
    DeliveryKeyConflict,
    DeliveryPersistenceUncertain,
    AlreadyCompleted {
        answer_message_id: Option<String>,
        completion_id: Option<String>,
    },
    InvalidCompletionState,
    InvalidObservationState,
    InvalidAssignmentFence,
    AssignmentStale {
        current: SessionAssignmentCurrentState,
        fresh_assignment_fence: Option<String>,
    },
    AssignmentHistoryLost {
        current: SessionAssignmentCurrentState,
    },
    AssignmentTooLarge {
        reply_count: usize,
        max_replies: usize,
        current: SessionAssignmentCurrentState,
    },
    PersistenceUncertain,
    /// Message-board mutation denied because the workflow session is closed.
    /// Query tools remain available.
    SessionClosed {
        lifecycle: SessionLifecycle,
    },
    InvalidInput(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionCounts {
    pub tool_calls: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub read_like: usize,
    pub write_like: usize,
    pub shell_like: usize,
    pub git_like: usize,
    pub change_summary_like: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub project: Option<String>,
    pub title: Option<String>,
    pub mode: SessionMode,
    pub guards: SessionGuards,
    pub execution_context: SessionExecutionContext,
    pub lifecycle: SessionLifecycle,
    /// Internal Final Changes presentation baseline. Runtime projections read it
    /// directly; it is not part of the model-facing Session summary payload.
    #[serde(skip_serializing)]
    pub git_baseline_tree: Option<String>,
    /// Internal sticky presentation eligibility fact; never model-facing state.
    #[serde(skip_serializing)]
    pub repository_edit_observed: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub counts: SessionCounts,
    pub events: Vec<SessionEvent>,
    /// Total events ever observed for this Session, including events already
    /// evicted by the bounded durable ledger.
    #[serde(default)]
    pub events_total: usize,
    /// Events currently retained in the durable ledger before model-facing tail
    /// slicing is applied.
    #[serde(default)]
    pub events_retained: usize,
    /// Events already evicted by the durable per-Session retention bound.
    #[serde(default)]
    pub events_evicted: usize,
    /// True only when durable Session history has actually been evicted. This is
    /// distinct from `events_truncated`, which may be true solely because one
    /// model-facing summary returns at most `MAX_SUMMARY_LIMIT` events.
    #[serde(default)]
    pub retention_truncated: bool,
    /// 0-based sequence of the first event still present in the durable ledger.
    #[serde(default)]
    pub ledger_first_retained_sequence: usize,
    /// Number of events actually returned in `events` (the bounded tail).
    #[serde(default)]
    pub events_returned: usize,
    /// True when the Session has more observed events than this response returns.
    /// This covers both ordinary model-facing tail slicing and durable eviction;
    /// use `retention_truncated` to distinguish actual history loss.
    #[serde(default)]
    pub events_truncated: bool,
    /// 0-based absolute sequence of the first event returned in `events`.
    /// The legacy field name is retained for compatibility with continuation
    /// consumers that already interpret it as the returned-window base.
    #[serde(default)]
    pub first_retained_sequence: usize,
    pub messages: SessionMessagesSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_instructions: Option<ProjectInstructionsSummarySnapshot>,
}

//! Session data model: IDs, records, events, messages, and summary types.
use super::super::permissions::PermissionDecision;
use super::super::project_instructions::{
    ProjectInstructionsSnapshot, ProjectInstructionsSummarySnapshot,
};
use super::super::tool_inputs::{ExecutionShell, SessionMode};
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

pub(crate) const SESSION_ID_PREFIX: &str = "wc_sess_";
pub(super) const EVENT_ID_PREFIX: &str = "evt_";
pub(super) const CALL_ID_PREFIX: &str = "wc_call_";
pub(crate) const DEFAULT_MAX_SESSIONS: usize = 100;
pub(crate) const DEFAULT_MAX_EVENTS_PER_SESSION: usize = 200;
/// Exact terminal-validation Job identities retained per Workflow Session. This
/// matches the Runner's authoritative terminal Job inventory bound: while a
/// terminal Job can still be a reconciliation candidate, one of these bounded
/// identities can represent it without turning the Session ledger into an
/// unbounded Job-id history.
pub(super) const MAX_MATERIALIZED_VALIDATION_JOB_IDS: usize =
    crate::shell_protocol::JOB_INVENTORY_MAX_TERMINAL_JOBS;
/// Maximum project-relative exploration paths retained on one ledger event.
/// This covers the largest currently supported structured search/LSP result
/// while keeping every event independently bounded.
pub(crate) const MAX_OBSERVED_PATHS_PER_EVENT: usize = 201;
pub(super) const DEFAULT_SUMMARY_LIMIT: usize = 50;
pub(super) const MAX_SUMMARY_LIMIT: usize = 200;
pub(super) const MAX_SUMMARY_STRING_CHARS: usize = 240;
pub(super) const MAX_INPUT_STRING_CHARS: usize = 120;
pub(super) const MAX_INPUT_OBJECT_KEYS: usize = 16;
pub(super) const MAX_INPUT_ARRAY_ITEMS: usize = 8;
pub(crate) const MAX_VALIDATION_EXCERPT_CHARS: usize = 800;
pub(super) const SESSION_LEDGER_VERSION: u32 = 1;
pub(super) const DURABLE_CURRENT_BINDINGS_PER_SESSION: usize = 8;
pub(crate) const MESSAGE_ID_PREFIX: &str = "wc_msg_";
pub(crate) const DEFAULT_MAX_MESSAGES_PER_SESSION: usize = 200;
pub(crate) const MAX_CODING_INSTRUCTION_CHARS: usize = 4000;
pub(crate) const DEFAULT_MESSAGE_LIST_LIMIT: usize = 50;
pub(crate) const MAX_MESSAGE_LIST_LIMIT: usize = 100;
pub(crate) const MAX_SESSION_MESSAGE_OBSERVATION_TOKEN_LEN: usize = 192;
pub(crate) const MAX_MESSAGE_CHARS: usize = 8000;
pub(crate) const MAX_MESSAGE_TAGS: usize = 16;
pub(crate) const MAX_MESSAGE_TAG_CHARS: usize = 64;
pub(crate) const MAX_MESSAGE_RESOLUTION_CHARS: usize = 8000;
pub(crate) const MAX_MESSAGE_COMPLETION_KEY_CHARS: usize = 128;
pub(crate) const MESSAGE_COMPLETION_FINGERPRINT_HEX_CHARS: usize = 64;
pub(super) const MAX_MESSAGE_SUMMARY_CHARS: usize = 240;
pub(super) const SUMMARY_MESSAGE_GROUP_LIMIT: usize = 5;
pub(crate) const TOOL_EXPECTATION_RESULT_NONE: &str = "none";
pub(crate) const TOOL_EXPECTATION_RESULT_MATCHED: &str = "matched_expected_failure";
pub(crate) const TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE: &str = "unexpected_failure";
pub(crate) const TOOL_EXPECTATION_RESULT_MISMATCH: &str = "expectation_mismatch";
pub(crate) const TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS: &str = "unexpected_success";
pub(crate) const TOOL_CALL_RECORDING_SESSION_ID_FIELD: &str = "recording_session_id";
pub(crate) const TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD: &str = "ack_session_message_ids";
pub(crate) const TOOL_CALL_ACK_SESSION_MESSAGE_IDS_INTERNAL_FIELD: &str =
    "__webcodex_stateless_ack_session_message_ids";
pub(crate) const TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_FIELD: &str =
    "ack_session_context_revision";
pub(crate) const TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD: &str =
    "__webcodex_stateless_ack_session_context_revision";
pub(crate) const MAX_TOOL_CALL_ACK_MESSAGE_IDS: usize = 8;
pub(crate) const TOOL_EXPECTED_FAILURE_FIELD: &str = "expected_failure";
pub(crate) const TOOL_EXPECTED_FAILURE_KIND_FIELD: &str = "expected_failure_kind";
pub(crate) const TOOL_ASSERTION_NAME_FIELD: &str = "assertion_name";
pub(crate) const TOOL_CALL_EXPECTATION_METADATA_FIELDS: &[&str] = &[
    TOOL_EXPECTED_FAILURE_FIELD,
    TOOL_EXPECTED_FAILURE_KIND_FIELD,
    TOOL_ASSERTION_NAME_FIELD,
];

/// Durable execution defaults inherited by a closed set of execution tools
/// attached to a project-scoped Workflow Session.
///
/// This intentionally contains no environment, credential, connection, or
/// arbitrary option bag. `resource` is only a named Runner-local SSH resource;
/// it never stores an SSH host, config, key, password, or transport.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionExecutionContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) default_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) default_shell: Option<ExecutionShell>,
    /// Optional named SSH resource on the Runner that owns this Session's
    /// project. It changes `run_shell`, `run_job`, and newly opened
    /// `open_session_shell` execution location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resource: Option<String>,
}

impl SessionExecutionContext {
    pub(crate) fn is_empty(&self) -> bool {
        self.default_cwd.is_none() && self.default_shell.is_none() && self.resource.is_none()
    }

    /// Validate and normalize persisted execution-context fields.
    ///
    /// Without an SSH resource, `default_cwd` remains project-relative and
    /// follows the existing project-bound validation. With one, it is a remote
    /// path instead and never reaches Runner-local project path validation.
    pub(crate) fn validated(mut self) -> Result<Self, String> {
        if let Some(raw_resource) = self.resource.take() {
            let resource = raw_resource.trim();
            if resource.is_empty()
                || resource.len() > 80
                || resource.contains("..")
                || !resource
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
            {
                return Err(
                    "execution_context.resource must be a safe named SSH resource".to_string(),
                );
            }
            self.resource = Some(resource.to_string());
        }
        if let Some(raw_cwd) = self.default_cwd.take() {
            let cwd = raw_cwd.trim();
            if self.resource.is_some() {
                if cwd.is_empty() || cwd.len() > 4096 || cwd.chars().any(char::is_control) {
                    return Err(
                        "execution_context.default_cwd must be a bounded remote path without control characters"
                            .to_string(),
                    );
                }
                self.default_cwd = Some(cwd.to_string());
            } else {
                crate::validation_bridge::validate_project_relative_path(cwd)
                    .map_err(|error| format!("execution_context.default_cwd {error}"))?;
                let normalized = cwd
                    .split(['/', '\\'])
                    .filter(|component| !component.is_empty() && *component != ".")
                    .collect::<Vec<_>>()
                    .join("/");
                self.default_cwd = Some(if normalized.is_empty() {
                    ".".to_string()
                } else {
                    normalized
                });
            }
        }
        Ok(self)
    }

    /// Restore valid fields independently so a malformed persisted cwd cannot
    /// bypass the project boundary or erase a valid explicit shell choice.
    pub(super) fn sanitized_for_restore(mut self) -> Self {
        self.resource = self.resource.take().and_then(|raw_resource| {
            Self {
                default_cwd: None,
                default_shell: None,
                resource: Some(raw_resource),
            }
            .validated()
            .ok()
            .and_then(|context| context.resource)
        });
        if let Some(raw_cwd) = self.default_cwd.take() {
            let cwd_only = Self {
                default_cwd: Some(raw_cwd),
                default_shell: None,
                resource: self.resource.clone(),
            };
            self.default_cwd = cwd_only
                .validated()
                .ok()
                .and_then(|context| context.default_cwd);
        }
        self
    }

    /// Audit-safe form for pre-validation request logging. Invalid cwd text is
    /// represented only by booleans and never copied into evidence.
    pub(crate) fn audit_summary(&self) -> Value {
        let resource = Self {
            default_cwd: None,
            default_shell: None,
            resource: self.resource.clone(),
        }
        .validated()
        .ok()
        .and_then(|context| context.resource);
        let cwd = Self {
            default_cwd: self.default_cwd.clone(),
            default_shell: None,
            resource: resource.clone(),
        }
        .validated()
        .ok()
        .and_then(|context| context.default_cwd);
        serde_json::json!({
            "default_cwd": cwd,
            "default_cwd_present": self.default_cwd.is_some(),
            "default_cwd_valid": self.default_cwd.is_none() || cwd.is_some(),
            "default_shell": self.default_shell,
            "resource": resource,
            "resource_present": self.resource.is_some(),
            "resource_valid": self.resource.is_none() || resource.is_some(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CurrentSessionKey {
    pub(crate) principal_kind: String,
    pub(crate) principal_id: String,
    pub(crate) transport: String,
    pub(crate) window_key: String,
    pub(crate) resolved_project: String,
    /// Domain-separated hash of the agent-reported canonical repository root.
    /// A project registration that moves to another root must not inherit the
    /// old root's current-session binding.
    pub(crate) repository_root_key: String,
}

impl CurrentSessionKey {
    /// Return the only form of the exact current-session key that may enter the
    /// durable Workflow Session ledger.
    ///
    /// Components use a fixed order and u64 length prefixes so distinct tuples
    /// cannot collide through separator ambiguity. The principal id, transport
    /// window key, resolved project, and repository-root key remain available
    /// only to the process-local cache.
    pub(super) fn durable_binding_key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"webcodex.workflow-current-binding.v1\0");
        for component in [
            self.principal_kind.as_str(),
            self.principal_id.as_str(),
            self.transport.as_str(),
            self.window_key.as_str(),
            self.resolved_project.as_str(),
            self.repository_root_key.as_str(),
        ] {
            hasher.update((component.len() as u64).to_be_bytes());
            hasher.update(component.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
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
/// - `Archived` is a reserved wire state and is never produced by the store.
///
/// LRU eviction remains capacity management, not a lifecycle transition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionLifecycle {
    #[default]
    Active,
    /// Explicitly closed; query remains allowed, mutations are denied.
    Closed,
    /// Reserved wire state. Not produced; treated like Closed for denial.
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
    /// Domain-separated SHA-256 of the canonical creation-time authority group.
    /// The historical field name is retained for ledger compatibility; the raw
    /// authority identity is never stored. Restore may use a private noncanonical
    /// marker for a malformed persisted value so it remains distinguishable from
    /// a genuinely absent legacy field and permanently fails authorization.
    pub(super) owner_authority_fingerprint: Option<String>,
    pub(super) title: Option<String>,
    pub(super) mode: SessionMode,
    pub(super) guards: SessionGuards,
    pub(super) execution_context: SessionExecutionContext,
    /// Explicit lifecycle; always set in memory. Default on load: Active.
    pub(super) lifecycle: SessionLifecycle,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
    pub(super) events: VecDeque<Arc<SessionEvent>>,
    /// Cumulative number of events ever appended to this session ledger,
    /// including events the per-session event cap has since evicted. This is
    /// the source of truth for "did the durable ledger ever hold more events
    /// than are retained now". The persisted counterpart carries the additive
    /// serde default; the in-memory record is always constructed explicitly.
    pub(super) events_observed: u64,
    /// Durable Session-local model-facing continuity watermark. This advances
    /// exactly once for each recorded ToolResult returned to the model;
    /// generic/background Session events never advance it.
    pub(super) context_revision: u64,
    /// Bounded durable exact identities for terminal structured-validation Jobs
    /// already synthesized into this Session. Independent of the retained event
    /// deque so event FIFO eviction cannot resurrect an authoritative Job.
    pub(super) materialized_validation_job_ids: VecDeque<String>,
    pub(super) messages: VecDeque<Arc<SessionMessage>>,
    /// Durable Session-local monotonic message-state revision. This is never
    /// exposed as a public cursor; callers receive an opaque Session-bound token.
    pub(super) message_observation_revision: u64,
    /// Highest last-message revision no longer recoverable because that message
    /// was evicted or sanitized from retained state.
    pub(super) message_observation_floor: u64,
    /// Last observable mutation revision for each currently retained message.
    pub(super) message_observation_revisions: BTreeMap<String, u64>,
    pub(super) project_instructions: Option<ProjectInstructionsSnapshot>,
}

/// Internal residency state is deliberately orthogonal to business lifecycle.
/// Active sessions stay hot; historical closed sessions may keep only one
/// compact, immutable durable JSON object plus the small metadata required for
/// lifecycle/authorization checks and LRU bookkeeping.
#[derive(Debug)]
pub(super) enum StoredSession {
    Hot(SessionRecord),
    Cold(ColdSessionRecord),
}

#[derive(Debug, Clone)]
pub(super) struct ColdSessionRecord {
    pub(super) session_id: String,
    pub(super) project: Option<String>,
    pub(super) owner_authority_fingerprint: Option<String>,
    pub(super) mode: SessionMode,
    pub(super) guards: SessionGuards,
    pub(super) lifecycle: SessionLifecycle,
    pub(super) updated_at: i64,
    pub(super) project_instructions: Option<ProjectInstructionsSummarySnapshot>,
    pub(super) context_revision: u64,
    pub(super) raw: Arc<RawValue>,
}

impl StoredSession {
    pub(super) fn session_id(&self) -> &str {
        match self {
            Self::Hot(record) => &record.session_id,
            Self::Cold(record) => &record.session_id,
        }
    }

    pub(super) fn project(&self) -> Option<&str> {
        match self {
            Self::Hot(record) => record.project.as_deref(),
            Self::Cold(record) => record.project.as_deref(),
        }
    }

    pub(super) fn owner_authority_fingerprint(&self) -> Option<&str> {
        match self {
            Self::Hot(record) => record.owner_authority_fingerprint.as_deref(),
            Self::Cold(record) => record.owner_authority_fingerprint.as_deref(),
        }
    }

    pub(super) fn lifecycle(&self) -> SessionLifecycle {
        match self {
            Self::Hot(record) => record.lifecycle,
            Self::Cold(record) => record.lifecycle,
        }
    }

    pub(super) fn mode_guards(&self) -> (SessionMode, SessionGuards) {
        match self {
            Self::Hot(record) => (record.mode, record.guards),
            Self::Cold(record) => (record.mode, record.guards),
        }
    }

    pub(super) fn updated_at(&self) -> i64 {
        match self {
            Self::Hot(record) => record.updated_at,
            Self::Cold(record) => record.updated_at,
        }
    }

    pub(super) fn context_revision(&self) -> u64 {
        match self {
            Self::Hot(record) => record.context_revision,
            Self::Cold(record) => record.context_revision,
        }
    }

    pub(super) fn hot(&self) -> Option<&SessionRecord> {
        match self {
            Self::Hot(record) => Some(record),
            Self::Cold(_) => None,
        }
    }

    pub(super) fn hot_mut(&mut self) -> Option<&mut SessionRecord> {
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
pub(crate) struct SessionCreateOptions {
    pub(crate) project: Option<String>,
    pub(crate) owner_authority_fingerprint: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) mode: SessionMode,
    pub(crate) guards: SessionGuards,
    pub(crate) project_instructions: Option<ProjectInstructionsSnapshot>,
    pub(crate) execution_context: SessionExecutionContext,
}

impl SessionCreateOptions {
    pub(crate) fn new(
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

    pub(crate) fn with_project_instructions(
        mut self,
        project_instructions: Option<ProjectInstructionsSnapshot>,
    ) -> Self {
        self.project_instructions = project_instructions;
        self
    }

    pub(crate) fn with_owner_authority_fingerprint(
        mut self,
        owner_authority_fingerprint: Option<String>,
    ) -> Self {
        self.owner_authority_fingerprint = owner_authority_fingerprint;
        self
    }

    pub(crate) fn with_execution_context(
        mut self,
        execution_context: SessionExecutionContext,
    ) -> Self {
        self.execution_context = execution_context;
        self
    }
}

/// Atomic start-or-continue request used by `start_coding_task`.
///
/// The workflow session, instruction event, capability transition, and
/// process-local/durable exact current binding are committed under one store
/// lock. This is deliberately an internal Workflow Session primitive, not
/// another public task model.
#[derive(Debug, Clone)]
pub(crate) struct CodingSessionRequest {
    pub(crate) key: Option<CurrentSessionKey>,
    pub(crate) project: String,
    /// Canonical creation-time authority fence for authenticated callers.
    /// `None` is reserved for the trusted local/dev path.
    pub(crate) authority_fingerprint: Option<String>,
    pub(crate) resume_session_id: Option<String>,
    pub(crate) instruction: Option<String>,
    pub(crate) mode: SessionMode,
    pub(crate) guards: SessionGuards,
    /// `None` preserves an existing Session value during continuation.
    /// `Some({})` explicitly clears all execution defaults.
    pub(crate) execution_context: Option<SessionExecutionContext>,
    pub(crate) project_instructions: Option<ProjectInstructionsSnapshot>,
    pub(crate) transport: SessionTransport,
    pub(crate) bind_current: bool,
    pub(crate) new_session: bool,
    pub(crate) context_refreshed: bool,
    pub(crate) write_scope_verified: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CodingSessionOutcome {
    pub(crate) summary: SessionSummary,
    /// For a reused/resumed session, a bounded summary taken *before* the new
    /// `task_instruction` was appended. Continuation feedback projects over this
    /// so it describes the previous attempt's work rather than the empty new
    /// attempt. `None` for a freshly created session (no previous attempt).
    pub(crate) pre_instruction_summary: Option<SessionSummary>,
    pub(crate) reused: bool,
    pub(crate) previous_mode: Option<SessionMode>,
    pub(crate) previous_guards: Option<SessionGuards>,
    pub(crate) capability_changed: bool,
    pub(crate) execution_context_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodingSessionError {
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
    LegacySessionAuthorityUnverifiable {
        session_id: String,
    },
    ResumeNewSessionConflict,
    WriteScopeRequired,
    InvalidExecutionContext(String),
    CommitFailed,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionExecutionContextUpdateOutcome {
    pub(crate) summary: SessionSummary,
    pub(crate) previous_execution_context: SessionExecutionContext,
    pub(crate) changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionExecutionContextUpdateError {
    UnknownSession,
    SessionNotActive { lifecycle: SessionLifecycle },
    SessionHasNoProject,
    InvalidExecutionContext(String),
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionStoreStatus {
    pub(crate) persistence: String,
    pub(crate) restored_sessions: usize,
    pub(crate) durable_binding_count: usize,
    pub(crate) restored_binding_count: usize,
    pub(crate) discarded_binding_count: usize,
    pub(crate) max_durable_bindings: usize,
    pub(crate) max_sessions: usize,
    pub(crate) max_events_per_session: usize,
    pub(crate) max_messages_per_session: usize,
    pub(crate) last_persist_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PersistedSessionLedger {
    pub(super) version: u32,
    pub(super) sessions: Vec<PersistedSessionSnapshot>,
    /// Additive v1 field. Old ledgers omit it and deserialize to an empty map.
    /// The lossy wrapper prevents one malformed binding entry from rejecting
    /// otherwise valid Workflow Session records and events.
    #[serde(default)]
    pub(super) durable_current_bindings: PersistedCurrentBindings,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum PersistedSessionSnapshot {
    Hot(PersistedSessionRecord),
    Cold(Arc<RawValue>),
}

impl PersistedSessionSnapshot {
    #[cfg(test)]
    pub(super) fn hot(&self) -> Option<&PersistedSessionRecord> {
        match self {
            Self::Hot(record) => Some(record),
            Self::Cold(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedCurrentBinding {
    pub(super) binding_key_sha256: String,
    pub(super) session_id: String,
    pub(super) updated_at: i64,
}

#[derive(Debug, Default)]
pub(super) struct PersistedCurrentBindings {
    pub(super) records: Vec<PersistedCurrentBinding>,
    pub(super) malformed_count: usize,
}

impl Serialize for PersistedCurrentBindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.records.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PersistedCurrentBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let Value::Array(values) = value else {
            return Ok(Self {
                records: Vec::new(),
                malformed_count: usize::from(!value.is_null()),
            });
        };
        let mut records = Vec::with_capacity(values.len());
        let mut malformed_count = 0usize;
        for value in values {
            match serde_json::from_value(value) {
                Ok(record) => records.push(record),
                Err(_) => malformed_count = malformed_count.saturating_add(1),
            }
        }
        Ok(Self {
            records,
            malformed_count,
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct DurableCurrentBinding {
    pub(super) session_id: String,
    pub(super) updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PersistedSessionRecord {
    pub(super) session_id: String,
    pub(super) project: Option<String>,
    /// Additive v1 field. c3a09275 used it only for project-less ownership;
    /// current writers store the canonical authority-group fingerprint for every
    /// authenticated Session. The historical name keeps ledger compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) owner_authority_fingerprint: Option<String>,
    pub(super) title: Option<String>,
    pub(super) mode: SessionMode,
    pub(super) guards: SessionGuards,
    /// Additive ledger-v1 field. Older ledgers restore an empty context.
    #[serde(default)]
    pub(super) execution_context: SessionExecutionContext,
    /// Optional on disk for ledger compatibility; missing → Active.
    #[serde(default)]
    pub(super) lifecycle: SessionLifecycle,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
    pub(super) events: Vec<Arc<SessionEvent>>,
    pub(super) messages: Vec<Arc<SessionMessage>>,
    #[serde(default)]
    pub(super) message_observation_revision: u64,
    #[serde(default)]
    pub(super) message_observation_floor: u64,
    #[serde(default)]
    pub(super) message_observation_revisions: BTreeMap<String, u64>,
    /// Additive v1 field. Cumulative number of events ever appended, including
    /// those the per-session event cap has evicted. Older ledgers omit it and
    /// deserialize to 0; the restore path treats 0 as "retain exactly the
    /// persisted events" for legacy compatibility.
    #[serde(default)]
    pub(super) events_observed: u64,
    /// Additive ledger-v1 model-facing continuity watermark. Legacy rows
    /// deserialize to zero and begin issuing revisions only after upgrade.
    #[serde(default)]
    pub(super) context_revision: u64,
    /// Additive ledger-v1 field. Exact identities are sanitized and bounded on
    /// restore; old ledgers deserialize to an empty set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) materialized_validation_job_ids: Vec<String>,
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
            SessionMode::Inspect => Self {
                deny_write_tools: true,
                deny_shell_tools: false,
            },
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
    pub(crate) call_id: String,
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
    /// Validated project-relative paths that the call may establish as
    /// exploration evidence if and only if it finishes successfully.
    pub(crate) observed_paths: Vec<String>,
    pub(crate) started_at: i64,
    pub(crate) started_instant: Instant,
    pub(crate) permission: Option<PermissionDecision>,
    pub(crate) expectation: ToolCallExpectation,
    pub(crate) pre_call_context_revision: u64,
    pub(crate) ack_session_context_revision: SessionContextRevisionAck,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ToolCallExpectation {
    pub(crate) expected_failure: bool,
    pub(crate) expected_failure_kind: Option<String>,
    pub(crate) assertion_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ToolCallRecorderMetadata {
    /// Explicit generic wrapper recorder provenance. It is internal metadata,
    /// never concrete tool business input or execution authority.
    pub(crate) recording_session_id: Option<String>,
    pub(crate) expectation: ToolCallExpectation,
    pub(crate) ack_session_message_ids: Vec<String>,
    pub(crate) ack_session_context_revision: SessionContextRevisionAck,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum SessionContextRevisionAck {
    /// The current request surface does not expose the context-continuity ACK
    /// protocol. Model-facing history still advances its durable watermark,
    /// but the response must preserve the legacy surface contract.
    #[default]
    Unsupported,
    Unacknowledged,
    Revision(u64),
    Invalid,
}

#[derive(Debug, Clone)]
pub(crate) struct RecordedModelFacingToolCall {
    pub(crate) event_id: String,
    pub(crate) session_id: String,
    pub(crate) context_revision: u64,
    pub(crate) pre_call_context_revision: u64,
    pub(crate) ack_session_context_revision: SessionContextRevisionAck,
    /// Retained model-facing results strictly after the caller's proven view and
    /// at or before the pre-call watermark. The current ToolResult is excluded.
    pub(crate) recovery_events: Vec<SessionEvent>,
    pub(crate) history_lost: bool,
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

/// Bounded, non-secret persistent-shell evidence retained in the Session
/// ledger. Command text, output, environment, and internal shell state are
/// deliberately excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersistentShellEventEvidence {
    pub(crate) action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) shell_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) shell_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) execution_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) command_started: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) command_completed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) already_closed: Option<bool>,
}

/// Minimal, bounded effect evidence copied from a structured ToolResult into a
/// finished Session event. It deliberately retains only safe booleans and a
/// closed execution-state atom; raw commands, output, paths, and secrets are
/// never persisted here. Legacy ledger rows deserialize this as `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolEffectEventEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) state_changed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) command_started: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) command_completed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) execution_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionEvent {
    pub(crate) event_id: String,
    /// Additive correlation only: it joins one tool-call start/finish pair and
    /// never participates in authority, retry, outcome, or lifecycle semantics.
    /// Legacy ledger rows omit it and restore as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) call_id: Option<String>,
    pub(crate) session_id: String,
    pub(crate) kind: String,
    /// Model-facing context revision assigned atomically with a finished
    /// ToolResult. Started/background/system events leave this unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) context_revision: Option<u64>,
    /// Closed, bounded consequence projection used only for model-context
    /// recovery. It never stores arbitrary ToolResult bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) context_result_summary: Option<Value>,
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
    /// Additive ledger-v1 field. Only validated project-relative paths are
    /// retained; older ledgers deserialize it as an empty workset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) observed_paths: Vec<String>,
    pub(crate) job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) persistent_shell: Option<PersistentShellEventEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) effect_evidence: Option<ToolEffectEventEvidence>,
    pub(crate) input_summary: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) validation_output_summary: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) permission: Option<PermissionDecision>,
    /// Full bounded user instruction for `task_instruction` events. Ordinary
    /// tool-call events leave this unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) instruction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) requested_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) previous_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) requested_guards: Option<SessionGuards>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) previous_guards: Option<SessionGuards>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) capability_changed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) context_refreshed: Option<bool>,
    /// Safe, strongly typed context supplied by a creation/resume/update call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) execution_context: Option<SessionExecutionContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) previous_execution_context: Option<SessionExecutionContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) execution_context_changed: Option<bool>,
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

impl SessionMessageKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Proposal => "proposal",
            Self::Question => "question",
            Self::Answer => "answer",
            Self::Decision => "decision",
            Self::Risk => "risk",
            Self::Progress => "progress",
            Self::Guidance => "guidance",
            Self::Todo => "todo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionMessageStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionMessageClosureKind {
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionMessagePriority {
    Low,
    #[default]
    Normal,
    High,
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
    #[serde(default)]
    pub(crate) requires_ack: bool,
    #[serde(default)]
    pub(crate) first_ack_observed_at: Option<i64>,
    #[serde(default)]
    pub(crate) author_session_id: Option<String>,
    pub(crate) resolved_at: Option<i64>,
    pub(crate) resolution: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_session_message_closure_kind"
    )]
    pub(crate) closure_kind: Option<SessionMessageClosureKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) superseded_by_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) supersedes_message_id: Option<String>,
    #[serde(default)]
    pub(crate) resolved_by_message_id: Option<String>,
    #[serde(default)]
    pub(crate) completion_id: Option<String>,
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

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionAckObservation {
    pub(crate) accepted_ids: Vec<String>,
    pub(crate) accepted_count: usize,
    pub(crate) ignored_count: usize,
    pub(crate) first_observed_count: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionAttentionSnapshot {
    pub(crate) messages: Vec<SessionMessage>,
    pub(crate) total_open_requires_ack: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct CompleteSessionMessageInput {
    pub(crate) session_id: String,
    pub(crate) message_id: String,
    pub(crate) answer: String,
    pub(crate) tags: Vec<String>,
    pub(crate) priority: SessionMessagePriority,
    pub(crate) completion_id: String,
    pub(crate) author_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CompleteSessionMessageOutcome {
    pub(crate) todo: SessionMessage,
    pub(crate) answer: SessionMessage,
    pub(crate) replayed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WithdrawSessionMessageOutcome {
    pub(crate) message: SessionMessage,
    pub(crate) replayed: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ReplaceSessionMessageInput {
    pub(crate) session_id: String,
    pub(crate) message_id: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplaceSessionMessageOutcome {
    pub(crate) original: SessionMessage,
    pub(crate) replacement: SessionMessage,
    pub(crate) replayed: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ListSessionMessagesFilter {
    pub(crate) kind: Option<SessionMessageKind>,
    pub(crate) status: Option<SessionMessageStatus>,
    pub(crate) message_id: Option<String>,
    pub(crate) reply_to: Option<String>,
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionMessageObservationOutcome {
    pub(crate) messages: Vec<SessionMessage>,
    pub(crate) observation_token: String,
    pub(crate) changed: bool,
    pub(crate) wait_outcome: &'static str,
    pub(crate) waited_ms: u64,
    pub(crate) history_lost: bool,
    pub(crate) has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionMessageObservationError {
    UnknownSession,
    MalformedToken,
    OversizedToken,
    WrongSession,
    FutureRevision,
    InvalidObservationState,
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
    pub(crate) answer: usize,
    pub(crate) decision: usize,
    pub(crate) open_guidance: usize,
    pub(crate) open_questions: usize,
    pub(crate) open_risks: usize,
    pub(crate) open_todos: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionMessageCompletionSummary {
    pub(crate) todo_message_id: String,
    pub(crate) answer_message_id: String,
    pub(crate) author_session_id: Option<String>,
    pub(crate) completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionDiscussionSummary {
    pub(crate) counts: SessionDiscussionCounts,
    pub(crate) open_guidance: Vec<SessionMessage>,
    pub(crate) open_questions: Vec<SessionMessage>,
    pub(crate) open_risks: Vec<SessionMessage>,
    pub(crate) open_todos: Vec<SessionMessage>,
    pub(crate) high_priority_open_todos: Vec<SessionMessage>,
    pub(crate) recent_answers: Vec<SessionMessage>,
    pub(crate) recent_completions: Vec<SessionMessageCompletionSummary>,
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

pub(crate) const SESSION_INBOX_HIGH_GUIDANCE_ATTENTION_REASON: &str =
    "high_priority_guidance_requires_ack";
pub(crate) const SESSION_INBOX_HIGH_GUIDANCE_ATTENTION_INSTRUCTION: &str =
    "High-priority Session guidance is pending. Read session_discussion_summary before continuing.";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionInboxHint {
    pub(crate) has_open_messages: bool,
    pub(crate) open_counts: SessionInboxOpenCounts,
    pub(crate) highest_priority: SessionMessagePriority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) attention_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) attention_reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) attention_instruction: Option<&'static str>,
    pub(crate) suggested_next_tool: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionMessageError {
    UnknownSession,
    UnknownMessage,
    MessageNotOpen,
    NotTodo,
    IdempotencyConflict,
    AlreadyCompleted {
        answer_message_id: Option<String>,
        completion_id: Option<String>,
    },
    InvalidCompletionState,
    InvalidObservationState,
    PersistenceUncertain,
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
    pub(crate) execution_context: SessionExecutionContext,
    pub(crate) lifecycle: SessionLifecycle,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) counts: SessionCounts,
    pub(crate) events: Vec<SessionEvent>,
    /// Total number of events retained in the durable ledger for the session
    /// *before* the returned window was sliced. This is the source of truth for
    /// whether older events (e.g. an attempt boundary `task_instruction`) were
    /// evicted by the per-session event cap. Older persisted sessions that predate
    /// these additive fields deserialize to 0/0/true and are treated as the
    /// returned window being the whole retained ledger (no eviction observed).
    #[serde(default)]
    pub(crate) events_total: usize,
    /// Number of events actually returned in `events` (the retained tail).
    #[serde(default)]
    pub(crate) events_returned: usize,
    /// True when the durable ledger retained more events than were returned
    /// (`events_total > events_returned`), i.e. the returned window is a tail
    /// slice and older events are not present.
    #[serde(default)]
    pub(crate) events_truncated: bool,
    /// 0-based sequence of the first returned event within the retained ledger
    /// (`events_total - events_returned`). `0` means the returned window starts
    /// at the ledger head. Read-only projections use this to avoid mistaking a
    /// truncated tail for the session start.
    #[serde(default)]
    pub(crate) first_retained_sequence: usize,
    pub(crate) messages: SessionMessagesSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_instructions: Option<ProjectInstructionsSummarySnapshot>,
}

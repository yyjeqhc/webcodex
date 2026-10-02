//! Durable WebCodex state persistence and SQLite storage semantics.

use self::connection_observation::{
    lock_connection as observed_lock_connection, StoreConnectionGuard, StoreConnectionObserver,
    TracingStoreConnectionObserver,
};
use crate::models::PairingCodeRecord;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod accounts;
mod activity;
mod admin_project_lifecycle;
mod agent_attention;
mod agent_task;
mod agent_wait;
mod agent_wake;
mod artifact_handoff;
mod audit;
mod trace_query;
pub use trace_query::{ToolTraceCallRecord, ToolTraceQueryFilter};
mod communication;
mod connection_observation;
mod external_observations;
#[cfg(test)]
mod external_observations_tests;
mod store_primitives;
pub use external_observations::{
    ExternalObservation, ExternalObservationError, MAX_EXTERNAL_OBSERVATIONS_PER_SESSION,
};
mod agent_continuation_reference;
mod agent_task_attempt_reference;
mod goal;
mod goal_plan;
#[cfg(test)]
mod goal_plan_tests;
mod goal_stall;
mod job_receipts;
mod job_terminal_wait;
#[cfg(test)]
mod job_terminal_wait_tests;
mod memory;
mod model_reference;
pub mod models;
mod oauth;
mod peer_collaboration;
mod window_collaboration;
#[cfg(test)]
mod window_collaboration_tests;
pub use window_collaboration::*;
mod project_reference;
mod schema;
mod server_instance;
mod window_activity;
mod window_inventory;
pub use window_inventory::{WindowInventoryPage, WindowInventoryQuery, WindowInventoryRow};

pub use self::admin_project_lifecycle::{AdminProjectAudit, AdminProjectIdempotencyRecord};
pub use self::agent_continuation_reference::AgentContinuationReferenceRecord;
pub use self::agent_task::{
    AgentTaskAttemptCompletionMutation, AgentTaskAttemptHeartbeatMutation, AgentTaskAttemptRecord,
    AgentTaskAttemptStartMutation, AgentTaskAttemptState, AgentTaskCodingRunBindingIntent,
    AgentTaskCodingRunBindingRecord, AgentTaskCodingRunDispatchClaim,
    AgentTaskCodingRunDispatchState, AgentTaskCodingRunObservation, AgentTaskCodingRunPrepared,
    AgentTaskCodingRunReconcileMutation, AgentTaskCodingRunStartContext, AgentTaskDetail,
    AgentTaskExecutionKind, AgentTaskExecutionRecoveryKind, AgentTaskExecutionStatus,
    AgentTaskMutation, AgentTaskPage, AgentTaskState, AgentTaskSummary, LiveAgentTaskAttemptPin,
    NewAgentTask, MAX_AGENT_TASK_LIST_LIMIT, MAX_AGENT_TASK_TERMINAL_TEXT_BYTES,
};
pub use self::agent_task_attempt_reference::AgentTaskAttemptReferenceRecord;
pub use self::agent_wait::{
    AgentWaitDetail, AgentWaitEventSelector, AgentWaitMatchRecord, AgentWaitMode,
    AgentWaitMutation, AgentWaitSourceRecord, AgentWaitState, NewAgentWait,
    AGENT_WAIT_EVENT_KIND_AGENT_TASK_TERMINAL, AGENT_WAIT_ID_PREFIX,
    MAX_ACTIVE_AGENT_WAITS_PER_AGENT, MAX_AGENT_WAITS_PER_SOURCE, MAX_AGENT_WAIT_SOURCES,
    MAX_GOAL_AGENT_WAIT_LIST_LIMIT,
};
#[allow(unused_imports)]
pub use self::agent_wake::{
    AgentConversationBootstrapRecord, AgentInboxBootstrapSummary, AgentWakeAttemptRecord,
    AgentWakeAttemptState, AgentWakeBootstrapSummary, AgentWakeClaim, AgentWakeConsumeResult,
    AgentWakeEnvelope, AgentWakeExplicitActivation, AgentWakePrepared, AgentWakeRecord,
    AgentWakeState, AGENT_WAKE_CONSUME_TOKEN_PREFIX, AGENT_WAKE_ID_PREFIX,
};
pub use self::artifact_handoff::{
    ArtifactHandoffAcceptance, ArtifactHandoffAcceptanceClaim, ArtifactHandoffAcceptanceOutcome,
    ArtifactHandoffAcceptanceState, ArtifactHandoffGrant, ArtifactHandoffGrantState,
    ArtifactHandoffImportRequest, ArtifactHandoffOperation, ArtifactHandoffPrincipal,
    ArtifactHandoffSourceSnapshot, ArtifactHandoffStoreError, NewArtifactHandoffGrant,
    ARTIFACT_HANDOFF_ACCEPTANCE_ID_PREFIX, ARTIFACT_HANDOFF_GRANT_ID_PREFIX,
    DEFAULT_ARTIFACT_HANDOFF_TTL_MS, MAX_ARTIFACT_HANDOFF_IDEMPOTENCY_KEY_CHARS,
    MAX_ARTIFACT_HANDOFF_TTL_MS,
};
pub use self::communication::{
    AgentEndpointLifecycle, AgentEndpointMutation, AgentEndpointRecord, AgentIdentityMutation,
    AgentIdentityPage, AgentInboxItem, AgentInboxPage, AgentProfilePatch, ConversationAccess,
    ConversationDetailRecord, ConversationLifecycle, ConversationMessageMutation,
    ConversationMessageRecord, ConversationMutation, ConversationPage,
    ConversationParticipantRecord, ConversationSummaryRecord, DeliveryConsumeResult,
    DurableAgentIdentity, McpAppEndpointRecovery, MessageAuthorRecord, MessageDeliveryRecord,
    MessageDeliveryState, NewAgentEndpoint, NewAgentIdentity, NewConversation,
    NewConversationMessage, MAX_COMMUNICATION_LIST_LIMIT, MAX_DURABLE_AGENTS,
};
pub(crate) use self::connection_observation::StoreDomain;
pub use self::goal::{
    GoalCorrelation, GoalCorrelationKind, GoalDetail, GoalLifecycle, GoalMutation, GoalPage,
    GoalPatch, GoalStoreError, GoalSummary, NewGoal, GOAL_ID_PREFIX, MAX_GOAL_CORRELATIONS,
    MAX_GOAL_LIST_LIMIT, MAX_GOAL_OBJECTIVE_BYTES, MAX_GOAL_TERMINAL_REASON_BYTES,
    MAX_GOAL_TITLE_CHARS, WORKFLOW_SESSION_ID_PREFIX,
};
pub use self::job_terminal_wait::{
    JobTerminalDeliveryPrepared, JobTerminalDeliveryState, JobTerminalFact,
    JobTerminalSourceIdentity, JobTerminalWaitMatch, JobTerminalWaitMutation,
    JobTerminalWaitPrincipal, JobTerminalWaitRecord, JobTerminalWaitState,
    JobTerminalWaitStoreError, NewJobTerminalWait, JOB_TERMINAL_DELIVERY_ATTEMPT_ID_PREFIX,
    JOB_TERMINAL_WAIT_ID_PREFIX, MAX_JOB_TERMINAL_WAITS_GLOBAL,
    MAX_JOB_TERMINAL_WAITS_PER_PRINCIPAL, MAX_JOB_TERMINAL_WAITS_PER_SOURCE,
};
#[allow(unused_imports)]
pub use self::memory::{
    canonicalize_memory_tags, memory_catalog_revision, valid_memory_catalog_revision,
    validate_memory_query, validate_memory_revision, validate_memory_scope_id, MemoryDeleteOutcome,
    MemoryPrincipalAttribution, MemoryPriority, MemoryScopeAttribution, MemoryScopePurgeOutcome,
    MemorySetInput, MemorySetOutcome, MemoryStoreError, ProjectMemoryRecord,
    ProjectMemoryScopeRecord, ProjectMemoryScopeSnapshot, MAX_MEMORIES_GLOBAL,
    MAX_MEMORY_BODY_BYTES, MAX_MEMORY_BOOTSTRAP_BYTES, MAX_MEMORY_KEY_CHARS,
    MAX_MEMORY_QUERY_CHARS, MAX_MEMORY_SCOPE_LIST_LIMIT, MAX_MEMORY_SEARCH_LIMIT,
    MAX_MEMORY_SEARCH_RESULT_BYTES, MAX_MEMORY_SUMMARY_CHARS, MAX_MEMORY_TAGS,
    MAX_MEMORY_TAG_CHARS,
};
#[cfg(any(test, feature = "root-test-support"))]
pub use self::memory::{
    memory_definition_hash, memory_state_revision, validate_memory_body, validate_memory_key,
    validate_memory_summary, MAX_MEMORIES_PER_PROJECT, MEMORY_SCOPE_IDENTITY_ATTRIBUTED,
};
pub use self::model_reference::{ModelReferenceRecord, ModelReferenceStoreError};
pub use self::oauth::RotateResult;
pub use self::peer_collaboration::{
    NewPeerMessage, PeerAttentionBatch, PeerMessageDelivery, PeerMessageDeliveryOutcome,
    PeerMessageRecord, PeerProjectionRollback, RecentProjectPeerRecord, MAX_PEER_DISCOVERY_LIMIT,
    MAX_PEER_MESSAGE_LIMIT,
};
pub use self::project_reference::{ProjectReferenceRecord, ProjectReferenceStoreError};
pub use self::server_instance::ServerInstanceGuard;
pub use self::store_primitives::{
    CommunicationPrincipal, CommunicationStoreError, COMMUNICATION_PRINCIPAL_DIGEST_PREFIX,
};
pub use self::window_activity::{MAX_WINDOW_ACTIVITY_LIMIT, MAX_WINDOW_LINK_LIMIT};

pub struct Database {
    conn: Mutex<Connection>,
    // Exactly one additional, read-only WAL connection for potentially long history.
    // Fast authority/reference/receipt reads stay on the canonical writer lane.
    history_reader: std::sync::OnceLock<Mutex<Connection>>,
    #[cfg(any(test, feature = "root-test-support"))]
    window_history_reads: std::sync::atomic::AtomicUsize,
    #[cfg(any(test, feature = "root-test-support"))]
    window_inventory_reads: std::sync::atomic::AtomicUsize,
    connection_observer: Arc<dyn StoreConnectionObserver>,
    state_path: PathBuf,
}

impl Database {
    fn from_connection(conn: Connection, state_path: PathBuf) -> Self {
        Self {
            conn: Mutex::new(conn),
            history_reader: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "root-test-support"))]
            window_history_reads: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(any(test, feature = "root-test-support"))]
            window_inventory_reads: std::sync::atomic::AtomicUsize::new(0),
            connection_observer: Arc::new(TracingStoreConnectionObserver),
            state_path,
        }
    }

    pub(crate) fn lock_connection(&self, domain: StoreDomain) -> StoreConnectionGuard<'_> {
        observed_lock_connection(&self.conn, self.connection_observer.as_ref(), domain)
    }

    pub(crate) fn open_history_reader(&self) -> anyhow::Result<()> {
        let connection = Connection::open_with_flags(
            &self.state_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.execute_batch("PRAGMA query_only=ON; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA cache_size=-2048;")?;
        self.history_reader
            .set(Mutex::new(connection))
            .map_err(|_| anyhow::anyhow!("history reader already initialized"))
    }

    pub(crate) fn lock_history_connection(&self, domain: StoreDomain) -> StoreConnectionGuard<'_> {
        observed_lock_connection(
            self.history_reader
                .get()
                .expect("history reader initialized before Database::open returns"),
            self.connection_observer.as_ref(),
            domain,
        )
    }

    pub(crate) fn state_path(&self) -> &Path {
        &self.state_path
    }
}

#[derive(Debug, Clone)]
pub enum PairingConsumeResult {
    NotFound,
    Consumed(PairingCodeRecord),
    AlreadyUsed(PairingCodeRecord),
    Expired(PairingCodeRecord),
    ClientMismatch(PairingCodeRecord),
}

#[cfg(any(test, feature = "root-test-support"))]
impl Database {
    /// Test-only access to the underlying connection so tests can assert on
    /// raw storage (e.g. that a plaintext token is never stored as `key_hash`).
    pub fn window_read_counts_for_test(&self) -> (usize, usize) {
        use std::sync::atomic::Ordering::Relaxed;
        (
            self.window_history_reads.load(Relaxed),
            self.window_inventory_reads.load(Relaxed),
        )
    }

    pub fn conn_for_tests(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

#[cfg(test)]
mod agent_attention_tests;
#[cfg(test)]
mod agent_continuation_reference_tests;
#[cfg(test)]
mod agent_task_attempt_reference_tests;
#[cfg(test)]
mod agent_task_tests;
#[cfg(test)]
mod agent_wait_tests;
#[cfg(test)]
mod agent_wake_recovery_tests;
#[cfg(test)]
mod agent_wake_tests;
#[cfg(test)]
mod artifact_handoff_tests;
#[cfg(test)]
mod communication_tests;
#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod goal_tests;
#[cfg(test)]
mod memory_tests;
#[cfg(test)]
mod model_reference_tests;
#[cfg(test)]
mod project_reference_tests;

#[cfg(test)]
mod job_receipts_tests;

pub use goal_plan::{
    GoalCheckpoint, GoalPlan, GoalStep, GoalStepStatus, NewGoalStep,
    MAX_GOAL_COMPLETION_CONDITIONS, MAX_GOAL_CONDITION_BYTES, MAX_GOAL_PLAN_BYTES,
    MAX_GOAL_PROGRESS_SUMMARY_BYTES, MAX_GOAL_STEPS, MAX_GOAL_STEP_ID_BYTES,
    MAX_GOAL_STEP_TITLE_CHARS,
};

pub use goal_stall::{
    GoalStallAttention, GoalStallCandidate, GoalStallContinuityObservation,
    GoalStallHostDeliveryObservation, GoalStallResumeObservation, GoalStallWakeObservation,
    GOAL_ACTIVITY_ATTENTION_AFTER_MS, GOAL_CARD_OBSERVATION_ADVANCE_MS,
    GOAL_CARD_OBSERVATION_LEASE_MS,
};

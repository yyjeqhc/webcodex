use crate::models::PairingCodeRecord;
#[cfg(test)]
use crate::models::{
    ApiKeyRecord, OAuthAccessTokenRecord, OAuthAuthorizationCodeRecord, OAuthClientRecord,
    OAuthRefreshTokenRecord, UserRecord,
};
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Mutex;

mod accounts;
mod activity;
mod admin_project_lifecycle;
mod audit;
mod communication;
mod execution_model;
mod executions;
mod memory;
mod oauth;
mod schema;
mod task_kernel;

pub use self::activity::WorkspaceActivityStore;
pub(crate) use self::admin_project_lifecycle::AdminProjectAudit;
pub(crate) use self::communication::{
    AgentProfilePatch, CommunicationPrincipal, CommunicationStoreError, ConversationAccess,
    NewAgentEndpoint, NewAgentIdentity, NewConversation, NewConversationMessage,
    COMMUNICATION_PRINCIPAL_DIGEST_PREFIX,
};
#[cfg(test)]
pub(crate) use self::execution_model::ConnectorExecutionContinuationIntent;
pub(crate) use self::execution_model::{
    ConnectorExecution, ConnectorExecutionFailure, ConnectorExecutionObservation,
    ConnectorExecutionReservation, MAX_ASSERTION_EVIDENCE_BYTES,
};
pub(crate) use self::memory::{
    canonicalize_memory_tags, memory_catalog_revision, valid_memory_catalog_revision,
    validate_memory_query, validate_memory_revision, validate_memory_scope_id,
    MemoryPrincipalAttribution, MemoryPriority, MemoryScopeAttribution, MemorySetInput,
    MemoryStoreError, ProjectMemoryRecord, ProjectMemoryScopeRecord, MAX_MEMORIES_GLOBAL,
    MAX_MEMORY_BODY_BYTES, MAX_MEMORY_BOOTSTRAP_BYTES, MAX_MEMORY_KEY_CHARS,
    MAX_MEMORY_QUERY_CHARS, MAX_MEMORY_SCOPE_LIST_LIMIT, MAX_MEMORY_SEARCH_LIMIT,
    MAX_MEMORY_SEARCH_RESULT_BYTES, MAX_MEMORY_SUMMARY_CHARS, MAX_MEMORY_TAGS,
    MAX_MEMORY_TAG_CHARS,
};
#[cfg(test)]
pub(crate) use self::memory::{
    validate_memory_body, validate_memory_key, validate_memory_summary, MAX_MEMORIES_PER_PROJECT,
};
pub use self::oauth::RotateResult;
pub(crate) use self::task_kernel::{
    ConnectorApproval, ConnectorApprovalGate, ConnectorBinding, ConnectorEditOperationGate,
    ConnectorPreservedWorkspace, ConnectorTaskContinuation, ConnectorTaskResult,
    ConnectorTaskSnapshot, ConnectorTaskStoreError, ConnectorWindowBinding,
    ConnectorWorkspaceTransition, GuidanceReadState, NewConnectorResult, NewConnectorTask,
    WindowProjectActivation,
};
pub struct Database {
    conn: Mutex<Connection>,
    /// Ephemeral navigation only. Durable work stays in wc_tasks and
    /// wc_window_project_contexts; restarting never guesses a window's current
    /// project.
    window_projects: Mutex<HashMap<(String, String), String>>,
}

#[derive(Debug, Clone)]
pub enum PairingConsumeResult {
    NotFound,
    Consumed(PairingCodeRecord),
    AlreadyUsed(PairingCodeRecord),
    Expired(PairingCodeRecord),
    ClientMismatch(PairingCodeRecord),
}

#[cfg(test)]
impl Database {
    /// Test-only access to the underlying connection so tests can assert on
    /// raw storage (e.g. that a plaintext token is never stored as `key_hash`).
    pub fn conn_for_tests(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

#[cfg(test)]
#[path = "db/communication_tests.rs"]
mod communication_tests;

#[cfg(test)]
#[path = "db/continuation_delivery_tests.rs"]
mod continuation_delivery_tests;

#[cfg(test)]
#[path = "db/execution_intent_tests.rs"]
mod execution_intent_tests;

#[cfg(test)]
#[path = "db/memory_tests.rs"]
mod memory_tests;

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;

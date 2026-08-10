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
mod execution_model;
mod executions;
mod oauth;
mod schema;
mod task_kernel;

pub use self::activity::WorkspaceActivityStore;
pub(crate) use self::admin_project_lifecycle::AdminProjectAudit;
pub(crate) use self::execution_model::{
    ConnectorExecution, ConnectorExecutionFailure, ConnectorExecutionObservation,
    ConnectorExecutionReservation, MAX_ASSERTION_EVIDENCE_BYTES,
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
#[path = "db_tests.rs"]
mod tests;

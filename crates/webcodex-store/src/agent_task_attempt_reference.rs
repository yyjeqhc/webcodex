use super::agent_task::{
    AGENT_TASK_ATTEMPT_FENCE_PREFIX, AGENT_TASK_ATTEMPT_ID_PREFIX, AGENT_TASK_ID_PREFIX,
};
use super::communication::DURABLE_AGENT_ID_PREFIX;
use super::store_primitives::{
    validate_communication_principal, validate_id, validate_proof, CommunicationPrincipal,
    CommunicationStoreError,
};
use super::{Database, StoreDomain};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskAttemptReferenceRecord {
    pub ref_index: u64,
    pub task_id: String,
    pub attempt_id: String,
    pub assignee_agent_id: String,
    pub attempt_fence: String,
    pub attempt_controller_generation: i64,
}

fn reference_store_error(error: rusqlite::Error) -> CommunicationStoreError {
    tracing::warn!(
        error = %error,
        "agent task attempt reference store operation failed"
    );
    CommunicationStoreError::new(
        "communication_store_unavailable",
        "Durable communication store is unavailable",
    )
}

fn validate_reference_tuple(
    principal: &CommunicationPrincipal,
    task_id: &str,
    attempt_id: &str,
    assignee_agent_id: &str,
    attempt_fence: &str,
    attempt_controller_generation: i64,
) -> Result<(), CommunicationStoreError> {
    validate_communication_principal(principal)?;
    validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
    validate_id(
        attempt_id,
        AGENT_TASK_ATTEMPT_ID_PREFIX,
        "invalid_agent_task_attempt_id",
    )?;
    validate_id(
        assignee_agent_id,
        DURABLE_AGENT_ID_PREFIX,
        "invalid_agent_id",
    )?;
    validate_proof(
        attempt_fence,
        AGENT_TASK_ATTEMPT_FENCE_PREFIX,
        "invalid_agent_task_attempt_fence",
    )?;
    if attempt_controller_generation < 1 {
        return Err(CommunicationStoreError::new(
            "invalid_agent_task_attempt_controller_generation",
            "attempt_controller_generation must be at least 1",
        ));
    }
    Ok(())
}

impl Database {
    /// Return the stable index for one exact fenced Attempt tuple. A different
    /// fence, assignee, or controller generation is a different row. This
    /// mapping grants no ownership or execution authority.
    pub fn get_or_create_agent_task_attempt_reference(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        created_at_unix_ms: i64,
    ) -> Result<AgentTaskAttemptReferenceRecord, CommunicationStoreError> {
        validate_reference_tuple(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        let mut conn = self.lock_connection(StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(reference_store_error)?;
        let existing = transaction
            .query_row(
                "SELECT ref_index, task_id, attempt_id, assignee_agent_id, attempt_fence,
                        attempt_controller_generation
                 FROM wc_agent_task_attempt_references
                 WHERE principal_kind = ?1
                   AND principal_digest = ?2
                   AND task_id = ?3
                   AND attempt_id = ?4
                   AND assignee_agent_id = ?5
                   AND attempt_fence = ?6
                   AND attempt_controller_generation = ?7",
                params![
                    principal.kind,
                    principal.digest,
                    task_id,
                    attempt_id,
                    assignee_agent_id,
                    attempt_fence,
                    attempt_controller_generation
                ],
                row_to_reference,
            )
            .optional()
            .map_err(reference_store_error)?;
        if let Some(existing) = existing {
            transaction.commit().map_err(reference_store_error)?;
            return Ok(existing);
        }

        let next_index: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(ref_index), 0) + 1
                 FROM wc_agent_task_attempt_references
                 WHERE principal_kind = ?1 AND principal_digest = ?2",
                params![principal.kind, principal.digest],
                |row| row.get(0),
            )
            .map_err(reference_store_error)?;
        if next_index <= 0 {
            return Err(CommunicationStoreError::new(
                "communication_store_unavailable",
                "Durable communication store is unavailable",
            ));
        }
        transaction
            .execute(
                "INSERT INTO wc_agent_task_attempt_references (
                    principal_kind, principal_digest, ref_index, task_id, attempt_id,
                    assignee_agent_id, attempt_fence, attempt_controller_generation,
                    created_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    principal.kind,
                    principal.digest,
                    next_index,
                    task_id,
                    attempt_id,
                    assignee_agent_id,
                    attempt_fence,
                    attempt_controller_generation,
                    created_at_unix_ms
                ],
            )
            .map_err(reference_store_error)?;
        transaction.commit().map_err(reference_store_error)?;
        Ok(AgentTaskAttemptReferenceRecord {
            ref_index: next_index as u64,
            task_id: task_id.to_string(),
            attempt_id: attempt_id.to_string(),
            assignee_agent_id: assignee_agent_id.to_string(),
            attempt_fence: attempt_fence.to_string(),
            attempt_controller_generation,
        })
    }

    pub fn lookup_agent_task_attempt_reference(
        &self,
        principal: &CommunicationPrincipal,
        ref_index: u64,
    ) -> Result<Option<AgentTaskAttemptReferenceRecord>, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        let ref_index = i64::try_from(ref_index).map_err(|_| invalid_reference_index())?;
        if ref_index <= 0 {
            return Err(invalid_reference_index());
        }
        let conn = self.lock_connection(StoreDomain::AgentTask);
        conn.query_row(
            "SELECT ref_index, task_id, attempt_id, assignee_agent_id, attempt_fence,
                    attempt_controller_generation
             FROM wc_agent_task_attempt_references
             WHERE principal_kind = ?1 AND principal_digest = ?2 AND ref_index = ?3",
            params![principal.kind, principal.digest, ref_index],
            row_to_reference,
        )
        .optional()
        .map_err(reference_store_error)
    }
}

fn invalid_reference_index() -> CommunicationStoreError {
    CommunicationStoreError::new(
        "invalid_agent_task_attempt_ref",
        "attempt_ref is not a server-issued selector",
    )
}

fn row_to_reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTaskAttemptReferenceRecord> {
    let ref_index: i64 = row.get(0)?;
    Ok(AgentTaskAttemptReferenceRecord {
        ref_index: ref_index as u64,
        task_id: row.get(1)?,
        attempt_id: row.get(2)?,
        assignee_agent_id: row.get(3)?,
        attempt_fence: row.get(4)?,
        attempt_controller_generation: row.get(5)?,
    })
}

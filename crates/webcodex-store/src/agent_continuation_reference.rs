use super::communication::{
    validate_communication_principal, validate_id, CommunicationPrincipal, CommunicationStoreError,
    AGENT_ENDPOINT_ID_PREFIX, DURABLE_AGENT_ID_PREFIX,
};
use super::{Database, StoreDomain};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContinuationReferenceRecord {
    pub ref_index: u64,
    pub agent_id: String,
    pub endpoint_id: String,
    pub controller_generation: i64,
}

fn reference_store_error(error: rusqlite::Error) -> CommunicationStoreError {
    tracing::warn!(error = %error, "agent continuation reference store operation failed");
    CommunicationStoreError::new(
        "communication_store_unavailable",
        "Durable communication store is unavailable",
    )
}

fn validate_reference_tuple(
    principal: &CommunicationPrincipal,
    agent_id: &str,
    endpoint_id: &str,
    controller_generation: i64,
) -> Result<(), CommunicationStoreError> {
    validate_communication_principal(principal)?;
    validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")?;
    validate_id(endpoint_id, AGENT_ENDPOINT_ID_PREFIX, "invalid_endpoint_id")?;
    if controller_generation < 1 {
        return Err(CommunicationStoreError::new(
            "invalid_controller_generation",
            "controller_generation must be at least 1",
        ));
    }
    Ok(())
}

impl Database {
    /// Return the stable index for one exact fenced tuple. A different generation
    /// is a different row. This mapping grants no ownership or execution authority.
    pub fn get_or_create_agent_continuation_reference(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        controller_generation: i64,
        created_at_unix_ms: i64,
    ) -> Result<AgentContinuationReferenceRecord, CommunicationStoreError> {
        validate_reference_tuple(principal, agent_id, endpoint_id, controller_generation)?;
        let mut conn = self.lock_connection(StoreDomain::Communication);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(reference_store_error)?;
        let existing = transaction
            .query_row(
                "SELECT ref_index, agent_id, endpoint_id, controller_generation
                 FROM wc_agent_continuation_references
                 WHERE principal_kind = ?1
                   AND principal_digest = ?2
                   AND agent_id = ?3
                   AND endpoint_id = ?4
                   AND controller_generation = ?5",
                params![
                    principal.kind,
                    principal.digest,
                    agent_id,
                    endpoint_id,
                    controller_generation
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
                 FROM wc_agent_continuation_references
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
                "INSERT INTO wc_agent_continuation_references (
                    principal_kind, principal_digest, ref_index, agent_id, endpoint_id,
                    controller_generation, created_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    principal.kind,
                    principal.digest,
                    next_index,
                    agent_id,
                    endpoint_id,
                    controller_generation,
                    created_at_unix_ms
                ],
            )
            .map_err(reference_store_error)?;
        transaction.commit().map_err(reference_store_error)?;
        Ok(AgentContinuationReferenceRecord {
            ref_index: next_index as u64,
            agent_id: agent_id.to_string(),
            endpoint_id: endpoint_id.to_string(),
            controller_generation,
        })
    }

    pub fn lookup_agent_continuation_reference(
        &self,
        principal: &CommunicationPrincipal,
        ref_index: u64,
    ) -> Result<Option<AgentContinuationReferenceRecord>, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        let ref_index = i64::try_from(ref_index).map_err(|_| invalid_reference_index())?;
        if ref_index <= 0 {
            return Err(invalid_reference_index());
        }
        let conn = self.lock_connection(StoreDomain::Communication);
        conn.query_row(
            "SELECT ref_index, agent_id, endpoint_id, controller_generation
             FROM wc_agent_continuation_references
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
        "invalid_agent_continuation_ref",
        "agent_continuation_ref is not a server-issued selector",
    )
}

fn row_to_reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentContinuationReferenceRecord> {
    let ref_index: i64 = row.get(0)?;
    Ok(AgentContinuationReferenceRecord {
        ref_index: ref_index as u64,
        agent_id: row.get(1)?,
        endpoint_id: row.get(2)?,
        controller_generation: row.get(3)?,
    })
}

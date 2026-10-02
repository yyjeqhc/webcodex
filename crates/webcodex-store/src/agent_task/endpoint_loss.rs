//! AgentTask-owned endpoint-loss writes, composed with Wake reconciliation
//! through the caller-owned immediate transaction. Wake state predicates remain
//! exact historical SQL; this helper does not broaden authority or commit.

use super::{replace_agent_task_attempt_controller_in_transaction, AttemptAuthority};
use crate::store_primitives::{store_error, CommunicationPrincipal, CommunicationStoreError};
use rusqlite::{params, Transaction};

pub(crate) fn fence_agent_task_controllers_for_endpoint_loss(
    transaction: &Transaction<'_>,
    agent_id: &str,
    endpoint_id: &str,
    endpoint_controller_generation: i64,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let controllers = {
        let mut statement = transaction
            .prepare(
                "SELECT t.owner_principal_kind, t.owner_principal_digest,
                        e.task_id, e.attempt_id, a.assignee_agent_id,
                        a.attempt_fence, a.attempt_controller_generation
                 FROM wc_agent_task_endpoint_executions e
                 JOIN wc_agent_wakes w ON w.wake_id = e.wake_id
                 JOIN wc_agent_tasks t ON t.task_id = e.task_id
                 JOIN wc_agent_task_attempts a
                   ON a.task_id = e.task_id AND a.attempt_id = e.attempt_id
                 WHERE e.endpoint_id = ?1 AND e.endpoint_controller_generation = ?2
                   AND w.target_agent_id = ?3 AND w.trigger_kind = 'agent_task_attempt'
                   AND t.latest_attempt_id = a.attempt_id AND t.state = 'active'
                   AND a.assignee_agent_id = ?3 AND a.state = 'active'
                   AND a.lease_expires_at_unix_ms > ?4",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map(
                params![endpoint_id, endpoint_controller_generation, agent_id, now],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .map_err(store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store_error)?;
        rows
    };

    for (
        principal_kind,
        principal_digest,
        task_id,
        attempt_id,
        assignee_agent_id,
        attempt_fence,
        attempt_controller_generation,
    ) in controllers
    {
        let principal = CommunicationPrincipal {
            kind: principal_kind,
            digest: principal_digest,
        };
        let authority = AttemptAuthority::validated(
            &principal,
            &task_id,
            &attempt_id,
            &assignee_agent_id,
            &attempt_fence,
            attempt_controller_generation,
        )?;
        replace_agent_task_attempt_controller_in_transaction(
            transaction,
            &principal,
            authority,
            now,
        )?;
    }
    Ok(())
}

pub(crate) fn clear_endpoint_execution_bindings_for_endpoint_loss_in_transaction(
    transaction: &Transaction<'_>,
    agent_id: &str,
    endpoint_id: &str,
    controller_generation: i64,
    now: i64,
    fence_task_controller: bool,
) -> Result<(), CommunicationStoreError> {
    transaction
        .execute(
            "UPDATE wc_agent_task_endpoint_executions
             SET endpoint_id = NULL, endpoint_controller_generation = NULL,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?4)
             WHERE endpoint_id = ?1 AND endpoint_controller_generation = ?2
               AND wake_id IN (
                   SELECT wake_id FROM wc_agent_wakes
                   WHERE target_agent_id = ?3 AND trigger_kind = 'agent_task_attempt'
                     AND (?5 != 0 OR state = 'claimed')
               )",
            params![
                endpoint_id,
                controller_generation,
                agent_id,
                now,
                fence_task_controller as i64,
            ],
        )
        .map_err(store_error)?;
    Ok(())
}

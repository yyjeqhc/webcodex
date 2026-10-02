//! Atomic TaskAttempt terminal transition and its dependent notifications.
//!
//! Request/replay admission and commit remain with the outer Store operation.
//! This coordinator accepts that same transaction and cannot independently
//! commit a Task without its attention events or Wait matches.

use super::{
    require_current_attempt, retire_pre_dispatch_endpoint_execution_for_attempt,
    AgentTaskAttemptState, AgentTaskState, AttemptAuthority, StoredTask,
};
use crate::agent_attention::create_agent_task_terminal_attention_in_transaction;
use crate::agent_wait::record_agent_task_terminal_wait_matches_in_transaction;
use crate::store_primitives::{store_error, CommunicationPrincipal, CommunicationStoreError};
use rusqlite::{params, Transaction};

pub(super) struct CompletionEffects {
    pub(super) attention_event_count: usize,
    pub(super) attention_target_agent_ids: Vec<String>,
    pub(super) wait_target_agent_ids: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn complete_attempt_in_transaction(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    task: &StoredTask,
    authority: AttemptAuthority<'_>,
    outcome: AgentTaskState,
    terminal_result: Option<&str>,
    terminal_reason: Option<&str>,
    now: i64,
) -> Result<CompletionEffects, CommunicationStoreError> {
    let task_id = authority.task_id;
    let attempt_id = authority.attempt_id;
    let assignee_agent_id = authority.assignee_agent_id;
    let _attempt = require_current_attempt(transaction, task, authority, now)?;
    let attempt_state = match outcome {
        AgentTaskState::Succeeded => AgentTaskAttemptState::Succeeded,
        AgentTaskState::Failed => AgentTaskAttemptState::Failed,
        AgentTaskState::Ready | AgentTaskState::Active => {
            unreachable!("validated terminal outcome")
        }
    };
    transaction
        .execute(
            "UPDATE wc_agent_task_attempts
             SET state = ?2, terminal_at_unix_ms = ?3,
                 terminal_result = ?4, terminal_reason = ?5
             WHERE attempt_id = ?1",
            params![
                attempt_id,
                attempt_state.as_str(),
                now,
                terminal_result,
                terminal_reason,
            ],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_tasks
             SET state = ?2, terminal_attempt_id = ?3, terminal_at_unix_ms = ?4,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?4)
             WHERE task_id = ?1",
            params![task_id, outcome.as_str(), attempt_id, now],
        )
        .map_err(store_error)?;
    retire_pre_dispatch_endpoint_execution_for_attempt(transaction, attempt_id, now)?;
    let (attention_event_count, attention_target_agent_ids) =
        create_agent_task_terminal_attention_in_transaction(
            transaction,
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            outcome,
            now,
        )?;
    let wait_matches = record_agent_task_terminal_wait_matches_in_transaction(
        transaction,
        principal,
        task_id,
        attempt_id,
        outcome,
        now,
    )?;
    Ok(CompletionEffects {
        attention_event_count,
        attention_target_agent_ids,
        wait_target_agent_ids: wait_matches.schedule_agent_ids,
    })
}

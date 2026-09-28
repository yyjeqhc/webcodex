use super::agent_task::AgentTaskState;
use super::agent_wait::goal_scoped_wait_owns_terminal_attention_in_transaction;
use super::agent_wake::{AGENT_WAKE_ID_PREFIX, WAKE_TRIGGER_ATTENTION_EVENT};
use super::communication::{
    allocate_identity, store_error, CommunicationPrincipal, CommunicationStoreError,
};
use super::goal::MAX_GOAL_CORRELATIONS;
use super::Database;
use rusqlite::{params, Connection, OptionalExtension, Transaction};

pub(crate) const AGENT_ATTENTION_EVENT_ID_PREFIX: &str = "wc_attention_event_";
pub(crate) const AGENT_ATTENTION_EVENT_KIND_AGENT_TASK_TERMINAL: &str = "agent_task_terminal";
pub(crate) const AGENT_ATTENTION_EVENT_KIND_GOAL_WORKFLOW_STALLED: &str = "goal_workflow_stalled";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentAttentionSource {
    AgentTaskTerminal {
        task_id: String,
        task_attempt_id: String,
        terminal_task_state: AgentTaskState,
    },
    GoalWorkflowStalled {
        workflow_session_id: String,
        observed_window_key: String,
        observation_principal_kind: String,
        observation_principal_id: String,
        last_meaningful_activity_at_ms: i64,
        last_seen_at_ms: i64,
        goal_revision: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentAttentionEventRecord {
    pub event_id: String,
    pub kind: String,
    pub owner_principal_kind: String,
    pub owner_principal_digest: String,
    pub target_agent_id: String,
    pub goal_id: String,
    pub source: AgentAttentionSource,
    pub created_at_unix_ms: i64,
}

impl AgentAttentionEventRecord {
    pub fn task_id(&self) -> Option<&str> {
        match &self.source {
            AgentAttentionSource::AgentTaskTerminal { task_id, .. } => Some(task_id),
            AgentAttentionSource::GoalWorkflowStalled { .. } => None,
        }
    }

    pub fn task_attempt_id(&self) -> Option<&str> {
        match &self.source {
            AgentAttentionSource::AgentTaskTerminal {
                task_attempt_id, ..
            } => Some(task_attempt_id),
            AgentAttentionSource::GoalWorkflowStalled { .. } => None,
        }
    }

    pub fn workflow_session_id(&self) -> Option<&str> {
        match &self.source {
            AgentAttentionSource::AgentTaskTerminal { .. } => None,
            AgentAttentionSource::GoalWorkflowStalled {
                workflow_session_id,
                ..
            } => Some(workflow_session_id),
        }
    }
}

const ATTENTION_TABLE_SQL: &str = "
    CREATE TABLE IF NOT EXISTS wc_agent_attention_events (
        event_id TEXT PRIMARY KEY,
        kind TEXT NOT NULL CHECK(kind IN ('agent_task_terminal', 'goal_workflow_stalled')),
        owner_principal_kind TEXT NOT NULL,
        owner_principal_digest TEXT NOT NULL,
        target_agent_id TEXT NOT NULL,
        goal_id TEXT NOT NULL,
        task_id TEXT,
        task_attempt_id TEXT,
        terminal_task_state TEXT,
        workflow_session_id TEXT,
        observed_window_key TEXT,
        observation_principal_kind TEXT,
        observation_principal_id TEXT,
        last_meaningful_activity_at_ms INTEGER,
        last_seen_at_ms INTEGER,
        goal_revision INTEGER,
        created_at_unix_ms INTEGER NOT NULL,
        FOREIGN KEY(target_agent_id) REFERENCES wc_agent_identities(agent_id),
        FOREIGN KEY(goal_id) REFERENCES wc_goals(goal_id),
        FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id),
        FOREIGN KEY(task_attempt_id) REFERENCES wc_agent_task_attempts(attempt_id),
        CHECK (
            (kind = 'agent_task_terminal'
             AND task_id IS NOT NULL AND task_attempt_id IS NOT NULL
             AND terminal_task_state IS NOT NULL AND terminal_task_state IN ('succeeded', 'failed')
             AND workflow_session_id IS NULL AND observed_window_key IS NULL
             AND observation_principal_kind IS NULL AND observation_principal_id IS NULL
             AND last_meaningful_activity_at_ms IS NULL AND last_seen_at_ms IS NULL AND goal_revision IS NULL)
            OR
            (kind = 'goal_workflow_stalled'
             AND task_id IS NULL AND task_attempt_id IS NULL AND terminal_task_state IS NULL
             AND workflow_session_id IS NOT NULL AND observed_window_key IS NOT NULL
             AND observation_principal_kind IS NOT NULL AND observation_principal_id IS NOT NULL
             AND last_meaningful_activity_at_ms IS NOT NULL AND last_seen_at_ms IS NOT NULL
             AND goal_revision IS NOT NULL AND goal_revision >= 1
             AND last_meaningful_activity_at_ms < last_seen_at_ms
             AND last_seen_at_ms <= created_at_unix_ms)
        ),
        UNIQUE(kind, goal_id, task_attempt_id),
        UNIQUE(kind, goal_id, last_meaningful_activity_at_ms)
    );";

impl Database {
    pub(super) fn ensure_agent_attention_schema(conn: &mut Connection) -> anyhow::Result<()> {
        let old_shape: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'wc_agent_attention_events')
               AND NOT EXISTS(SELECT 1 FROM pragma_table_info('wc_agent_attention_events') WHERE name = 'workflow_session_id')",
            [], |row| row.get(0),
        )?;
        // A single transactional schema replacement preserves the existing Task
        // attention facts. There is no legacy read/write shape or dual routing.
        if old_shape {
            let transaction =
                conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            transaction.execute_batch("ALTER TABLE wc_agent_attention_events RENAME TO wc_agent_attention_events_before_goal_workflow;")?;
            transaction.execute_batch(ATTENTION_TABLE_SQL)?;
            transaction.execute_batch(
                "INSERT INTO wc_agent_attention_events (
                    event_id, kind, owner_principal_kind, owner_principal_digest,
                    target_agent_id, goal_id, task_id, task_attempt_id, terminal_task_state, created_at_unix_ms
                 ) SELECT event_id, kind, owner_principal_kind, owner_principal_digest,
                          target_agent_id, goal_id, task_id, task_attempt_id, terminal_task_state, created_at_unix_ms
                   FROM wc_agent_attention_events_before_goal_workflow;
                 DROP TABLE wc_agent_attention_events_before_goal_workflow;",
            )?;
            transaction.commit()?;
        } else {
            conn.execute_batch(ATTENTION_TABLE_SQL)?;
        }
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_wc_agent_attention_events_owner_created
                ON wc_agent_attention_events(owner_principal_digest, created_at_unix_ms, event_id);
             CREATE INDEX IF NOT EXISTS idx_wc_agent_attention_events_target_created
                ON wc_agent_attention_events(target_agent_id, created_at_unix_ms, event_id);",
        )?;
        Ok(())
    }
}

pub(super) fn create_agent_task_terminal_attention_in_transaction(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    task_id: &str,
    task_attempt_id: &str,
    fallback_worker_agent_id: &str,
    terminal_task_state: AgentTaskState,
    now: i64,
) -> Result<(usize, Vec<String>), CommunicationStoreError> {
    if !terminal_task_state.terminal() {
        return Err(CommunicationStoreError::new(
            "agent_attention_event_invariant",
            "Terminal Agent attention can only be created for a terminal AgentTask",
        ));
    }

    let goal_routes = {
        let mut statement = transaction
            .prepare(
                "SELECT g.goal_id, g.controller_agent_id
                 FROM wc_goal_correlations c
                 JOIN wc_goals g ON g.goal_id = c.goal_id
                 WHERE c.kind = 'agent_task' AND c.reference_id = ?1
                   AND g.owner_principal_kind = ?2 AND g.owner_principal_digest = ?3
                   AND g.lifecycle = 'active'
                 ORDER BY g.goal_id
                 LIMIT ?4",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map(
                params![
                    task_id,
                    principal.kind,
                    principal.digest,
                    MAX_GOAL_CORRELATIONS + 1,
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .map_err(store_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(store_error)?
    };
    if goal_routes.len() as i64 > MAX_GOAL_CORRELATIONS {
        return Err(CommunicationStoreError::new(
            "agent_attention_goal_capacity_exceeded",
            format!(
                "One AgentTask may produce attention for at most {MAX_GOAL_CORRELATIONS} active Goals"
            ),
        ));
    }

    let mut schedule_agent_ids = Vec::new();
    let mut attention_event_count = 0usize;
    for (goal_id, controller_agent_id) in &goal_routes {
        if goal_scoped_wait_owns_terminal_attention_in_transaction(
            transaction,
            principal,
            goal_id,
            task_id,
        )? {
            continue;
        }
        let target_agent_id = controller_agent_id
            .as_deref()
            .unwrap_or(fallback_worker_agent_id);
        require_owned_attention_target(transaction, principal, target_agent_id)?;
        if !schedule_agent_ids
            .iter()
            .any(|agent_id| agent_id == target_agent_id)
        {
            schedule_agent_ids.push(target_agent_id.to_string());
        }
        let event_id = allocate_identity(
            transaction,
            AGENT_ATTENTION_EVENT_ID_PREFIX,
            "SELECT EXISTS(SELECT 1 FROM wc_agent_attention_events WHERE event_id = ?1)",
        )?;
        transaction
            .execute(
                "INSERT INTO wc_agent_attention_events (
                    event_id, kind, owner_principal_kind, owner_principal_digest,
                    target_agent_id, goal_id, task_id, task_attempt_id,
                    terminal_task_state, created_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    event_id,
                    AGENT_ATTENTION_EVENT_KIND_AGENT_TASK_TERMINAL,
                    principal.kind,
                    principal.digest,
                    target_agent_id,
                    goal_id,
                    task_id,
                    task_attempt_id,
                    terminal_task_state.as_str(),
                    now,
                ],
            )
            .map_err(store_error)?;
        insert_attention_wake_in_transaction(transaction, target_agent_id, &event_id, now)?;
        attention_event_count += 1;
    }
    Ok((attention_event_count, schedule_agent_ids))
}

pub(super) fn insert_attention_wake_in_transaction(
    transaction: &Transaction<'_>,
    target_agent_id: &str,
    event_id: &str,
    now: i64,
) -> Result<String, CommunicationStoreError> {
    let wake_id = allocate_identity(
        transaction,
        AGENT_WAKE_ID_PREFIX,
        "SELECT EXISTS(SELECT 1 FROM wc_agent_wakes WHERE wake_id = ?1)",
    )?;
    transaction
        .execute(
            "INSERT INTO wc_agent_wakes (
                wake_id, target_agent_id, trigger_kind,
                first_triggering_delivery_id, latest_triggering_delivery_id,
                latest_conversation_id, latest_message_id,
                inbox_high_watermark, queued_delivery_count_snapshot,
                source_task_id, source_task_attempt_id, source_event_id,
                state, revision, created_at_unix_ms, updated_at_unix_ms,
                claimed_attempt_id, claimed_endpoint_id,
                claimed_controller_generation, claim_lease_expires_at_unix_ms,
                consumed_at_unix_ms, consumed_by_endpoint_id,
                consumed_controller_generation
             ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, NULL, NULL, NULL,
                       NULL, NULL, ?4, 'pending', 1, ?5, ?5,
                       NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
            params![
                wake_id,
                target_agent_id,
                WAKE_TRIGGER_ATTENTION_EVENT,
                event_id,
                now,
            ],
        )
        .map_err(store_error)?;
    Ok(wake_id)
}

pub(super) fn require_owned_attention_target(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    agent_id: &str,
) -> Result<(), CommunicationStoreError> {
    let owned = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM wc_agent_identities
                WHERE agent_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3
             )",
            params![agent_id, principal.kind, principal.digest],
            |row| row.get::<_, bool>(0),
        )
        .map_err(store_error)?;
    if !owned {
        return Err(CommunicationStoreError::new(
            "agent_attention_target_invariant",
            "Goal attention target is missing or unauthorized",
        ));
    }
    Ok(())
}

pub(crate) fn require_agent_attention_event_for_wake(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    source_event_id: Option<&str>,
    target_agent_id: &str,
) -> Result<AgentAttentionEventRecord, CommunicationStoreError> {
    let source_event_id = source_event_id.ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_attention_wake_invariant",
            "Agent attention Wake is missing source_event_id",
        )
    })?;
    let kind: Option<String> = conn.query_row(
        "SELECT kind FROM wc_agent_attention_events
         WHERE event_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3 AND target_agent_id = ?4",
        params![source_event_id, principal.kind, principal.digest, target_agent_id],
        |row| row.get(0),
    ).optional().map_err(store_error)?;
    if kind.as_deref() == Some(AGENT_ATTENTION_EVENT_KIND_GOAL_WORKFLOW_STALLED) {
        return super::goal_stall::require_goal_stall_event_for_wake(
            conn,
            principal,
            source_event_id,
            target_agent_id,
        );
    }
    let row = conn
        .query_row(
            "SELECT e.event_id, e.kind, e.owner_principal_kind, e.owner_principal_digest,
                    e.target_agent_id, e.goal_id, e.task_id, e.task_attempt_id,
                    e.terminal_task_state, e.created_at_unix_ms
             FROM wc_agent_attention_events e
             JOIN wc_goals g ON g.goal_id = e.goal_id
             JOIN wc_agent_tasks t ON t.task_id = e.task_id
             JOIN wc_agent_task_attempts a
               ON a.attempt_id = e.task_attempt_id AND a.task_id = e.task_id
             WHERE e.event_id = ?1
               AND e.kind = 'agent_task_terminal'
               AND e.owner_principal_kind = ?2 AND e.owner_principal_digest = ?3
               AND g.owner_principal_kind = ?2 AND g.owner_principal_digest = ?3
               AND t.owner_principal_kind = ?2 AND t.owner_principal_digest = ?3
               AND e.target_agent_id = ?4
               AND t.assignee_agent_id = a.assignee_agent_id
               AND EXISTS (
                   SELECT 1 FROM wc_agent_identities target
                   WHERE target.agent_id = e.target_agent_id
                     AND target.owner_principal_kind = ?2
                     AND target.owner_principal_digest = ?3
               )
               AND t.latest_attempt_id = e.task_attempt_id
               AND t.terminal_attempt_id = e.task_attempt_id
               AND t.state = e.terminal_task_state
               AND a.state = e.terminal_task_state
               AND e.terminal_task_state IN ('succeeded', 'failed')
               AND EXISTS (
                   SELECT 1 FROM wc_goal_correlations c
                   WHERE c.goal_id = e.goal_id AND c.kind = 'agent_task'
                     AND c.reference_id = e.task_id
               )",
            params![
                source_event_id,
                principal.kind,
                principal.digest,
                target_agent_id
            ],
            |row| {
                let terminal_task_state: String = row.get(8)?;
                Ok(AgentAttentionEventRecord {
                    event_id: row.get(0)?,
                    kind: row.get(1)?,
                    owner_principal_kind: row.get(2)?,
                    owner_principal_digest: row.get(3)?,
                    target_agent_id: row.get(4)?,
                    goal_id: row.get(5)?,
                    source: AgentAttentionSource::AgentTaskTerminal {
                        task_id: row.get(6)?,
                        task_attempt_id: row.get(7)?,
                        terminal_task_state: AgentTaskState::from_db(&terminal_task_state, 8)?,
                    },
                    created_at_unix_ms: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(store_error)?;
    row.ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_attention_wake_invariant",
            "Agent attention Wake source is missing, unauthorized, or inconsistent with authoritative Goal/Task state",
        )
    })
}

#[cfg(test)]
pub(crate) fn attention_events_for_attempt(
    conn: &Connection,
    task_attempt_id: &str,
) -> Result<Vec<AgentAttentionEventRecord>, CommunicationStoreError> {
    let mut statement = conn
        .prepare(
            "SELECT event_id, kind, owner_principal_kind, owner_principal_digest,
                    target_agent_id, goal_id, task_id, task_attempt_id,
                    terminal_task_state, created_at_unix_ms
             FROM wc_agent_attention_events
             WHERE task_attempt_id = ?1
             ORDER BY goal_id, event_id",
        )
        .map_err(store_error)?;
    let rows = statement
        .query_map(params![task_attempt_id], |row| {
            let terminal_task_state: String = row.get(8)?;
            Ok(AgentAttentionEventRecord {
                event_id: row.get(0)?,
                kind: row.get(1)?,
                owner_principal_kind: row.get(2)?,
                owner_principal_digest: row.get(3)?,
                target_agent_id: row.get(4)?,
                goal_id: row.get(5)?,
                source: AgentAttentionSource::AgentTaskTerminal {
                    task_id: row.get(6)?,
                    task_attempt_id: row.get(7)?,
                    terminal_task_state: AgentTaskState::from_db(&terminal_task_state, 8)?,
                },
                created_at_unix_ms: row.get(9)?,
            })
        })
        .map_err(store_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(store_error)
}

use super::agent_task::{AgentTaskState, AGENT_TASK_ID_PREFIX};
use super::agent_wake::{AgentWakeState, AGENT_WAKE_ID_PREFIX};
use super::communication::{
    allocate_identity, digest_json, digest_text, lookup_idempotent_resource, now_unix_ms,
    record_idempotent_resource, require_agent_owner, require_current_endpoint, store_error,
    validate_communication_principal, validate_id, validate_idempotency_key,
    CommunicationPrincipal, CommunicationStoreError,
};
use super::goal::GOAL_ID_PREFIX;
use super::Database;
use rusqlite::{
    params, types::Type, Connection, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeSet, HashSet};

pub const AGENT_WAIT_ID_PREFIX: &str = "wc_agent_wait_";
pub const MAX_AGENT_WAIT_SOURCES: usize = 8;
pub const MAX_ACTIVE_AGENT_WAITS_PER_AGENT: i64 = 32;
pub const MAX_AGENT_WAITS_PER_SOURCE: i64 = 32;
pub const MAX_GOAL_AGENT_WAIT_LIST_LIMIT: usize = 64;
pub const AGENT_WAIT_EVENT_KIND_AGENT_TASK_TERMINAL: &str = "agent_task_terminal";

const OP_WAIT_FOR_AGENT_EVENTS: &str = "wait_for_agent_events";
const OP_CANCEL_AGENT_WAIT: &str = "cancel_agent_wait";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentWaitState {
    Waiting,
    Triggered,
    Resumed,
    Cancelled,
}

impl AgentWaitState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Triggered => "triggered",
            Self::Resumed => "resumed",
            Self::Cancelled => "cancelled",
        }
    }

    fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "waiting" => Ok(Self::Waiting),
            "triggered" => Ok(Self::Triggered),
            "resumed" => Ok(Self::Resumed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported AgentWait state: {other}").into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentWaitMode {
    Any,
    All,
}

impl AgentWaitMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::All => "all",
        }
    }

    fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "any" => Ok(Self::Any),
            "all" => Ok(Self::All),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported AgentWait mode: {other}").into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWaitEventSelector {
    pub kind: String,
    pub task_id: String,
}

#[derive(Debug, Clone)]
pub struct NewAgentWait {
    pub target_agent_id: String,
    pub goal_id: Option<String>,
    pub endpoint_id: String,
    pub expected_controller_generation: i64,
    pub mode: AgentWaitMode,
    pub events: Vec<AgentWaitEventSelector>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWaitSourceRecord {
    pub ordinal: i64,
    pub kind: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWaitMatchRecord {
    pub sequence: i64,
    pub kind: String,
    pub task_id: String,
    pub task_attempt_id: String,
    pub terminal_task_state: AgentTaskState,
    pub occurred_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWaitDetail {
    pub wait_id: String,
    pub target_agent_id: String,
    pub goal_id: Option<String>,
    pub state: AgentWaitState,
    pub mode: AgentWaitMode,
    pub revision: i64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub triggered_at_unix_ms: Option<i64>,
    pub resumed_at_unix_ms: Option<i64>,
    pub cancelled_at_unix_ms: Option<i64>,
    pub source_count: usize,
    pub match_count: usize,
    pub match_sequence: i64,
    pub sources: Vec<AgentWaitSourceRecord>,
    pub matches: Vec<AgentWaitMatchRecord>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWaitMutation {
    pub agent_wait: AgentWaitDetail,
    pub replayed: bool,
    pub state_changed: bool,
    #[serde(skip_serializing)]
    pub schedule_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentWaitTerminalMatches {
    pub match_count: usize,
    pub schedule_agent_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentWaitWakeSnapshot {
    pub wait_id: String,
    pub target_agent_id: String,
    pub goal_id: Option<String>,
    pub state: AgentWaitState,
    pub mode: AgentWaitMode,
    pub source_count: i64,
    pub match_count: i64,
    pub match_sequence: i64,
}

impl Database {
    pub(super) fn ensure_agent_wait_schema(conn: &mut Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS wc_agent_waits (
                wait_id TEXT PRIMARY KEY,
                owner_principal_kind TEXT NOT NULL,
                owner_principal_digest TEXT NOT NULL,
                target_agent_id TEXT NOT NULL,
                goal_id TEXT,
                mode TEXT NOT NULL DEFAULT 'any' CHECK(mode IN ('any', 'all')),
                state TEXT NOT NULL CHECK(state IN ('waiting', 'triggered', 'resumed', 'cancelled')),
                revision INTEGER NOT NULL CHECK(revision >= 1),
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                triggered_at_unix_ms INTEGER,
                resumed_at_unix_ms INTEGER,
                cancelled_at_unix_ms INTEGER,
                FOREIGN KEY(target_agent_id) REFERENCES wc_agent_identities(agent_id),
                CHECK((state = 'waiting' AND triggered_at_unix_ms IS NULL AND resumed_at_unix_ms IS NULL AND cancelled_at_unix_ms IS NULL)
                   OR (state = 'triggered' AND triggered_at_unix_ms IS NOT NULL AND resumed_at_unix_ms IS NULL AND cancelled_at_unix_ms IS NULL)
                   OR (state = 'resumed' AND triggered_at_unix_ms IS NOT NULL AND resumed_at_unix_ms IS NOT NULL AND cancelled_at_unix_ms IS NULL)
                   OR (state = 'cancelled' AND resumed_at_unix_ms IS NULL AND cancelled_at_unix_ms IS NOT NULL))
            );
            CREATE INDEX IF NOT EXISTS idx_wc_agent_waits_owner_agent_state
                ON wc_agent_waits(owner_principal_kind, owner_principal_digest, target_agent_id, state, created_at_unix_ms);

            CREATE TABLE IF NOT EXISTS wc_agent_wait_sources (
                wait_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL CHECK(ordinal >= 0 AND ordinal < 8),
                kind TEXT NOT NULL CHECK(kind = 'agent_task_terminal'),
                task_id TEXT NOT NULL,
                PRIMARY KEY(wait_id, ordinal),
                UNIQUE(wait_id, kind, task_id),
                FOREIGN KEY(wait_id) REFERENCES wc_agent_waits(wait_id),
                FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_agent_wait_sources_task
                ON wc_agent_wait_sources(kind, task_id, wait_id);

            CREATE TABLE IF NOT EXISTS wc_agent_wait_matches (
                wait_id TEXT NOT NULL,
                sequence INTEGER NOT NULL CHECK(sequence >= 1 AND sequence <= 8),
                kind TEXT NOT NULL CHECK(kind = 'agent_task_terminal'),
                task_id TEXT NOT NULL,
                task_attempt_id TEXT NOT NULL,
                terminal_task_state TEXT NOT NULL CHECK(terminal_task_state IN ('succeeded', 'failed')),
                occurred_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(wait_id, sequence),
                UNIQUE(wait_id, kind, task_id),
                FOREIGN KEY(wait_id) REFERENCES wc_agent_waits(wait_id),
                FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id),
                FOREIGN KEY(task_attempt_id) REFERENCES wc_agent_task_attempts(attempt_id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_agent_wait_matches_wait
                ON wc_agent_wait_matches(wait_id, sequence);
            ",
        )?;
        let has_mode: i64 = conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('wc_agent_waits')
                WHERE name = 'mode'
            )",
            [],
            |row| row.get(0),
        )?;
        if has_mode == 0 {
            conn.execute(
                "ALTER TABLE wc_agent_waits
                 ADD COLUMN mode TEXT NOT NULL DEFAULT 'any' CHECK(mode IN ('any', 'all'))",
                [],
            )?;
        }
        let has_goal_id: i64 = conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('wc_agent_waits')
                WHERE name = 'goal_id'
            )",
            [],
            |row| row.get(0),
        )?;
        if has_goal_id == 0 {
            conn.execute("ALTER TABLE wc_agent_waits ADD COLUMN goal_id TEXT", [])?;
        }
        Ok(())
    }

    pub fn create_agent_wait(
        &self,
        principal: &CommunicationPrincipal,
        input: NewAgentWait,
    ) -> Result<AgentWaitMutation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(
            &input.target_agent_id,
            super::communication::DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        validate_id(
            &input.endpoint_id,
            super::communication::AGENT_ENDPOINT_ID_PREFIX,
            "invalid_endpoint_id",
        )?;
        if let Some(goal_id) = input.goal_id.as_deref() {
            validate_id(goal_id, GOAL_ID_PREFIX, "invalid_goal_id")?;
        }
        if input.expected_controller_generation < 1 {
            return Err(CommunicationStoreError::new(
                "invalid_controller_generation",
                "expected_controller_generation must be at least 1",
            ));
        }
        if input.events.is_empty() || input.events.len() > MAX_AGENT_WAIT_SOURCES {
            return Err(CommunicationStoreError::new(
                "invalid_agent_wait_events",
                format!("events must contain 1..={MAX_AGENT_WAIT_SOURCES} selectors"),
            ));
        }
        let mut unique_tasks = HashSet::new();
        for event in &input.events {
            if event.kind != AGENT_WAIT_EVENT_KIND_AGENT_TASK_TERMINAL {
                return Err(CommunicationStoreError::new(
                    "unsupported_agent_wait_event_kind",
                    "Durable Agent Wait v1 supports only agent_task_terminal",
                ));
            }
            validate_id(
                &event.task_id,
                AGENT_TASK_ID_PREFIX,
                "invalid_agent_task_id",
            )?;
            if !unique_tasks.insert(event.task_id.as_str()) {
                return Err(CommunicationStoreError::new(
                    "duplicate_agent_wait_event_selector",
                    "Durable Agent Wait event selectors must be unique",
                ));
            }
        }
        let idempotency_key = validate_idempotency_key(&input.idempotency_key)?;
        let request_hash_payload = match (input.mode, input.goal_id.as_deref()) {
            (AgentWaitMode::Any, None) => json!({
                "agent_id": input.target_agent_id,
                "endpoint_id": input.endpoint_id,
                "expected_controller_generation": input.expected_controller_generation,
                "events": input.events,
            }),
            (AgentWaitMode::All, None) => json!({
                "agent_id": input.target_agent_id,
                "endpoint_id": input.endpoint_id,
                "expected_controller_generation": input.expected_controller_generation,
                "events": input.events,
                "mode": "all",
            }),
            (mode, Some(goal_id)) => json!({
                "agent_id": input.target_agent_id,
                "endpoint_id": input.endpoint_id,
                "expected_controller_generation": input.expected_controller_generation,
                "events": input.events,
                "mode": mode.as_str(),
                "goal_id": goal_id,
            }),
        };
        let request_hash = digest_json("webcodex.agent-wait.request.v1", &request_hash_payload)
            .map_err(|_| {
                CommunicationStoreError::new(
                    "agent_wait_request_invalid",
                    "Agent Wait request could not be canonicalized",
                )
            })?;

        let mut conn = self.lock_connection(crate::StoreDomain::AgentWait);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        require_agent_owner(&transaction, principal, &input.target_agent_id)?;
        let _endpoint = require_current_endpoint(
            &transaction,
            principal,
            &input.target_agent_id,
            &input.endpoint_id,
            Some(input.expected_controller_generation),
        )?;
        if let Some(wait_id) = lookup_idempotent_resource(
            &transaction,
            principal,
            OP_WAIT_FOR_AGENT_EVENTS,
            &idempotency_key,
            &request_hash,
        )? {
            let agent_wait = load_owned_agent_wait_detail(&transaction, principal, &wait_id)?;
            transaction.commit().map_err(store_error)?;
            return Ok(AgentWaitMutation {
                agent_wait,
                replayed: true,
                state_changed: false,
                schedule_required: false,
            });
        }

        if let Some(goal_id) = input.goal_id.as_deref() {
            let goal: Option<(String, Option<String>)> = transaction
                .query_row(
                    "SELECT lifecycle, controller_agent_id
                     FROM wc_goals
                     WHERE goal_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3",
                    params![goal_id, principal.kind, principal.digest],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(store_error)?;
            let Some((lifecycle, controller_agent_id)) = goal else {
                return Err(CommunicationStoreError::new(
                    "goal_not_found",
                    "Goal does not exist",
                ));
            };
            if lifecycle != "active" {
                return Err(CommunicationStoreError::new(
                    "goal_scoped_agent_wait_goal_not_active",
                    "Goal-scoped AgentWait requires an active Goal",
                ));
            }
            let Some(controller_agent_id) = controller_agent_id else {
                return Err(CommunicationStoreError::new(
                    "goal_scoped_agent_wait_controller_required",
                    "Goal-scoped AgentWait requires the Goal to have an explicit controller_agent_id",
                ));
            };
            if controller_agent_id != input.target_agent_id {
                return Err(CommunicationStoreError::new(
                    "goal_scoped_agent_wait_controller_mismatch",
                    "Goal-scoped AgentWait target_agent_id must equal the Goal controller_agent_id",
                ));
            }
        }

        let active_count: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM wc_agent_waits
                 WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                   AND target_agent_id = ?3 AND state IN ('waiting', 'triggered')",
                params![principal.kind, principal.digest, input.target_agent_id],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        if active_count >= MAX_ACTIVE_AGENT_WAITS_PER_AGENT {
            return Err(CommunicationStoreError::new(
                "agent_wait_agent_capacity_reached",
                "Durable Agent active Wait capacity is exhausted",
            ));
        }

        let mut terminal_snapshots = Vec::with_capacity(input.events.len());
        for event in &input.events {
            let source_wait_count: i64 = transaction
                .query_row(
                    "SELECT COUNT(*)
                     FROM wc_agent_wait_sources s
                     JOIN wc_agent_waits w ON w.wait_id = s.wait_id
                     WHERE s.kind = 'agent_task_terminal' AND s.task_id = ?1
                       AND w.owner_principal_kind = ?2 AND w.owner_principal_digest = ?3
                       AND w.state IN ('waiting', 'triggered')",
                    params![event.task_id, principal.kind, principal.digest],
                    |row| row.get(0),
                )
                .map_err(store_error)?;
            if source_wait_count >= MAX_AGENT_WAITS_PER_SOURCE {
                return Err(CommunicationStoreError::new(
                    "agent_wait_source_capacity_reached",
                    "Durable Agent Wait source fanout capacity is exhausted",
                ));
            }
            let snapshot: Option<(String, Option<String>, Option<i64>)> = transaction
                .query_row(
                    "SELECT state, terminal_attempt_id, terminal_at_unix_ms
                     FROM wc_agent_tasks
                     WHERE task_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3",
                    params![event.task_id, principal.kind, principal.digest],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(store_error)?;
            let Some((state_text, terminal_attempt_id, terminal_at)) = snapshot else {
                return Err(CommunicationStoreError::new(
                    "agent_task_not_found",
                    "AgentTask does not exist",
                ));
            };
            let task_state = AgentTaskState::from_db(&state_text, 0).map_err(store_error)?;
            if let Some(goal_id) = input.goal_id.as_deref() {
                if task_state.terminal() {
                    return Err(CommunicationStoreError::new(
                        "goal_scoped_agent_wait_source_already_terminal",
                        "Goal-scoped rendezvous must be registered before selected source Tasks terminalize.",
                    ));
                }
                let correlated: bool = transaction
                    .query_row(
                        "SELECT EXISTS(
                            SELECT 1 FROM wc_goal_correlations
                            WHERE goal_id = ?1 AND kind = 'agent_task' AND reference_id = ?2
                         )",
                        params![goal_id, event.task_id],
                        |row| row.get(0),
                    )
                    .map_err(store_error)?;
                if !correlated {
                    return Err(CommunicationStoreError::new(
                        "goal_scoped_agent_wait_source_not_correlated",
                        "Every Goal-scoped AgentWait source must be explicitly correlated to the exact Goal",
                    ));
                }
            }
            terminal_snapshots.push((event.clone(), task_state, terminal_attempt_id, terminal_at));
        }

        let wait_id = allocate_identity(
            &transaction,
            AGENT_WAIT_ID_PREFIX,
            "SELECT EXISTS(SELECT 1 FROM wc_agent_waits WHERE wait_id = ?1)",
        )?;
        let now = now_unix_ms();
        transaction
            .execute(
                "INSERT INTO wc_agent_waits (
                    wait_id, owner_principal_kind, owner_principal_digest, target_agent_id, goal_id, mode,
                    state, revision, created_at_unix_ms, updated_at_unix_ms,
                    triggered_at_unix_ms, resumed_at_unix_ms, cancelled_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'waiting', 1, ?7, ?7, NULL, NULL, NULL)",
                params![
                    wait_id,
                    principal.kind,
                    principal.digest,
                    input.target_agent_id,
                    input.goal_id,
                    input.mode.as_str(),
                    now
                ],
            )
            .map_err(store_error)?;
        for (ordinal, event) in input.events.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO wc_agent_wait_sources (wait_id, ordinal, kind, task_id)
                     VALUES (?1, ?2, 'agent_task_terminal', ?3)",
                    params![wait_id, ordinal as i64, event.task_id],
                )
                .map_err(store_error)?;
        }

        let mut schedule_required = false;
        for (event, state, attempt_id, occurred_at) in terminal_snapshots {
            if !state.terminal() {
                continue;
            }
            let attempt_id = attempt_id.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "terminal AgentTask is missing terminal_attempt_id",
                )
            })?;
            schedule_required |= record_wait_match_in_transaction(
                &transaction,
                principal,
                &wait_id,
                &input.target_agent_id,
                &event.task_id,
                &attempt_id,
                state,
                occurred_at.unwrap_or(now),
            )?;
        }
        record_idempotent_resource(
            &transaction,
            principal,
            OP_WAIT_FOR_AGENT_EVENTS,
            &idempotency_key,
            &request_hash,
            &wait_id,
            now,
        )?;
        let agent_wait = load_owned_agent_wait_detail(&transaction, principal, &wait_id)?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentWaitMutation {
            agent_wait,
            replayed: false,
            state_changed: true,
            schedule_required,
        })
    }

    pub fn read_agent_wait(
        &self,
        principal: &CommunicationPrincipal,
        wait_id: &str,
    ) -> Result<AgentWaitDetail, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(wait_id, AGENT_WAIT_ID_PREFIX, "invalid_agent_wait_id")?;
        let conn = self.lock_connection(crate::StoreDomain::AgentWait);
        load_owned_agent_wait_detail(&conn, principal, wait_id)
    }

    pub fn list_agent_waits_for_goal(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        limit: usize,
    ) -> Result<(Vec<AgentWaitDetail>, bool), CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(goal_id, GOAL_ID_PREFIX, "invalid_goal_id")?;
        let limit = limit.clamp(1, MAX_GOAL_AGENT_WAIT_LIST_LIMIT);
        let conn = self.lock_connection(crate::StoreDomain::AgentWait);
        let mut stmt = conn
            .prepare(
                "SELECT wait_id FROM wc_agent_waits
                 WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2 AND goal_id = ?3
                 ORDER BY updated_at_unix_ms DESC, wait_id
                 LIMIT ?4",
            )
            .map_err(store_error)?;
        let ids = stmt
            .query_map(
                params![
                    principal.kind,
                    principal.digest,
                    goal_id,
                    (limit + 1) as i64
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store_error)?;
        let truncated = ids.len() > limit;
        let mut waits = Vec::with_capacity(ids.len().min(limit));
        for wait_id in ids.into_iter().take(limit) {
            waits.push(load_owned_agent_wait_detail(&conn, principal, &wait_id)?);
        }
        Ok((waits, truncated))
    }

    pub fn cancel_agent_wait(
        &self,
        principal: &CommunicationPrincipal,
        wait_id: &str,
        idempotency_key: &str,
    ) -> Result<AgentWaitMutation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(wait_id, AGENT_WAIT_ID_PREFIX, "invalid_agent_wait_id")?;
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        let request_hash = digest_text("webcodex.agent-wait.cancel.v1", wait_id);
        let mut conn = self.lock_connection(crate::StoreDomain::AgentWait);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        if let Some(resource) = lookup_idempotent_resource(
            &transaction,
            principal,
            OP_CANCEL_AGENT_WAIT,
            &idempotency_key,
            &request_hash,
        )? {
            if resource != wait_id {
                return Err(CommunicationStoreError::new(
                    "agent_wait_idempotency_invariant",
                    "Agent Wait cancellation replay points to a different Wait",
                ));
            }
            let agent_wait = load_owned_agent_wait_detail(&transaction, principal, wait_id)?;
            transaction.commit().map_err(store_error)?;
            return Ok(AgentWaitMutation {
                agent_wait,
                replayed: true,
                state_changed: false,
                schedule_required: false,
            });
        }
        let state = load_owned_agent_wait_detail(&transaction, principal, wait_id)?.state;
        let now = now_unix_ms();
        match state {
            AgentWaitState::Waiting => {}
            AgentWaitState::Triggered => {
                let wake: Option<(String, String, Option<String>)> = transaction
                    .query_row(
                        "SELECT wake_id, state, claimed_attempt_id FROM wc_agent_wakes
                         WHERE trigger_kind = 'agent_wait_events' AND source_wait_id = ?1",
                        [wait_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()
                    .map_err(store_error)?;
                let Some((wake_id, wake_state, claimed_attempt_id)) = wake else {
                    return Err(CommunicationStoreError::new(
                        "agent_wait_storage_invariant",
                        "triggered Agent Wait is missing its durable Wake",
                    ));
                };
                match wake_state.as_str() {
                    "pending" => {}
                    "claimed" => {
                        if let Some(attempt_id) = claimed_attempt_id {
                            transaction
                                .execute(
                                    "UPDATE wc_agent_wake_attempts
                                     SET state = 'revoked', revoked_at_unix_ms = ?2
                                     WHERE attempt_id = ?1 AND state = 'claimed'",
                                    params![attempt_id, now],
                                )
                                .map_err(store_error)?;
                        }
                    }
                    "prepared" | "delivered" | "delivery_unknown" => {
                        return Err(CommunicationStoreError::new(
                            "agent_wait_dispatch_fence_crossed",
                            "Agent Wait cannot be cancelled after Host dispatch preparation",
                        ));
                    }
                    "consumed" | "retired" => {
                        return Err(CommunicationStoreError::new(
                            "agent_wait_terminal",
                            "Agent Wait is already terminal",
                        ));
                    }
                    _ => {
                        return Err(CommunicationStoreError::new(
                            "agent_wait_storage_invariant",
                            "Agent Wait Wake has an unsupported state",
                        ));
                    }
                }
                transaction
                    .execute(
                        "UPDATE wc_agent_wakes
                         SET state = 'retired', revision = revision + 1,
                             updated_at_unix_ms = MAX(updated_at_unix_ms, ?2),
                             claimed_attempt_id = NULL, claimed_endpoint_id = NULL,
                             claimed_controller_generation = NULL, claim_lease_expires_at_unix_ms = NULL
                         WHERE wake_id = ?1 AND state IN ('pending', 'claimed')",
                        params![wake_id, now],
                    )
                    .map_err(store_error)?;
            }
            AgentWaitState::Resumed | AgentWaitState::Cancelled => {
                return Err(CommunicationStoreError::new(
                    "agent_wait_terminal",
                    "Agent Wait is already terminal",
                ));
            }
        }
        let updated = transaction
            .execute(
                "UPDATE wc_agent_waits
                 SET state = 'cancelled', revision = revision + 1,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?2), cancelled_at_unix_ms = ?2
                 WHERE wait_id = ?1 AND owner_principal_kind = ?3 AND owner_principal_digest = ?4
                   AND state IN ('waiting', 'triggered')",
                params![wait_id, now, principal.kind, principal.digest],
            )
            .map_err(store_error)?;
        if updated != 1 {
            return Err(CommunicationStoreError::new(
                "agent_wait_stale",
                "Agent Wait changed while cancellation was being applied",
            ));
        }
        record_idempotent_resource(
            &transaction,
            principal,
            OP_CANCEL_AGENT_WAIT,
            &idempotency_key,
            &request_hash,
            wait_id,
            now,
        )?;
        let agent_wait = load_owned_agent_wait_detail(&transaction, principal, wait_id)?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentWaitMutation {
            agent_wait,
            replayed: false,
            state_changed: true,
            schedule_required: false,
        })
    }
}

pub(crate) fn goal_scoped_wait_owns_terminal_attention_in_transaction(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    goal_id: &str,
    task_id: &str,
) -> Result<bool, CommunicationStoreError> {
    let active_owner_count: i64 = transaction
        .query_row(
            "SELECT COUNT(*)
             FROM wc_agent_wait_sources s
             JOIN wc_agent_waits w ON w.wait_id = s.wait_id
             WHERE s.kind = 'agent_task_terminal' AND s.task_id = ?1
               AND w.goal_id = ?2
               AND w.owner_principal_kind = ?3 AND w.owner_principal_digest = ?4
               AND w.state IN ('waiting', 'triggered')",
            params![task_id, goal_id, principal.kind, principal.digest],
            |row| row.get(0),
        )
        .map_err(store_error)?;
    if active_owner_count > MAX_AGENT_WAITS_PER_SOURCE {
        return Err(CommunicationStoreError::new(
            "agent_wait_source_capacity_invariant",
            "accepted Goal-scoped AgentWait source fanout exceeds the durable admission bound",
        ));
    }
    Ok(active_owner_count > 0)
}

pub(crate) fn record_agent_task_terminal_wait_matches_in_transaction(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    task_id: &str,
    task_attempt_id: &str,
    terminal_task_state: AgentTaskState,
    occurred_at_unix_ms: i64,
) -> Result<AgentWaitTerminalMatches, CommunicationStoreError> {
    if !terminal_task_state.terminal() {
        return Err(CommunicationStoreError::new(
            "invalid_agent_wait_terminal_state",
            "Agent Wait matches require terminal AgentTask state",
        ));
    }
    let mut statement = transaction
        .prepare(
            "SELECT w.wait_id, w.target_agent_id
             FROM wc_agent_wait_sources s
             JOIN wc_agent_waits w ON w.wait_id = s.wait_id
             WHERE s.kind = 'agent_task_terminal' AND s.task_id = ?1
               AND w.owner_principal_kind = ?2 AND w.owner_principal_digest = ?3
               AND w.state IN ('waiting', 'triggered')
             ORDER BY w.created_at_unix_ms, w.wait_id
             LIMIT ?4",
        )
        .map_err(store_error)?;
    let rows = statement
        .query_map(
            params![
                task_id,
                principal.kind,
                principal.digest,
                MAX_AGENT_WAITS_PER_SOURCE + 1
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(store_error)?;
    let mut waits = Vec::new();
    for row in rows {
        waits.push(row.map_err(store_error)?);
    }
    if waits.len() as i64 > MAX_AGENT_WAITS_PER_SOURCE {
        return Err(CommunicationStoreError::new(
            "agent_wait_source_capacity_invariant",
            "accepted Agent Wait source fanout exceeds the durable admission bound",
        ));
    }
    let mut schedule_agents = BTreeSet::new();
    let mut match_count = 0usize;
    for (wait_id, target_agent_id) in waits {
        if record_wait_match_in_transaction(
            transaction,
            principal,
            &wait_id,
            &target_agent_id,
            task_id,
            task_attempt_id,
            terminal_task_state,
            occurred_at_unix_ms,
        )? {
            schedule_agents.insert(target_agent_id);
        }
        match_count += 1;
    }
    Ok(AgentWaitTerminalMatches {
        match_count,
        schedule_agent_ids: schedule_agents.into_iter().collect(),
    })
}

fn record_wait_match_in_transaction(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    wait_id: &str,
    target_agent_id: &str,
    task_id: &str,
    task_attempt_id: &str,
    terminal_task_state: AgentTaskState,
    occurred_at_unix_ms: i64,
) -> Result<bool, CommunicationStoreError> {
    let wait: Option<(String, String, i64, i64, i64)> = transaction
        .query_row(
            "SELECT mode, state,
                    (SELECT COUNT(*) FROM wc_agent_wait_sources s WHERE s.wait_id = w.wait_id),
                    (SELECT COUNT(*) FROM wc_agent_wait_matches m WHERE m.wait_id = w.wait_id),
                    (SELECT COALESCE(MAX(sequence), 0) FROM wc_agent_wait_matches m WHERE m.wait_id = w.wait_id)
             FROM wc_agent_waits w
             WHERE wait_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3
               AND target_agent_id = ?4 AND state IN ('waiting', 'triggered')",
            params![wait_id, principal.kind, principal.digest, target_agent_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(store_error)?;
    let Some((mode_text, state_text, source_count, current_match_count, current_match_sequence)) =
        wait
    else {
        return Err(CommunicationStoreError::new(
            "agent_wait_storage_invariant",
            "Agent Wait match could not load the exact active Wait",
        ));
    };
    let mode = AgentWaitMode::from_db(&mode_text, 0).map_err(store_error)?;
    let state = AgentWaitState::from_db(&state_text, 1).map_err(store_error)?;
    validate_wait_join_invariants(
        mode,
        state,
        source_count,
        current_match_count,
        current_match_sequence,
    )?;
    validate_wait_match_source_membership(transaction, wait_id)?;

    let already_matched: i64 = transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM wc_agent_wait_matches
                WHERE wait_id = ?1 AND kind = 'agent_task_terminal' AND task_id = ?2
            )",
            params![wait_id, task_id],
            |row| row.get(0),
        )
        .map_err(store_error)?;
    if already_matched != 0 {
        return Ok(false);
    }

    let next_sequence = current_match_count + 1;
    if next_sequence > source_count || next_sequence > MAX_AGENT_WAIT_SOURCES as i64 {
        return Err(CommunicationStoreError::new(
            "agent_wait_match_capacity_invariant",
            "Agent Wait match count exceeds the registered source bound",
        ));
    }
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO wc_agent_wait_matches (
                wait_id, sequence, kind, task_id, task_attempt_id, terminal_task_state, occurred_at_unix_ms
             ) VALUES (?1, ?2, 'agent_task_terminal', ?3, ?4, ?5, ?6)",
            params![
                wait_id,
                next_sequence,
                task_id,
                task_attempt_id,
                terminal_task_state.as_str(),
                occurred_at_unix_ms,
            ],
        )
        .map_err(store_error)?;
    if inserted == 0 {
        return Ok(false);
    }

    let match_count = current_match_count + 1;
    let should_trigger = match mode {
        AgentWaitMode::Any => match_count >= 1,
        AgentWaitMode::All => match_count == source_count,
    };
    let updated = transaction
        .execute(
            "UPDATE wc_agent_waits
             SET state = CASE WHEN ?6 = 1 AND state = 'waiting' THEN 'triggered' ELSE state END,
                 revision = revision + 1,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?2),
                 triggered_at_unix_ms = CASE
                     WHEN ?6 = 1 THEN COALESCE(triggered_at_unix_ms, ?2)
                     ELSE triggered_at_unix_ms
                 END
             WHERE wait_id = ?1 AND owner_principal_kind = ?3 AND owner_principal_digest = ?4
               AND target_agent_id = ?5 AND state IN ('waiting', 'triggered')",
            params![
                wait_id,
                occurred_at_unix_ms,
                principal.kind,
                principal.digest,
                target_agent_id,
                should_trigger as i64,
            ],
        )
        .map_err(store_error)?;
    if updated != 1 {
        return Err(CommunicationStoreError::new(
            "agent_wait_storage_invariant",
            "Agent Wait match could not update the exact active Wait",
        ));
    }
    if !should_trigger {
        return Ok(false);
    }
    coalesce_wait_wake_in_transaction(
        transaction,
        wait_id,
        target_agent_id,
        match_count,
        next_sequence,
        occurred_at_unix_ms,
    )
}

fn validate_wait_match_source_membership(
    conn: &Connection,
    wait_id: &str,
) -> Result<(), CommunicationStoreError> {
    let orphan_match_count: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM wc_agent_wait_matches m
             WHERE m.wait_id = ?1
               AND NOT EXISTS (
                   SELECT 1 FROM wc_agent_wait_sources s
                   WHERE s.wait_id = m.wait_id AND s.kind = m.kind AND s.task_id = m.task_id
               )",
            [wait_id],
            |row| row.get(0),
        )
        .map_err(store_error)?;
    if orphan_match_count != 0 {
        return Err(CommunicationStoreError::new(
            "agent_wait_match_source_invariant",
            "Agent Wait contains a durable match that is not one of its registered sources",
        ));
    }
    Ok(())
}

fn coalesce_wait_wake_in_transaction(
    transaction: &Transaction<'_>,
    wait_id: &str,
    target_agent_id: &str,
    match_count: i64,
    match_sequence: i64,
    now: i64,
) -> Result<bool, CommunicationStoreError> {
    let existing: Option<(String, AgentWakeState)> = transaction
        .query_row(
            "SELECT wake_id, state FROM wc_agent_wakes
             WHERE trigger_kind = 'agent_wait_events' AND source_wait_id = ?1",
            [wait_id],
            |row| {
                Ok((
                    row.get(0)?,
                    AgentWakeState::from_db(&row.get::<_, String>(1)?, 1)?,
                ))
            },
        )
        .optional()
        .map_err(store_error)?;
    if let Some((wake_id, state)) = existing {
        if matches!(state, AgentWakeState::Pending | AgentWakeState::Claimed) {
            transaction
                .execute(
                    "UPDATE wc_agent_wakes
                     SET wait_match_count_snapshot = ?2, wait_match_sequence_snapshot = ?3,
                         revision = revision + 1, updated_at_unix_ms = MAX(updated_at_unix_ms, ?4)
                     WHERE wake_id = ?1 AND state IN ('pending', 'claimed')",
                    params![wake_id, match_count, match_sequence, now],
                )
                .map_err(store_error)?;
            return Ok(true);
        }
        return Ok(false);
    }
    let wake_id = allocate_identity(
        &transaction,
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
                source_wait_id, wait_match_count_snapshot, wait_match_sequence_snapshot,
                state, revision, created_at_unix_ms, updated_at_unix_ms
             ) VALUES (?1, ?2, 'agent_wait_events', NULL, NULL, NULL, NULL, NULL, NULL,
                       NULL, NULL, NULL, ?3, ?4, ?5, 'pending', 1, ?6, ?6)",
            params![
                wake_id,
                target_agent_id,
                wait_id,
                match_count,
                match_sequence,
                now
            ],
        )
        .map_err(store_error)?;
    Ok(true)
}

pub(crate) fn require_agent_wait_for_wake(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    wait_id: Option<&str>,
    target_agent_id: &str,
) -> Result<AgentWaitWakeSnapshot, CommunicationStoreError> {
    let wait_id = wait_id.ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_wait_wake_invariant",
            "Agent Wait Wake is missing source_wait_id",
        )
    })?;
    let row: Option<(String, Option<String>, String, String, i64, i64, i64)> = conn
        .query_row(
            "SELECT target_agent_id, goal_id, state, mode,
                    (SELECT COUNT(*) FROM wc_agent_wait_sources s WHERE s.wait_id = w.wait_id),
                    (SELECT COUNT(*) FROM wc_agent_wait_matches m WHERE m.wait_id = w.wait_id),
                    (SELECT COALESCE(MAX(sequence), 0) FROM wc_agent_wait_matches m WHERE m.wait_id = w.wait_id)
             FROM wc_agent_waits w
             WHERE wait_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3",
            params![wait_id, principal.kind, principal.digest],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()
        .map_err(store_error)?;
    let Some((
        stored_target,
        goal_id,
        state_text,
        mode_text,
        source_count,
        match_count,
        match_sequence,
    )) = row
    else {
        return Err(CommunicationStoreError::new(
            "agent_wait_not_found",
            "Agent Wait does not exist",
        ));
    };
    if stored_target != target_agent_id {
        return Err(CommunicationStoreError::new(
            "agent_wait_wake_invariant",
            "Agent Wait Wake target does not match its durable Wait",
        ));
    }
    if let Some(goal_id) = goal_id.as_deref() {
        validate_id(goal_id, GOAL_ID_PREFIX, "agent_wait_storage_invariant")?;
    }
    let state = AgentWaitState::from_db(&state_text, 2).map_err(store_error)?;
    let mode = AgentWaitMode::from_db(&mode_text, 3).map_err(store_error)?;
    validate_wait_join_invariants(mode, state, source_count, match_count, match_sequence)?;
    validate_wait_match_source_membership(conn, wait_id)?;
    if state != AgentWaitState::Triggered {
        return Err(CommunicationStoreError::new(
            "agent_wait_wake_stale",
            "Agent Wait Wake no longer belongs to a triggered Wait",
        ));
    }
    Ok(AgentWaitWakeSnapshot {
        wait_id: wait_id.to_string(),
        target_agent_id: stored_target,
        goal_id,
        state,
        mode,
        source_count,
        match_count,
        match_sequence,
    })
}

pub(crate) fn resume_agent_wait_for_wake_in_transaction(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    wait_id: Option<&str>,
    target_agent_id: &str,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let wait_id = wait_id.ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_wait_wake_invariant",
            "Agent Wait Wake is missing source_wait_id",
        )
    })?;
    require_agent_wait_for_wake(transaction, principal, Some(wait_id), target_agent_id)?;
    let updated = transaction
        .execute(
            "UPDATE wc_agent_waits
             SET state = 'resumed', revision = revision + 1,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?4), resumed_at_unix_ms = ?4
             WHERE wait_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3
               AND target_agent_id = ?5 AND state = 'triggered'",
            params![
                wait_id,
                principal.kind,
                principal.digest,
                now,
                target_agent_id
            ],
        )
        .map_err(store_error)?;
    if updated != 1 {
        return Err(CommunicationStoreError::new(
            "agent_wait_wake_stale",
            "Agent Wait is not the exact triggered Wait for this continuation",
        ));
    }
    Ok(())
}

pub(crate) fn verify_agent_wait_resumed_for_consumed_wake(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    wait_id: Option<&str>,
    target_agent_id: &str,
) -> Result<(), CommunicationStoreError> {
    let wait_id = wait_id.ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_wait_wake_invariant",
            "consumed Agent Wait Wake is missing source_wait_id",
        )
    })?;
    let detail = load_owned_agent_wait_detail(conn, principal, wait_id)?;
    if detail.state != AgentWaitState::Resumed {
        return Err(CommunicationStoreError::new(
            "agent_wait_storage_invariant",
            "consumed Agent Wait Wake does not correspond to a resumed Wait",
        ));
    }
    if detail.target_agent_id != target_agent_id {
        return Err(CommunicationStoreError::new(
            "agent_wait_wake_invariant",
            "consumed Agent Wait Wake target does not match the Wait",
        ));
    }
    Ok(())
}

fn load_owned_agent_wait_detail(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    wait_id: &str,
) -> Result<AgentWaitDetail, CommunicationStoreError> {
    let row: Option<(
        String,
        Option<String>,
        String,
        String,
        i64,
        i64,
        i64,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    )> = conn
        .query_row(
            "SELECT target_agent_id, goal_id, state, mode, revision, created_at_unix_ms, updated_at_unix_ms,
                    triggered_at_unix_ms, resumed_at_unix_ms, cancelled_at_unix_ms
             FROM wc_agent_waits
             WHERE wait_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3",
            params![wait_id, principal.kind, principal.digest],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .optional()
        .map_err(store_error)?;
    let Some((
        target_agent_id,
        goal_id,
        state_text,
        mode_text,
        revision,
        created_at,
        updated_at,
        triggered_at,
        resumed_at,
        cancelled_at,
    )) = row
    else {
        return Err(CommunicationStoreError::new(
            "agent_wait_not_found",
            "Agent Wait does not exist",
        ));
    };
    if let Some(goal_id) = goal_id.as_deref() {
        validate_id(goal_id, GOAL_ID_PREFIX, "agent_wait_storage_invariant")?;
    }
    let state = AgentWaitState::from_db(&state_text, 2).map_err(store_error)?;
    let mode = AgentWaitMode::from_db(&mode_text, 3).map_err(store_error)?;

    let mut source_stmt = conn
        .prepare(
            "SELECT ordinal, kind, task_id FROM wc_agent_wait_sources
             WHERE wait_id = ?1 ORDER BY ordinal",
        )
        .map_err(store_error)?;
    let source_rows = source_stmt
        .query_map([wait_id], |row| {
            Ok(AgentWaitSourceRecord {
                ordinal: row.get(0)?,
                kind: row.get(1)?,
                task_id: row.get(2)?,
            })
        })
        .map_err(store_error)?;
    let mut sources = Vec::new();
    for row in source_rows {
        sources.push(row.map_err(store_error)?);
    }
    if sources.len() > MAX_AGENT_WAIT_SOURCES {
        return Err(CommunicationStoreError::new(
            "agent_wait_source_capacity_invariant",
            "Agent Wait source count exceeds its durable bound",
        ));
    }

    let mut match_stmt = conn
        .prepare(
            "SELECT sequence, kind, task_id, task_attempt_id, terminal_task_state, occurred_at_unix_ms
             FROM wc_agent_wait_matches WHERE wait_id = ?1 ORDER BY sequence",
        )
        .map_err(store_error)?;
    let match_rows = match_stmt
        .query_map([wait_id], |row| {
            let terminal = AgentTaskState::from_db(&row.get::<_, String>(4)?, 4)?;
            Ok(AgentWaitMatchRecord {
                sequence: row.get(0)?,
                kind: row.get(1)?,
                task_id: row.get(2)?,
                task_attempt_id: row.get(3)?,
                terminal_task_state: terminal,
                occurred_at_unix_ms: row.get(5)?,
            })
        })
        .map_err(store_error)?;
    let mut matches = Vec::new();
    for row in match_rows {
        matches.push(row.map_err(store_error)?);
    }
    if matches.len() > MAX_AGENT_WAIT_SOURCES {
        return Err(CommunicationStoreError::new(
            "agent_wait_match_capacity_invariant",
            "Agent Wait match count exceeds its durable source bound",
        ));
    }
    let match_sequence = matches.last().map(|entry| entry.sequence).unwrap_or(0);
    validate_wait_match_source_membership(conn, wait_id)?;
    validate_wait_join_invariants(
        mode,
        state,
        sources.len() as i64,
        matches.len() as i64,
        match_sequence,
    )?;
    Ok(AgentWaitDetail {
        wait_id: wait_id.to_string(),
        target_agent_id,
        state,
        goal_id,
        mode,
        revision,
        created_at_unix_ms: created_at,
        updated_at_unix_ms: updated_at,
        triggered_at_unix_ms: triggered_at,
        resumed_at_unix_ms: resumed_at,
        cancelled_at_unix_ms: cancelled_at,
        source_count: sources.len(),
        match_count: matches.len(),
        match_sequence,
        sources,
        matches,
    })
}

fn validate_wait_join_invariants(
    mode: AgentWaitMode,
    state: AgentWaitState,
    source_count: i64,
    match_count: i64,
    match_sequence: i64,
) -> Result<(), CommunicationStoreError> {
    if !(1..=MAX_AGENT_WAIT_SOURCES as i64).contains(&source_count) {
        return Err(CommunicationStoreError::new(
            "agent_wait_source_capacity_invariant",
            "Agent Wait source count is outside its durable bound",
        ));
    }
    if match_count < 0 || match_count > source_count {
        return Err(CommunicationStoreError::new(
            "agent_wait_match_capacity_invariant",
            "Agent Wait match count exceeds its durable source bound",
        ));
    }
    if match_sequence != match_count {
        return Err(CommunicationStoreError::new(
            "agent_wait_match_sequence_invariant",
            "Agent Wait match sequence is not contiguous with its durable match count",
        ));
    }
    let valid = match (mode, state) {
        (AgentWaitMode::Any, AgentWaitState::Waiting) => match_count == 0,
        (AgentWaitMode::Any, AgentWaitState::Triggered | AgentWaitState::Resumed) => {
            match_count >= 1
        }
        (AgentWaitMode::All, AgentWaitState::Waiting) => match_count < source_count,
        (AgentWaitMode::All, AgentWaitState::Triggered | AgentWaitState::Resumed) => {
            match_count == source_count
        }
        (_, AgentWaitState::Cancelled) => true,
    };
    if !valid {
        return Err(CommunicationStoreError::new(
            "agent_wait_join_invariant",
            "Agent Wait mode/state does not match its durable source and match counts",
        ));
    }
    Ok(())
}

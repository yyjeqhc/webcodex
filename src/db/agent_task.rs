use super::communication::{
    authorize_conversation_access, digest_text, lookup_idempotent_resource, new_id, now_unix_ms,
    record_idempotent_resource, store_error, validate_communication_principal, validate_id,
    validate_idempotency_key, CommunicationPrincipal, CommunicationStoreError, ConversationAccess,
    CONVERSATION_ID_PREFIX, CONVERSATION_MESSAGE_ID_PREFIX, DURABLE_AGENT_ID_PREFIX,
};
use super::Database;
use rusqlite::{
    params, types::Type, Connection, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::Serialize;
use serde_json::json;

pub(crate) const AGENT_TASK_ID_PREFIX: &str = "wc_agent_task_";
pub(crate) const AGENT_TASK_ATTEMPT_ID_PREFIX: &str = "wc_agent_task_attempt_";
pub(crate) const AGENT_TASK_ATTEMPT_FENCE_PREFIX: &str = "wc_agent_task_fence_";

pub(crate) const MAX_AGENT_TASK_TITLE_CHARS: usize = 200;
pub(crate) const MAX_AGENT_TASK_INSTRUCTION_BYTES: usize = 8_192;
pub(crate) const MAX_AGENT_TASK_TERMINAL_TEXT_BYTES: usize = 4_096;
pub(crate) const MAX_AGENT_TASK_PROJECT_REF_CHARS: usize = 256;
pub(crate) const MAX_AGENT_TASK_LIST_LIMIT: usize = 100;
pub(crate) const DEFAULT_AGENT_TASK_ATTEMPT_LEASE_MS: i64 = 60_000;

const OP_CREATE_AGENT_TASK: &str = "create_agent_task";
const OP_START_AGENT_TASK_ATTEMPT: &str = "start_agent_task_attempt";
const OP_COMPLETE_AGENT_TASK_ATTEMPT: &str = "complete_agent_task_attempt";

#[derive(Debug, Clone)]
pub(crate) struct NewAgentTask {
    pub(crate) title: String,
    pub(crate) instruction: String,
    pub(crate) assignee_agent_id: Option<String>,
    pub(crate) source_conversation_id: Option<String>,
    pub(crate) source_message_id: Option<String>,
    pub(crate) referenced_project_id: Option<String>,
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentTaskState {
    Ready,
    Active,
    Succeeded,
    Failed,
}

impl AgentTaskState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "ready" => Ok(Self::Ready),
            "active" => Ok(Self::Active),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported AgentTask state: {other}").into(),
            )),
        }
    }

    const fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentTaskAttemptState {
    Active,
    Expired,
    Succeeded,
    Failed,
}

impl AgentTaskAttemptState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "expired" => Ok(Self::Expired),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported AgentTaskAttempt state: {other}").into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AgentTaskAttemptRecord {
    pub(crate) attempt_id: String,
    pub(crate) task_id: String,
    pub(crate) attempt_number: i64,
    pub(crate) assignee_agent_id: String,
    pub(crate) state: AgentTaskAttemptState,
    pub(crate) lease_expires_at_unix_ms: i64,
    pub(crate) lease_active: bool,
    pub(crate) attempt_controller_generation: i64,
    pub(crate) created_at_unix_ms: i64,
    pub(crate) started_at_unix_ms: i64,
    pub(crate) terminal_at_unix_ms: Option<i64>,
    pub(crate) terminal_result: Option<String>,
    pub(crate) terminal_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AgentTaskSummary {
    pub(crate) task_id: String,
    pub(crate) assignee_agent_id: Option<String>,
    pub(crate) title: String,
    pub(crate) source_conversation_id: Option<String>,
    pub(crate) source_message_id: Option<String>,
    pub(crate) referenced_project_id: Option<String>,
    pub(crate) state: AgentTaskState,
    pub(crate) created_at_unix_ms: i64,
    pub(crate) updated_at_unix_ms: i64,
    pub(crate) terminal_at_unix_ms: Option<i64>,
    pub(crate) latest_attempt: Option<AgentTaskAttemptRecord>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AgentTaskDetail {
    pub(crate) summary: AgentTaskSummary,
    pub(crate) instruction: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AgentTaskMutation {
    pub(crate) task: AgentTaskDetail,
    pub(crate) created: bool,
    pub(crate) replayed: bool,
    pub(crate) state_changed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AgentTaskPage {
    pub(crate) total_count: i64,
    pub(crate) offset: usize,
    pub(crate) next_offset: Option<usize>,
    pub(crate) truncated: bool,
    pub(crate) tasks: Vec<AgentTaskSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AgentTaskAttemptStartMutation {
    pub(crate) task: AgentTaskSummary,
    pub(crate) attempt: AgentTaskAttemptRecord,
    pub(crate) attempt_fence: String,
    pub(crate) replayed: bool,
    pub(crate) state_changed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AgentTaskAttemptHeartbeatMutation {
    pub(crate) task: AgentTaskSummary,
    pub(crate) attempt: AgentTaskAttemptRecord,
    pub(crate) state_changed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AgentTaskAttemptCompletionMutation {
    pub(crate) task: AgentTaskSummary,
    pub(crate) attempt: AgentTaskAttemptRecord,
    pub(crate) replayed: bool,
    pub(crate) state_changed: bool,
}

#[derive(Debug, Clone)]
struct StoredAttempt {
    attempt_id: String,
    task_id: String,
    attempt_number: i64,
    assignee_agent_id: String,
    stored_state: AgentTaskAttemptState,
    lease_expires_at_unix_ms: i64,
    attempt_fence: String,
    attempt_controller_generation: i64,
    created_at_unix_ms: i64,
    started_at_unix_ms: i64,
    terminal_at_unix_ms: Option<i64>,
    terminal_result: Option<String>,
    terminal_reason: Option<String>,
}

impl StoredAttempt {
    fn effective_state(&self, now: i64) -> AgentTaskAttemptState {
        if self.stored_state == AgentTaskAttemptState::Active
            && self.lease_expires_at_unix_ms <= now
        {
            AgentTaskAttemptState::Expired
        } else {
            self.stored_state
        }
    }

    fn record(&self, now: i64) -> AgentTaskAttemptRecord {
        let state = self.effective_state(now);
        AgentTaskAttemptRecord {
            attempt_id: self.attempt_id.clone(),
            task_id: self.task_id.clone(),
            attempt_number: self.attempt_number,
            assignee_agent_id: self.assignee_agent_id.clone(),
            state,
            lease_expires_at_unix_ms: self.lease_expires_at_unix_ms,
            lease_active: state == AgentTaskAttemptState::Active,
            attempt_controller_generation: self.attempt_controller_generation,
            created_at_unix_ms: self.created_at_unix_ms,
            started_at_unix_ms: self.started_at_unix_ms,
            terminal_at_unix_ms: self.terminal_at_unix_ms,
            terminal_result: self.terminal_result.clone(),
            terminal_reason: self.terminal_reason.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct StoredTask {
    task_id: String,
    assignee_agent_id: Option<String>,
    title: String,
    instruction: String,
    source_conversation_id: Option<String>,
    source_message_id: Option<String>,
    referenced_project_id: Option<String>,
    stored_state: AgentTaskState,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
    terminal_at_unix_ms: Option<i64>,
    latest_attempt: Option<StoredAttempt>,
}

impl StoredTask {
    fn effective_state(&self, now: i64) -> AgentTaskState {
        if self.stored_state == AgentTaskState::Active
            && self.latest_attempt.as_ref().is_some_and(|attempt| {
                attempt.effective_state(now) == AgentTaskAttemptState::Expired
            })
        {
            AgentTaskState::Ready
        } else {
            self.stored_state
        }
    }

    fn summary(&self, now: i64) -> AgentTaskSummary {
        AgentTaskSummary {
            task_id: self.task_id.clone(),
            assignee_agent_id: self.assignee_agent_id.clone(),
            title: self.title.clone(),
            source_conversation_id: self.source_conversation_id.clone(),
            source_message_id: self.source_message_id.clone(),
            referenced_project_id: self.referenced_project_id.clone(),
            state: self.effective_state(now),
            created_at_unix_ms: self.created_at_unix_ms,
            updated_at_unix_ms: self.updated_at_unix_ms,
            terminal_at_unix_ms: self.terminal_at_unix_ms,
            latest_attempt: self
                .latest_attempt
                .as_ref()
                .map(|attempt| attempt.record(now)),
        }
    }

    fn detail(&self, now: i64) -> AgentTaskDetail {
        AgentTaskDetail {
            summary: self.summary(now),
            instruction: self.instruction.clone(),
        }
    }
}

impl Database {
    pub(crate) fn ensure_agent_task_schema(conn: &mut Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS wc_agent_tasks (
                task_id TEXT PRIMARY KEY,
                owner_principal_kind TEXT NOT NULL,
                owner_principal_digest TEXT NOT NULL,
                assignee_agent_id TEXT,
                title TEXT NOT NULL,
                instruction TEXT NOT NULL,
                source_conversation_id TEXT,
                source_message_id TEXT,
                referenced_project_id TEXT,
                state TEXT NOT NULL CHECK(state IN ('ready', 'active', 'succeeded', 'failed')),
                latest_attempt_id TEXT,
                terminal_attempt_id TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                terminal_at_unix_ms INTEGER,
                CHECK(
                    (state IN ('ready', 'active') AND terminal_attempt_id IS NULL AND terminal_at_unix_ms IS NULL)
                    OR (state IN ('succeeded', 'failed') AND terminal_attempt_id IS NOT NULL AND terminal_at_unix_ms IS NOT NULL)
                ),
                FOREIGN KEY(assignee_agent_id) REFERENCES wc_agent_identities(agent_id),
                FOREIGN KEY(source_conversation_id) REFERENCES wc_conversations(conversation_id),
                FOREIGN KEY(source_message_id) REFERENCES wc_conversation_messages(message_id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_agent_tasks_owner_updated
                ON wc_agent_tasks(owner_principal_digest, updated_at_unix_ms DESC, task_id);
            CREATE INDEX IF NOT EXISTS idx_wc_agent_tasks_assignee
                ON wc_agent_tasks(owner_principal_digest, assignee_agent_id, updated_at_unix_ms DESC);

            CREATE TABLE IF NOT EXISTS wc_agent_task_attempts (
                attempt_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                attempt_number INTEGER NOT NULL CHECK(attempt_number >= 1),
                assignee_agent_id TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('active', 'expired', 'succeeded', 'failed')),
                lease_expires_at_unix_ms INTEGER NOT NULL,
                attempt_fence TEXT NOT NULL UNIQUE,
                attempt_controller_generation INTEGER NOT NULL CHECK(attempt_controller_generation >= 1),
                created_at_unix_ms INTEGER NOT NULL,
                started_at_unix_ms INTEGER NOT NULL,
                terminal_at_unix_ms INTEGER,
                terminal_result TEXT,
                terminal_reason TEXT,
                CHECK(
                    (state = 'active' AND terminal_at_unix_ms IS NULL)
                    OR (state != 'active' AND terminal_at_unix_ms IS NOT NULL)
                ),
                UNIQUE(task_id, attempt_number),
                FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id),
                FOREIGN KEY(assignee_agent_id) REFERENCES wc_agent_identities(agent_id)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_wc_agent_task_attempts_one_active
                ON wc_agent_task_attempts(task_id) WHERE state = 'active';
            CREATE INDEX IF NOT EXISTS idx_wc_agent_task_attempts_task_number
                ON wc_agent_task_attempts(task_id, attempt_number DESC);
            CREATE INDEX IF NOT EXISTS idx_wc_agent_task_attempts_assignee
                ON wc_agent_task_attempts(assignee_agent_id, state, lease_expires_at_unix_ms);
            ",
        )?;
        Ok(())
    }

    pub(crate) fn create_agent_task(
        &self,
        principal: &CommunicationPrincipal,
        input: NewAgentTask,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        self.create_agent_task_at(principal, input, now_unix_ms())
    }

    pub(crate) fn create_agent_task_at(
        &self,
        principal: &CommunicationPrincipal,
        input: NewAgentTask,
        now: i64,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        let title = validate_title(&input.title)?;
        let instruction = validate_instruction(&input.instruction)?;
        let assignee_agent_id = validate_optional_id(
            input.assignee_agent_id.as_deref(),
            DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        let source_conversation_id = validate_optional_id(
            input.source_conversation_id.as_deref(),
            CONVERSATION_ID_PREFIX,
            "invalid_conversation_id",
        )?;
        let source_message_id = validate_optional_id(
            input.source_message_id.as_deref(),
            CONVERSATION_MESSAGE_ID_PREFIX,
            "invalid_message_id",
        )?;
        if source_message_id.is_some() && source_conversation_id.is_none() {
            return Err(CommunicationStoreError::new(
                "agent_task_source_invalid",
                "source_message_id requires source_conversation_id",
            ));
        }
        let referenced_project_id =
            validate_project_reference(input.referenced_project_id.as_deref())?;
        let idempotency_key = validate_idempotency_key(&input.idempotency_key)?;
        let request_hash = task_request_hash(&json!({
            "title": title,
            "instruction": instruction,
            "assignee_agent_id": assignee_agent_id,
            "source_conversation_id": source_conversation_id,
            "source_message_id": source_message_id,
            "referenced_project_id": referenced_project_id,
        }));

        let mut conn = self.conn.lock().unwrap();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;

        if let Some(task_id) = lookup_idempotent_resource(
            &transaction,
            principal,
            OP_CREATE_AGENT_TASK,
            &idempotency_key,
            &request_hash,
        )? {
            let task = load_owned_task(&transaction, principal, &task_id, now)?;
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskMutation {
                task: task.detail(now),
                created: false,
                replayed: true,
                state_changed: false,
            });
        }

        if let Some(agent_id) = assignee_agent_id.as_deref() {
            require_owned_agent(&transaction, principal, agent_id)?;
        }
        validate_source_references(
            &transaction,
            principal,
            source_conversation_id.as_deref(),
            source_message_id.as_deref(),
        )?;

        let task_id = new_id(AGENT_TASK_ID_PREFIX);
        transaction
            .execute(
                "INSERT INTO wc_agent_tasks (
                    task_id, owner_principal_kind, owner_principal_digest,
                    assignee_agent_id, title, instruction, source_conversation_id,
                    source_message_id, referenced_project_id, state, latest_attempt_id,
                    terminal_attempt_id, created_at_unix_ms, updated_at_unix_ms,
                    terminal_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'ready', NULL, NULL, ?10, ?10, NULL)",
                params![
                    task_id,
                    principal.kind,
                    principal.digest,
                    assignee_agent_id,
                    title,
                    instruction,
                    source_conversation_id,
                    source_message_id,
                    referenced_project_id,
                    now,
                ],
            )
            .map_err(store_error)?;
        record_idempotent_resource(
            &transaction,
            principal,
            OP_CREATE_AGENT_TASK,
            &idempotency_key,
            &request_hash,
            &task_id,
            now,
        )?;
        let task = load_owned_task(&transaction, principal, &task_id, now)?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskMutation {
            task: task.detail(now),
            created: true,
            replayed: false,
            state_changed: true,
        })
    }

    pub(crate) fn list_agent_tasks(
        &self,
        principal: &CommunicationPrincipal,
        assignee_agent_id: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<AgentTaskPage, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        let assignee_agent_id = validate_optional_id(
            assignee_agent_id,
            DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        if limit == 0 || limit > MAX_AGENT_TASK_LIST_LIMIT {
            return Err(CommunicationStoreError::new(
                "invalid_agent_task_list_limit",
                format!("limit must be 1..={MAX_AGENT_TASK_LIST_LIMIT}"),
            ));
        }
        let now = now_unix_ms();
        let conn = self.conn.lock().unwrap();
        let (total_count, task_ids) = if let Some(agent_id) = assignee_agent_id.as_deref() {
            let total_count = conn
                .query_row(
                    "SELECT COUNT(*) FROM wc_agent_tasks
                     WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                       AND assignee_agent_id = ?3",
                    params![principal.kind, principal.digest, agent_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(store_error)?;
            let mut statement = conn
                .prepare(
                    "SELECT task_id FROM wc_agent_tasks
                     WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                       AND assignee_agent_id = ?3
                     ORDER BY updated_at_unix_ms DESC, task_id
                     LIMIT ?4 OFFSET ?5",
                )
                .map_err(store_error)?;
            let ids = statement
                .query_map(
                    params![
                        principal.kind,
                        principal.digest,
                        agent_id,
                        limit as i64,
                        offset as i64
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(store_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(store_error)?;
            (total_count, ids)
        } else {
            let total_count = conn
                .query_row(
                    "SELECT COUNT(*) FROM wc_agent_tasks
                     WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2",
                    params![principal.kind, principal.digest],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(store_error)?;
            let mut statement = conn
                .prepare(
                    "SELECT task_id FROM wc_agent_tasks
                     WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                     ORDER BY updated_at_unix_ms DESC, task_id
                     LIMIT ?3 OFFSET ?4",
                )
                .map_err(store_error)?;
            let ids = statement
                .query_map(
                    params![
                        principal.kind,
                        principal.digest,
                        limit as i64,
                        offset as i64
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(store_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(store_error)?;
            (total_count, ids)
        };
        let tasks = task_ids
            .iter()
            .map(|task_id| {
                load_owned_task(&conn, principal, task_id, now).map(|task| task.summary(now))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next_offset = if offset.saturating_add(tasks.len()) < total_count as usize {
            Some(offset.saturating_add(tasks.len()))
        } else {
            None
        };
        Ok(AgentTaskPage {
            total_count,
            offset,
            next_offset,
            truncated: next_offset.is_some(),
            tasks,
        })
    }

    pub(crate) fn read_agent_task(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
    ) -> Result<AgentTaskDetail, CommunicationStoreError> {
        self.read_agent_task_at(principal, task_id, now_unix_ms())
    }

    pub(crate) fn read_agent_task_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        now: i64,
    ) -> Result<AgentTaskDetail, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
        let conn = self.conn.lock().unwrap();
        Ok(load_owned_task(&conn, principal, task_id, now)?.detail(now))
    }

    pub(crate) fn assign_agent_task(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        self.assign_agent_task_with_now(principal, task_id, assignee_agent_id, None)
    }

    #[cfg(test)]
    pub(crate) fn assign_agent_task_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        now: i64,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        self.assign_agent_task_with_now(principal, task_id, assignee_agent_id, Some(now))
    }

    fn assign_agent_task_with_now(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        now: Option<i64>,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
        validate_id(
            assignee_agent_id,
            DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        let mut conn = self.conn.lock().unwrap();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let mut task = load_owned_task(&transaction, principal, task_id, now)?;
        if task.stored_state.terminal() {
            return Err(task_terminal_error());
        }
        require_owned_agent(&transaction, principal, assignee_agent_id)?;
        materialize_expired_latest_attempt(&transaction, &mut task, now)?;
        if task
            .latest_attempt
            .as_ref()
            .is_some_and(|attempt| attempt.stored_state == AgentTaskAttemptState::Active)
        {
            if task.assignee_agent_id.as_deref() == Some(assignee_agent_id) {
                transaction.commit().map_err(store_error)?;
                return Ok(AgentTaskMutation {
                    task: task.detail(now),
                    created: false,
                    replayed: false,
                    state_changed: false,
                });
            }
            return Err(CommunicationStoreError::new(
                "agent_task_attempt_active",
                "AgentTask has an unexpired active Attempt; reassignment is fenced",
            ));
        }
        if task.assignee_agent_id.as_deref() == Some(assignee_agent_id) {
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskMutation {
                task: task.detail(now),
                created: false,
                replayed: false,
                state_changed: false,
            });
        }
        transaction
            .execute(
                "UPDATE wc_agent_tasks
                 SET assignee_agent_id = ?2, state = 'ready', updated_at_unix_ms = ?3
                 WHERE task_id = ?1",
                params![task_id, assignee_agent_id, now.max(task.updated_at_unix_ms)],
            )
            .map_err(store_error)?;
        task = load_owned_task(&transaction, principal, task_id, now)?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskMutation {
            task: task.detail(now),
            created: false,
            replayed: false,
            state_changed: true,
        })
    }

    pub(crate) fn start_agent_task_attempt(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        idempotency_key: &str,
    ) -> Result<AgentTaskAttemptStartMutation, CommunicationStoreError> {
        self.start_agent_task_attempt_with_now(
            principal,
            task_id,
            assignee_agent_id,
            idempotency_key,
            None,
        )
    }

    #[cfg(test)]
    pub(crate) fn start_agent_task_attempt_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        idempotency_key: &str,
        now: i64,
    ) -> Result<AgentTaskAttemptStartMutation, CommunicationStoreError> {
        self.start_agent_task_attempt_with_now(
            principal,
            task_id,
            assignee_agent_id,
            idempotency_key,
            Some(now),
        )
    }

    fn start_agent_task_attempt_with_now(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        idempotency_key: &str,
        now: Option<i64>,
    ) -> Result<AgentTaskAttemptStartMutation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
        validate_id(
            assignee_agent_id,
            DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        let request_hash = task_request_hash(&json!({
            "task_id": task_id,
            "assignee_agent_id": assignee_agent_id,
        }));
        let mut conn = self.conn.lock().unwrap();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let mut task = load_owned_task(&transaction, principal, task_id, now)?;

        if let Some(attempt_id) = lookup_idempotent_resource(
            &transaction,
            principal,
            OP_START_AGENT_TASK_ATTEMPT,
            &idempotency_key,
            &request_hash,
        )? {
            let attempt = load_attempt_for_task(&transaction, task_id, &attempt_id, now)?
                .ok_or_else(|| {
                    CommunicationStoreError::new(
                        "agent_task_attempt_not_found",
                        "AgentTaskAttempt does not exist",
                    )
                })?;
            let record = attempt.record(now);
            let fence = attempt.attempt_fence.clone();
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskAttemptStartMutation {
                task: task.summary(now),
                attempt: record,
                attempt_fence: fence,
                replayed: true,
                state_changed: false,
            });
        }

        if task.stored_state.terminal() {
            return Err(task_terminal_error());
        }
        let current_assignee = task.assignee_agent_id.as_deref().ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_unassigned",
                "AgentTask must have an explicit assignee before an Attempt can start",
            )
        })?;
        if current_assignee != assignee_agent_id {
            return Err(CommunicationStoreError::new(
                "agent_task_assignee_mismatch",
                "Requested Agent does not match the AgentTask current assignee",
            ));
        }
        require_owned_agent(&transaction, principal, assignee_agent_id)?;
        materialize_expired_latest_attempt(&transaction, &mut task, now)?;
        if task
            .latest_attempt
            .as_ref()
            .is_some_and(|attempt| attempt.stored_state == AgentTaskAttemptState::Active)
        {
            return Err(CommunicationStoreError::new(
                "agent_task_attempt_active",
                "AgentTask already has an unexpired authoritative Attempt",
            ));
        }

        let attempt_number = task
            .latest_attempt
            .as_ref()
            .map(|attempt| attempt.attempt_number.saturating_add(1))
            .unwrap_or(1);
        let attempt_id = new_id(AGENT_TASK_ATTEMPT_ID_PREFIX);
        let attempt_fence = new_id(AGENT_TASK_ATTEMPT_FENCE_PREFIX);
        let lease_expires_at = now.saturating_add(DEFAULT_AGENT_TASK_ATTEMPT_LEASE_MS);
        transaction
            .execute(
                "INSERT INTO wc_agent_task_attempts (
                    attempt_id, task_id, attempt_number, assignee_agent_id, state,
                    lease_expires_at_unix_ms, attempt_fence, attempt_controller_generation,
                    created_at_unix_ms, started_at_unix_ms, terminal_at_unix_ms,
                    terminal_result, terminal_reason
                 ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, 1, ?7, ?7, NULL, NULL, NULL)",
                params![
                    attempt_id,
                    task_id,
                    attempt_number,
                    assignee_agent_id,
                    lease_expires_at,
                    attempt_fence,
                    now,
                ],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_tasks
                 SET state = 'active', latest_attempt_id = ?2, updated_at_unix_ms = ?3
                 WHERE task_id = ?1",
                params![task_id, attempt_id, now.max(task.updated_at_unix_ms)],
            )
            .map_err(store_error)?;
        record_idempotent_resource(
            &transaction,
            principal,
            OP_START_AGENT_TASK_ATTEMPT,
            &idempotency_key,
            &request_hash,
            &attempt_id,
            now,
        )?;
        task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt =
            load_attempt_for_task(&transaction, task_id, &attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_attempt_not_found",
                    "AgentTaskAttempt disappeared after creation",
                )
            })?;
        let record = attempt.record(now);
        let fence = attempt.attempt_fence.clone();
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskAttemptStartMutation {
            task: task.summary(now),
            attempt: record,
            attempt_fence: fence,
            replayed: false,
            state_changed: true,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn heartbeat_agent_task_attempt(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
    ) -> Result<AgentTaskAttemptHeartbeatMutation, CommunicationStoreError> {
        self.heartbeat_agent_task_attempt_with_now(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            None,
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn heartbeat_agent_task_attempt_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        now: i64,
    ) -> Result<AgentTaskAttemptHeartbeatMutation, CommunicationStoreError> {
        self.heartbeat_agent_task_attempt_with_now(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            Some(now),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn heartbeat_agent_task_attempt_with_now(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        now: Option<i64>,
    ) -> Result<AgentTaskAttemptHeartbeatMutation, CommunicationStoreError> {
        validate_attempt_mutation_inputs(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        let mut conn = self.conn.lock().unwrap();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt = require_current_attempt(
            &transaction,
            &task,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            now,
        )?;
        let new_lease_expires_at = attempt
            .lease_expires_at_unix_ms
            .max(now.saturating_add(DEFAULT_AGENT_TASK_ATTEMPT_LEASE_MS));
        transaction
            .execute(
                "UPDATE wc_agent_task_attempts
                 SET lease_expires_at_unix_ms = ?2
                 WHERE attempt_id = ?1",
                params![attempt_id, new_lease_expires_at],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_tasks SET updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
                 WHERE task_id = ?1",
                params![task_id, now],
            )
            .map_err(store_error)?;
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt =
            load_attempt_for_task(&transaction, task_id, attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_attempt_not_found",
                    "AgentTaskAttempt disappeared after heartbeat",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskAttemptHeartbeatMutation {
            task: task.summary(now),
            attempt: attempt.record(now),
            state_changed: true,
        })
    }

    /// Replace only the carrier/controller generation for the same live Attempt.
    /// A3 keeps this internal until a concrete backend needs it; the durable
    /// primitive exists now so stale carriers are fenced independently from
    /// Attempt retry identity.
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn replace_agent_task_attempt_controller_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        expected_controller_generation: i64,
        now: i64,
    ) -> Result<AgentTaskAttemptRecord, CommunicationStoreError> {
        validate_attempt_mutation_inputs(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            expected_controller_generation,
        )?;
        let mut conn = self.conn.lock().unwrap();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt = require_current_attempt(
            &transaction,
            &task,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            expected_controller_generation,
            now,
        )?;
        let next_generation = attempt.attempt_controller_generation.saturating_add(1);
        transaction
            .execute(
                "UPDATE wc_agent_task_attempts
                 SET attempt_controller_generation = ?2
                 WHERE attempt_id = ?1",
                params![attempt_id, next_generation],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_tasks SET updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
                 WHERE task_id = ?1",
                params![task_id, now],
            )
            .map_err(store_error)?;
        let attempt =
            load_attempt_for_task(&transaction, task_id, attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_attempt_not_found",
                    "AgentTaskAttempt disappeared after controller replacement",
                )
            })?;
        let record = attempt.record(now);
        transaction.commit().map_err(store_error)?;
        Ok(record)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn complete_agent_task_attempt(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        outcome: AgentTaskState,
        terminal_result: Option<&str>,
        terminal_reason: Option<&str>,
        completion_key: &str,
    ) -> Result<AgentTaskAttemptCompletionMutation, CommunicationStoreError> {
        self.complete_agent_task_attempt_with_now(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            outcome,
            terminal_result,
            terminal_reason,
            completion_key,
            None,
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn complete_agent_task_attempt_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        outcome: AgentTaskState,
        terminal_result: Option<&str>,
        terminal_reason: Option<&str>,
        completion_key: &str,
        now: i64,
    ) -> Result<AgentTaskAttemptCompletionMutation, CommunicationStoreError> {
        self.complete_agent_task_attempt_with_now(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            outcome,
            terminal_result,
            terminal_reason,
            completion_key,
            Some(now),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn complete_agent_task_attempt_with_now(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        outcome: AgentTaskState,
        terminal_result: Option<&str>,
        terminal_reason: Option<&str>,
        completion_key: &str,
        now: Option<i64>,
    ) -> Result<AgentTaskAttemptCompletionMutation, CommunicationStoreError> {
        validate_attempt_mutation_inputs(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        if !outcome.terminal() {
            return Err(CommunicationStoreError::new(
                "invalid_agent_task_completion_outcome",
                "AgentTaskAttempt completion outcome must be succeeded or failed",
            ));
        }
        let terminal_result = validate_optional_terminal_text(terminal_result, "terminal_result")?;
        let terminal_reason = validate_optional_terminal_text(terminal_reason, "terminal_reason")?;
        let completion_key = validate_idempotency_key(completion_key)?;
        let request_hash = task_request_hash(&json!({
            "task_id": task_id,
            "attempt_id": attempt_id,
            "assignee_agent_id": assignee_agent_id,
            "attempt_fence": attempt_fence,
            "attempt_controller_generation": attempt_controller_generation,
            "outcome": outcome.as_str(),
            "terminal_result": terminal_result,
            "terminal_reason": terminal_reason,
        }));
        let mut conn = self.conn.lock().unwrap();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let task = load_owned_task(&transaction, principal, task_id, now)?;

        if let Some(replayed_attempt_id) = lookup_idempotent_resource(
            &transaction,
            principal,
            OP_COMPLETE_AGENT_TASK_ATTEMPT,
            &completion_key,
            &request_hash,
        )? {
            if replayed_attempt_id != attempt_id {
                return Err(CommunicationStoreError::new(
                    "agent_task_idempotency_invariant",
                    "Completion replay points to a different AgentTaskAttempt",
                ));
            }
            let attempt = load_attempt_for_task(&transaction, task_id, attempt_id, now)?
                .ok_or_else(|| {
                    CommunicationStoreError::new(
                        "agent_task_attempt_not_found",
                        "AgentTaskAttempt does not exist",
                    )
                })?;
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskAttemptCompletionMutation {
                task: task.summary(now),
                attempt: attempt.record(now),
                replayed: true,
                state_changed: false,
            });
        }

        let _attempt = require_current_attempt(
            &transaction,
            &task,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            now,
        )?;
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
        record_idempotent_resource(
            &transaction,
            principal,
            OP_COMPLETE_AGENT_TASK_ATTEMPT,
            &completion_key,
            &request_hash,
            attempt_id,
            now,
        )?;
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt =
            load_attempt_for_task(&transaction, task_id, attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_attempt_not_found",
                    "AgentTaskAttempt disappeared after completion",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskAttemptCompletionMutation {
            task: task.summary(now),
            attempt: attempt.record(now),
            replayed: false,
            state_changed: true,
        })
    }
}

fn task_request_hash(value: &serde_json::Value) -> String {
    digest_text(
        "webcodex.agent-task.request.v1",
        &serde_json::to_string(value).expect("AgentTask request serializes"),
    )
}

fn validate_title(value: &str) -> Result<String, CommunicationStoreError> {
    let value = value.trim();
    let chars = value.chars().count();
    if chars == 0 || chars > MAX_AGENT_TASK_TITLE_CHARS {
        return Err(CommunicationStoreError::new(
            "invalid_agent_task_title",
            format!("title must contain 1..={MAX_AGENT_TASK_TITLE_CHARS} characters"),
        ));
    }
    Ok(value.to_string())
}

fn validate_instruction(value: &str) -> Result<String, CommunicationStoreError> {
    let value = value.trim();
    let bytes = value.len();
    if bytes == 0 || bytes > MAX_AGENT_TASK_INSTRUCTION_BYTES {
        return Err(CommunicationStoreError::new(
            "invalid_agent_task_instruction",
            format!("instruction must contain 1..={MAX_AGENT_TASK_INSTRUCTION_BYTES} UTF-8 bytes"),
        ));
    }
    Ok(value.to_string())
}

fn validate_project_reference(
    value: Option<&str>,
) -> Result<Option<String>, CommunicationStoreError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    let chars = value.chars().count();
    if chars == 0 || chars > MAX_AGENT_TASK_PROJECT_REF_CHARS {
        return Err(CommunicationStoreError::new(
            "invalid_agent_task_project_reference",
            format!(
                "referenced_project_id must contain 1..={MAX_AGENT_TASK_PROJECT_REF_CHARS} characters"
            ),
        ));
    }
    Ok(Some(value.to_string()))
}

fn validate_optional_terminal_text(
    value: Option<&str>,
    label: &str,
) -> Result<Option<String>, CommunicationStoreError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_AGENT_TASK_TERMINAL_TEXT_BYTES {
        return Err(CommunicationStoreError::new(
            "invalid_agent_task_terminal_text",
            format!(
                "{label} must contain 1..={MAX_AGENT_TASK_TERMINAL_TEXT_BYTES} UTF-8 bytes when provided"
            ),
        ));
    }
    Ok(Some(value.to_string()))
}

fn validate_optional_id(
    value: Option<&str>,
    prefix: &str,
    code: &'static str,
) -> Result<Option<String>, CommunicationStoreError> {
    let Some(value) = value else {
        return Ok(None);
    };
    validate_id(value, prefix, code)?;
    Ok(Some(value.to_string()))
}

fn require_owned_agent(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    agent_id: &str,
) -> Result<(), CommunicationStoreError> {
    validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")?;
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
            "agent_not_found",
            "Agent identity does not exist",
        ));
    }
    Ok(())
}

fn validate_source_references(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    conversation_id: Option<&str>,
    message_id: Option<&str>,
) -> Result<(), CommunicationStoreError> {
    let Some(conversation_id) = conversation_id else {
        return Ok(());
    };
    authorize_conversation_access(conn, principal, &ConversationAccess::Human, conversation_id)?;
    if let Some(message_id) = message_id {
        let exists = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM wc_conversation_messages
                    WHERE conversation_id = ?1 AND message_id = ?2
                 )",
                params![conversation_id, message_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(store_error)?;
        if !exists {
            return Err(CommunicationStoreError::new(
                "agent_task_source_message_not_found",
                "Conversation Message does not exist in the referenced Conversation",
            ));
        }
    }
    Ok(())
}

fn load_owned_task(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    task_id: &str,
    now: i64,
) -> Result<StoredTask, CommunicationStoreError> {
    validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
    let row = conn
        .query_row(
            "SELECT task_id, assignee_agent_id, title, instruction,
                    source_conversation_id, source_message_id, referenced_project_id,
                    state, latest_attempt_id, created_at_unix_ms, updated_at_unix_ms,
                    terminal_at_unix_ms
             FROM wc_agent_tasks
             WHERE task_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3",
            params![task_id, principal.kind, principal.digest],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    AgentTaskState::from_db(&row.get::<_, String>(7)?, 7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, Option<i64>>(11)?,
                ))
            },
        )
        .optional()
        .map_err(store_error)?
        .ok_or_else(|| {
            CommunicationStoreError::new("agent_task_not_found", "AgentTask does not exist")
        })?;
    let latest_attempt = match row.8.as_deref() {
        Some(attempt_id) => {
            load_attempt_for_task(conn, task_id, attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "AgentTask latest Attempt is missing",
                )
            })?
        }
        None => {
            return Ok(StoredTask {
                task_id: row.0,
                assignee_agent_id: row.1,
                title: row.2,
                instruction: row.3,
                source_conversation_id: row.4,
                source_message_id: row.5,
                referenced_project_id: row.6,
                stored_state: row.7,
                created_at_unix_ms: row.9,
                updated_at_unix_ms: row.10,
                terminal_at_unix_ms: row.11,
                latest_attempt: None,
            })
        }
    };
    Ok(StoredTask {
        task_id: row.0,
        assignee_agent_id: row.1,
        title: row.2,
        instruction: row.3,
        source_conversation_id: row.4,
        source_message_id: row.5,
        referenced_project_id: row.6,
        stored_state: row.7,
        created_at_unix_ms: row.9,
        updated_at_unix_ms: row.10,
        terminal_at_unix_ms: row.11,
        latest_attempt: Some(latest_attempt),
    })
}

fn load_attempt_for_task(
    conn: &Connection,
    task_id: &str,
    attempt_id: &str,
    _now: i64,
) -> Result<Option<StoredAttempt>, CommunicationStoreError> {
    validate_id(
        attempt_id,
        AGENT_TASK_ATTEMPT_ID_PREFIX,
        "invalid_agent_task_attempt_id",
    )?;
    conn.query_row(
        "SELECT attempt_id, task_id, attempt_number, assignee_agent_id, state,
                lease_expires_at_unix_ms, attempt_fence, attempt_controller_generation,
                created_at_unix_ms, started_at_unix_ms, terminal_at_unix_ms,
                terminal_result, terminal_reason
         FROM wc_agent_task_attempts
         WHERE attempt_id = ?1 AND task_id = ?2",
        params![attempt_id, task_id],
        |row| {
            Ok(StoredAttempt {
                attempt_id: row.get(0)?,
                task_id: row.get(1)?,
                attempt_number: row.get(2)?,
                assignee_agent_id: row.get(3)?,
                stored_state: AgentTaskAttemptState::from_db(&row.get::<_, String>(4)?, 4)?,
                lease_expires_at_unix_ms: row.get(5)?,
                attempt_fence: row.get(6)?,
                attempt_controller_generation: row.get(7)?,
                created_at_unix_ms: row.get(8)?,
                started_at_unix_ms: row.get(9)?,
                terminal_at_unix_ms: row.get(10)?,
                terminal_result: row.get(11)?,
                terminal_reason: row.get(12)?,
            })
        },
    )
    .optional()
    .map_err(store_error)
}

fn materialize_expired_latest_attempt(
    transaction: &Transaction<'_>,
    task: &mut StoredTask,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let Some(attempt) = task.latest_attempt.as_mut() else {
        return Ok(());
    };
    if attempt.stored_state != AgentTaskAttemptState::Active
        || attempt.lease_expires_at_unix_ms > now
    {
        return Ok(());
    }
    transaction
        .execute(
            "UPDATE wc_agent_task_attempts
             SET state = 'expired', terminal_at_unix_ms = ?2
             WHERE attempt_id = ?1 AND state = 'active'",
            params![attempt.attempt_id, now],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_tasks
             SET state = 'ready', updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
             WHERE task_id = ?1 AND state = 'active'",
            params![task.task_id, now],
        )
        .map_err(store_error)?;
    attempt.stored_state = AgentTaskAttemptState::Expired;
    attempt.terminal_at_unix_ms = Some(now);
    task.stored_state = AgentTaskState::Ready;
    task.updated_at_unix_ms = task.updated_at_unix_ms.max(now);
    Ok(())
}

fn validate_attempt_mutation_inputs(
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
    validate_id(
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

#[allow(clippy::too_many_arguments)]
fn require_current_attempt(
    conn: &Connection,
    task: &StoredTask,
    attempt_id: &str,
    assignee_agent_id: &str,
    attempt_fence: &str,
    attempt_controller_generation: i64,
    now: i64,
) -> Result<StoredAttempt, CommunicationStoreError> {
    if task.stored_state.terminal() {
        return Err(task_terminal_error());
    }
    if task
        .latest_attempt
        .as_ref()
        .map(|attempt| attempt.attempt_id.as_str())
        != Some(attempt_id)
    {
        return Err(CommunicationStoreError::new(
            "agent_task_attempt_stale",
            "AgentTaskAttempt is not the latest authoritative Attempt",
        ));
    }
    if task.assignee_agent_id.as_deref() != Some(assignee_agent_id) {
        return Err(CommunicationStoreError::new(
            "agent_task_assignee_mismatch",
            "AgentTask current assignee does not match the Attempt caller assertion",
        ));
    }
    let attempt =
        load_attempt_for_task(conn, &task.task_id, attempt_id, now)?.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_attempt_not_found",
                "AgentTaskAttempt does not exist",
            )
        })?;
    if attempt.assignee_agent_id != assignee_agent_id
        || attempt.attempt_fence != attempt_fence
        || attempt.attempt_controller_generation != attempt_controller_generation
    {
        return Err(CommunicationStoreError::new(
            "agent_task_attempt_stale",
            "AgentTaskAttempt fence, assignee, or controller generation is stale",
        ));
    }
    if attempt.stored_state != AgentTaskAttemptState::Active
        || attempt.lease_expires_at_unix_ms <= now
    {
        return Err(CommunicationStoreError::new(
            "agent_task_attempt_stale",
            "AgentTaskAttempt is terminal, superseded, or its lease has expired",
        ));
    }
    Ok(attempt)
}

fn task_terminal_error() -> CommunicationStoreError {
    CommunicationStoreError::new(
        "agent_task_terminal",
        "AgentTask is terminal and cannot start or mutate execution ownership",
    )
}

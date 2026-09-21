use super::agent_task::AGENT_TASK_ID_PREFIX;
use super::communication::{
    allocate_identity, digest_json, digest_text, now_unix_ms, validate_communication_principal,
    validate_id, CommunicationPrincipal, CommunicationStoreError, DURABLE_AGENT_ID_PREFIX,
};
use super::goal_plan::{GoalCheckpoint, GoalPlan, NewGoalStep, MAX_GOAL_PLAN_BYTES};
use super::Database;
use rusqlite::{
    params, types::Type, Connection, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::Serialize;
use serde_json::json;

pub const GOAL_ID_PREFIX: &str = "wc_goal_";
pub const MAX_GOAL_TITLE_CHARS: usize = 200;
pub const MAX_GOAL_OBJECTIVE_BYTES: usize = 8_192;
pub const MAX_GOAL_TERMINAL_REASON_BYTES: usize = 4_096;
pub const MAX_GOAL_LIST_LIMIT: usize = 100;
pub const MAX_GOAL_CORRELATIONS: i64 = 64;
pub const WORKFLOW_SESSION_ID_PREFIX: &str = "wc_sess_";
const MAX_GOAL_IDEMPOTENCY_KEY_CHARS: usize = 128;

const OP_CREATE_GOAL: &str = "create_goal";
const OP_PREPARE_GOAL_WORKFLOW: &str = "prepare_goal_workflow";
const OP_UPDATE_GOAL: &str = "update_goal";
const OP_CHECKPOINT_GOAL: &str = "checkpoint_goal";
const OP_ASSOCIATE_GOAL_AGENT_TASK: &str = "associate_goal_agent_task";
const OP_ASSOCIATE_GOAL_WORKFLOW_SESSION: &str = "associate_goal_workflow_session";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalStoreError {
    code: &'static str,
    message: String,
    current_revision: Option<i64>,
}

impl GoalStoreError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            current_revision: None,
        }
    }

    fn revision_changed(current_revision: i64) -> Self {
        Self {
            code: "goal_revision_changed",
            message: format!("Goal changed; current authoritative revision is {current_revision}"),
            current_revision: Some(current_revision),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn current_revision(&self) -> Option<i64> {
        self.current_revision
    }
}

impl std::fmt::Display for GoalStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GoalStoreError {}

fn goal_store_error(error: rusqlite::Error) -> GoalStoreError {
    tracing::warn!(error = %error, "durable Goal store operation failed");
    GoalStoreError::new(
        "goal_store_unavailable",
        "Durable Goal store is unavailable",
    )
}

#[derive(Debug, Clone)]
pub struct NewGoal {
    pub completion_conditions: Vec<String>,
    pub steps: Vec<NewGoalStep>,
    pub title: String,
    pub objective: String,
    pub controller_agent_id: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Default)]
pub struct GoalPatch {
    pub title: Option<String>,
    pub objective: Option<String>,
    pub controller_agent_id: Option<String>,
    pub lifecycle: Option<GoalLifecycle>,
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalLifecycle {
    Active,
    Completed,
    Cancelled,
}

impl GoalLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_input(value: &str) -> Result<Self, GoalStoreError> {
        match value {
            "active" => Ok(Self::Active),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(GoalStoreError::new(
                "invalid_goal_lifecycle",
                "Goal lifecycle must be one of: active, completed, cancelled",
            )),
        }
    }

    fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported Goal lifecycle: {other}").into(),
            )),
        }
    }

    pub const fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalCorrelationKind {
    AgentTask,
    WorkflowSession,
}

impl GoalCorrelationKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AgentTask => "agent_task",
            Self::WorkflowSession => "workflow_session",
        }
    }

    fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "agent_task" => Ok(Self::AgentTask),
            "workflow_session" => Ok(Self::WorkflowSession),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported Goal correlation kind: {other}").into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GoalCorrelation {
    pub kind: GoalCorrelationKind,
    pub reference_id: String,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GoalSummary {
    pub goal_id: String,
    pub title: String,
    pub lifecycle: GoalLifecycle,
    pub revision: i64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub terminal_at_unix_ms: Option<i64>,
    pub agent_task_count: i64,
    pub workflow_session_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GoalDetail {
    pub summary: GoalSummary,
    pub plan: GoalPlan,
    pub objective: String,
    pub controller_agent_id: Option<String>,
    pub terminal_reason: Option<String>,
    pub correlations: Vec<GoalCorrelation>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GoalMutation {
    pub goal: GoalDetail,
    pub created: bool,
    pub replayed: bool,
    pub state_changed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GoalPage {
    pub total_count: i64,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub truncated: bool,
    pub goals: Vec<GoalSummary>,
}

impl Database {
    pub(super) fn ensure_goal_schema(conn: &mut Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS wc_goals (
                goal_id TEXT PRIMARY KEY,
                owner_principal_kind TEXT NOT NULL,
                owner_principal_digest TEXT NOT NULL,
                title TEXT NOT NULL,
                objective TEXT NOT NULL,
                controller_agent_id TEXT REFERENCES wc_agent_identities(agent_id),
                lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active', 'completed', 'cancelled')),
                revision INTEGER NOT NULL CHECK(revision >= 1),
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                terminal_at_unix_ms INTEGER,
                terminal_reason TEXT,
                CHECK(
                    (lifecycle = 'active' AND terminal_at_unix_ms IS NULL AND terminal_reason IS NULL)
                    OR (lifecycle IN ('completed', 'cancelled') AND terminal_at_unix_ms IS NOT NULL)
                )
            );
            CREATE INDEX IF NOT EXISTS idx_wc_goals_owner_updated
                ON wc_goals(owner_principal_digest, updated_at_unix_ms DESC, goal_id);
            CREATE INDEX IF NOT EXISTS idx_wc_goals_owner_lifecycle
                ON wc_goals(owner_principal_digest, lifecycle, updated_at_unix_ms DESC, goal_id);

            CREATE TABLE IF NOT EXISTS wc_goal_correlations (
                goal_id TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('agent_task', 'workflow_session')),
                reference_id TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(goal_id, kind, reference_id),
                FOREIGN KEY(goal_id) REFERENCES wc_goals(goal_id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_goal_correlations_reference
                ON wc_goal_correlations(kind, reference_id, goal_id);

            CREATE TABLE IF NOT EXISTS wc_goal_idempotency (
                principal_digest TEXT NOT NULL,
                operation TEXT NOT NULL,
                key_hash TEXT NOT NULL,
                request_hash TEXT NOT NULL,
                goal_id TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(principal_digest, operation, key_hash)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_goal_idempotency_created
                ON wc_goal_idempotency(created_at_unix_ms DESC);
            ",
        )?;
        let has_controller_agent_id: i64 = conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('wc_goals')
                WHERE name = 'controller_agent_id'
            )",
            [],
            |row| row.get(0),
        )?;
        if has_controller_agent_id == 0 {
            conn.execute(
                "ALTER TABLE wc_goals
                 ADD COLUMN controller_agent_id TEXT REFERENCES wc_agent_identities(agent_id)",
                [],
            )?;
        }
        let has_plan: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('wc_goals') WHERE name = 'plan_json')",
            [],
            |row| row.get(0),
        )?;
        if !has_plan {
            // One additive column; there is only one current plan encoding.
            conn.execute(
                "ALTER TABLE wc_goals ADD COLUMN plan_json TEXT NOT NULL DEFAULT '{\"completion_conditions\":[],\"steps\":[],\"progress_summary\":null,\"checkpoint_at_unix_ms\":null}'",
                [],
            )?;
        }
        Ok(())
    }

    pub fn create_goal(
        &self,
        principal: &CommunicationPrincipal,
        input: NewGoal,
    ) -> Result<GoalMutation, GoalStoreError> {
        self.create_goal_at(principal, input, now_unix_ms())
    }

    pub(crate) fn create_goal_at(
        &self,
        principal: &CommunicationPrincipal,
        input: NewGoal,
        now: i64,
    ) -> Result<GoalMutation, GoalStoreError> {
        validate_goal_principal(principal)?;
        let title = validate_title(&input.title)?;
        let objective = validate_objective(&input.objective)?;
        let controller_agent_id =
            validate_optional_controller_agent_id(input.controller_agent_id.as_deref())?;
        let idempotency_key = validate_idempotency_key(&input.idempotency_key)?;
        let plan = GoalPlan::new(input.completion_conditions, input.steps, now)?;
        let request_hash = goal_request_hash(&json!({
            "title": title,
            "objective": objective,
            "controller_agent_id": controller_agent_id,
            "completion_conditions": plan.completion_conditions,
            "steps": plan.steps.iter().map(|step| json!({"id": step.id, "title": step.title})).collect::<Vec<_>>(),
        }));

        let mut conn = self.lock_connection(crate::StoreDomain::Goal);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(goal_store_error)?;
        if let Some(controller_agent_id) = controller_agent_id.as_deref() {
            require_owned_controller_agent(&transaction, principal, controller_agent_id)?;
        }
        if let Some(goal_id) = lookup_idempotent_goal(
            &transaction,
            principal,
            OP_CREATE_GOAL,
            &idempotency_key,
            &request_hash,
        )? {
            let goal = load_owned_goal(&transaction, principal, &goal_id)?;
            transaction.commit().map_err(goal_store_error)?;
            return Ok(GoalMutation {
                goal,
                created: false,
                replayed: true,
                state_changed: false,
            });
        }

        let goal_id = allocate_identity(
            &transaction,
            GOAL_ID_PREFIX,
            "SELECT EXISTS(SELECT 1 FROM wc_goals WHERE goal_id = ?1)",
        )
        .map_err(map_communication_validation_error)?;
        transaction
            .execute(
                "INSERT INTO wc_goals (
                    goal_id, owner_principal_kind, owner_principal_digest,
                    title, objective, controller_agent_id, lifecycle, revision, created_at_unix_ms,
                    updated_at_unix_ms, terminal_at_unix_ms, terminal_reason, plan_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', 1, ?7, ?7, NULL, NULL, ?8)",
                params![
                    goal_id,
                    principal.kind,
                    principal.digest,
                    title,
                    objective,
                    controller_agent_id,
                    now,
                    plan.persisted_json()?,
                ],
            )
            .map_err(goal_store_error)?;
        record_idempotent_goal(
            &transaction,
            principal,
            OP_CREATE_GOAL,
            &idempotency_key,
            &request_hash,
            &goal_id,
            now,
        )?;
        let goal = load_owned_goal(&transaction, principal, &goal_id)?;
        transaction.commit().map_err(goal_store_error)?;
        Ok(GoalMutation {
            goal,
            created: true,
            replayed: false,
            state_changed: true,
        })
    }

    /// Atomically admit a new Goal and its exact initial Workflow Session correlation.
    /// Session existence/authority is intentionally a ToolRuntime concern; the Store
    /// validates only the canonical identity and never imports Host/Window concepts.
    pub fn prepare_goal_workflow(
        &self,
        principal: &CommunicationPrincipal,
        session_id: &str,
        input: NewGoal,
    ) -> Result<GoalMutation, GoalStoreError> {
        self.prepare_goal_workflow_at(principal, session_id, input, now_unix_ms())
    }

    pub(crate) fn prepare_goal_workflow_at(
        &self,
        principal: &CommunicationPrincipal,
        session_id: &str,
        input: NewGoal,
        now: i64,
    ) -> Result<GoalMutation, GoalStoreError> {
        validate_goal_principal(principal)?;
        validate_workflow_session_id(session_id).map_err(map_communication_validation_error)?;
        let title = validate_title(&input.title)?;
        let objective = validate_objective(&input.objective)?;
        let controller_agent_id =
            validate_optional_controller_agent_id(input.controller_agent_id.as_deref())?;
        let idempotency_key = validate_idempotency_key(&input.idempotency_key)?;
        let plan = GoalPlan::new(input.completion_conditions, input.steps, now)?;
        let request_hash = goal_request_hash(&json!({
            "session_id": session_id,
            "title": title,
            "objective": objective,
            "controller_agent_id": controller_agent_id,
            "completion_conditions": plan.completion_conditions,
            "steps": plan.steps.iter().map(|step| json!({"id": step.id, "title": step.title})).collect::<Vec<_>>(),
        }));

        let mut conn = self.lock_connection(crate::StoreDomain::Goal);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(goal_store_error)?;
        if let Some(goal_id) = lookup_idempotent_goal(
            &transaction,
            principal,
            OP_PREPARE_GOAL_WORKFLOW,
            &idempotency_key,
            &request_hash,
        )? {
            let goal = load_owned_goal(&transaction, principal, &goal_id)?;
            let exact_session = goal.correlations.iter().any(|correlation| {
                correlation.kind == GoalCorrelationKind::WorkflowSession
                    && correlation.reference_id == session_id
            });
            if !exact_session {
                return Err(persisted_goal_state_error());
            }
            transaction.commit().map_err(goal_store_error)?;
            return Ok(GoalMutation {
                goal,
                created: false,
                replayed: true,
                state_changed: false,
            });
        }
        if let Some(controller_agent_id) = controller_agent_id.as_deref() {
            require_owned_controller_agent(&transaction, principal, controller_agent_id)?;
        }

        let goal_id = allocate_identity(
            &transaction,
            GOAL_ID_PREFIX,
            "SELECT EXISTS(SELECT 1 FROM wc_goals WHERE goal_id = ?1)",
        )
        .map_err(map_communication_validation_error)?;
        transaction
            .execute(
                "INSERT INTO wc_goals (
                    goal_id, owner_principal_kind, owner_principal_digest,
                    title, objective, controller_agent_id, lifecycle, revision, created_at_unix_ms,
                    updated_at_unix_ms, terminal_at_unix_ms, terminal_reason, plan_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', 1, ?7, ?7, NULL, NULL, ?8)",
                params![
                    goal_id,
                    principal.kind,
                    principal.digest,
                    title,
                    objective,
                    controller_agent_id,
                    now,
                    plan.persisted_json()?,
                ],
            )
            .map_err(goal_store_error)?;
        transaction
            .execute(
                "INSERT INTO wc_goal_correlations (
                    goal_id, kind, reference_id, created_at_unix_ms
                 ) VALUES (?1, 'workflow_session', ?2, ?3)",
                params![goal_id, session_id, now],
            )
            .map_err(goal_store_error)?;
        record_idempotent_goal(
            &transaction,
            principal,
            OP_PREPARE_GOAL_WORKFLOW,
            &idempotency_key,
            &request_hash,
            &goal_id,
            now,
        )?;
        let goal = load_owned_goal(&transaction, principal, &goal_id)?;
        transaction.commit().map_err(goal_store_error)?;
        Ok(GoalMutation {
            goal,
            created: true,
            replayed: false,
            state_changed: true,
        })
    }

    pub fn read_goal(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
    ) -> Result<GoalDetail, GoalStoreError> {
        validate_goal_principal(principal)?;
        validate_goal_id(goal_id)?;
        let conn = self.lock_connection(crate::StoreDomain::Goal);
        load_owned_goal(&conn, principal, goal_id)
    }

    /// Bounded, owner-scoped explicit Session correlations for workflow closeout.
    /// The caller independently authorizes the Session; no Project/Window lookup
    /// or execution authority is inferred by this query.
    pub fn active_goals_for_workflow_session(
        &self,
        principal: &CommunicationPrincipal,
        session_id: &str,
        limit: usize,
    ) -> Result<(Vec<GoalDetail>, bool), GoalStoreError> {
        validate_goal_principal(principal)?;
        validate_workflow_session_id(session_id).map_err(map_communication_validation_error)?;
        if limit == 0 || limit > 8 {
            return Err(GoalStoreError::new(
                "invalid_goal_follow_up_limit",
                "Goal follow-up limit must be within 1..=8",
            ));
        }
        let conn = self.lock_connection(crate::StoreDomain::Goal);
        let mut statement = conn
            .prepare(
                "SELECT g.goal_id FROM wc_goals g
             JOIN wc_goal_correlations c ON c.goal_id = g.goal_id
             WHERE g.owner_principal_kind = ?1 AND g.owner_principal_digest = ?2
               AND g.lifecycle = 'active' AND c.kind = 'workflow_session' AND c.reference_id = ?3
             ORDER BY g.goal_id LIMIT ?4",
            )
            .map_err(goal_store_error)?;
        let ids = statement
            .query_map(
                params![
                    principal.kind,
                    principal.digest,
                    session_id,
                    (limit + 1) as i64
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(goal_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(goal_store_error)?;
        let truncated = ids.len() > limit;
        let goals = ids
            .iter()
            .take(limit)
            .map(|id| load_owned_goal(&conn, principal, id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((goals, truncated))
    }

    pub fn list_goals(
        &self,
        principal: &CommunicationPrincipal,
        lifecycle: Option<GoalLifecycle>,
        offset: usize,
        limit: usize,
    ) -> Result<GoalPage, GoalStoreError> {
        validate_goal_principal(principal)?;
        if limit == 0 || limit > MAX_GOAL_LIST_LIMIT {
            return Err(GoalStoreError::new(
                "invalid_goal_list_limit",
                format!("limit must be within 1..={MAX_GOAL_LIST_LIMIT}"),
            ));
        }
        let offset_i64 = i64::try_from(offset).map_err(|_| {
            GoalStoreError::new(
                "invalid_goal_list_offset",
                "offset exceeds the durable Goal store range",
            )
        })?;
        let conn = self.lock_connection(crate::StoreDomain::Goal);
        let (total_count, goal_ids) = match lifecycle {
            Some(lifecycle) => {
                let total_count = conn
                    .query_row(
                        "SELECT COUNT(*) FROM wc_goals
                         WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                           AND lifecycle = ?3",
                        params![principal.kind, principal.digest, lifecycle.as_str()],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(goal_store_error)?;
                let mut statement = conn
                    .prepare(
                        "SELECT goal_id FROM wc_goals
                         WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                           AND lifecycle = ?3
                         ORDER BY updated_at_unix_ms DESC, goal_id
                         LIMIT ?4 OFFSET ?5",
                    )
                    .map_err(goal_store_error)?;
                let ids = statement
                    .query_map(
                        params![
                            principal.kind,
                            principal.digest,
                            lifecycle.as_str(),
                            limit as i64,
                            offset_i64,
                        ],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(goal_store_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(goal_store_error)?;
                (total_count, ids)
            }
            None => {
                let total_count = conn
                    .query_row(
                        "SELECT COUNT(*) FROM wc_goals
                         WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2",
                        params![principal.kind, principal.digest],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(goal_store_error)?;
                let mut statement = conn
                    .prepare(
                        "SELECT goal_id FROM wc_goals
                         WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                         ORDER BY updated_at_unix_ms DESC, goal_id
                         LIMIT ?3 OFFSET ?4",
                    )
                    .map_err(goal_store_error)?;
                let ids = statement
                    .query_map(
                        params![principal.kind, principal.digest, limit as i64, offset_i64,],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(goal_store_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(goal_store_error)?;
                (total_count, ids)
            }
        };
        let goals = goal_ids
            .iter()
            .map(|goal_id| load_owned_goal(&conn, principal, goal_id).map(|goal| goal.summary))
            .collect::<Result<Vec<_>, _>>()?;
        let next_offset = if offset.saturating_add(goals.len()) < total_count as usize {
            Some(offset.saturating_add(goals.len()))
        } else {
            None
        };
        Ok(GoalPage {
            total_count,
            offset,
            truncated: next_offset.is_some(),
            next_offset,
            goals,
        })
    }

    pub fn update_goal(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        expected_revision: i64,
        patch: GoalPatch,
        idempotency_key: &str,
    ) -> Result<GoalMutation, GoalStoreError> {
        self.update_goal_at(
            principal,
            goal_id,
            expected_revision,
            patch,
            idempotency_key,
            now_unix_ms(),
        )
    }

    pub(crate) fn update_goal_at(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        expected_revision: i64,
        patch: GoalPatch,
        idempotency_key: &str,
        now: i64,
    ) -> Result<GoalMutation, GoalStoreError> {
        validate_goal_principal(principal)?;
        validate_goal_id(goal_id)?;
        if expected_revision < 1 {
            return Err(GoalStoreError::new(
                "invalid_goal_revision",
                "expected_revision must be at least 1",
            ));
        }
        let title = patch.title.as_deref().map(validate_title).transpose()?;
        let objective = patch
            .objective
            .as_deref()
            .map(validate_objective)
            .transpose()?;
        let controller_agent_id =
            validate_optional_controller_agent_id(patch.controller_agent_id.as_deref())?;
        let terminal_reason = patch
            .terminal_reason
            .as_deref()
            .map(validate_terminal_reason)
            .transpose()?;
        if title.is_none()
            && objective.is_none()
            && controller_agent_id.is_none()
            && patch.lifecycle.is_none()
            && terminal_reason.is_none()
        {
            return Err(GoalStoreError::new(
                "goal_update_empty",
                "Goal update must change bounded metadata or lifecycle",
            ));
        }
        if terminal_reason.is_some() && !patch.lifecycle.is_some_and(GoalLifecycle::terminal) {
            return Err(GoalStoreError::new(
                "goal_terminal_reason_requires_terminal_lifecycle",
                "terminal_reason requires an explicit completed or cancelled lifecycle transition",
            ));
        }
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        let request_hash = goal_request_hash(&json!({
            "goal_id": goal_id,
            "expected_revision": expected_revision,
            "title": title,
            "objective": objective,
            "controller_agent_id": controller_agent_id,
            "lifecycle": patch.lifecycle,
            "terminal_reason": terminal_reason,
        }));

        let mut conn = self.lock_connection(crate::StoreDomain::Goal);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(goal_store_error)?;
        // Authorize the Goal before the optional controller target so a foreign Goal
        // cannot be used as an existence oracle for durable Agent ids.
        let current = load_owned_goal(&transaction, principal, goal_id)?;
        if let Some(controller_agent_id) = controller_agent_id.as_deref() {
            require_owned_controller_agent(&transaction, principal, controller_agent_id)?;
        }
        if let Some(replayed_goal_id) = lookup_idempotent_goal(
            &transaction,
            principal,
            OP_UPDATE_GOAL,
            &idempotency_key,
            &request_hash,
        )? {
            if replayed_goal_id != goal_id {
                return Err(GoalStoreError::new(
                    "goal_idempotency_conflict",
                    "Goal update idempotency replay points at a different Goal",
                ));
            }
            let goal = load_owned_goal(&transaction, principal, goal_id)?;
            transaction.commit().map_err(goal_store_error)?;
            return Ok(GoalMutation {
                goal,
                created: false,
                replayed: true,
                state_changed: false,
            });
        }

        if current.summary.revision != expected_revision {
            return Err(GoalStoreError::revision_changed(current.summary.revision));
        }
        if current.summary.lifecycle.terminal() {
            return Err(GoalStoreError::new(
                "goal_terminal",
                "Terminal Goal state is immutable",
            ));
        }
        let target_lifecycle = patch.lifecycle.unwrap_or(GoalLifecycle::Active);
        if target_lifecycle == GoalLifecycle::Completed && !current.plan.complete() {
            return Err(GoalStoreError::new(
                "goal_plan_incomplete",
                "Complete every Goal plan step with checkpoint_goal before completing the Goal",
            ));
        }
        let target_title = title.unwrap_or_else(|| current.summary.title.clone());
        let target_objective = objective.unwrap_or_else(|| current.objective.clone());
        let target_controller_agent_id =
            controller_agent_id.or_else(|| current.controller_agent_id.clone());
        let state_changed = target_title != current.summary.title
            || target_objective != current.objective
            || target_controller_agent_id != current.controller_agent_id
            || target_lifecycle != current.summary.lifecycle
            || terminal_reason != current.terminal_reason;

        if state_changed {
            let (terminal_at, terminal_reason) = if target_lifecycle.terminal() {
                (Some(now), terminal_reason)
            } else {
                (None, None)
            };
            transaction
                .execute(
                    "UPDATE wc_goals
                     SET title = ?2, objective = ?3, controller_agent_id = ?4, lifecycle = ?5,
                         revision = revision + 1, updated_at_unix_ms = ?6,
                         terminal_at_unix_ms = ?7, terminal_reason = ?8
                     WHERE goal_id = ?1",
                    params![
                        goal_id,
                        target_title,
                        target_objective,
                        target_controller_agent_id,
                        target_lifecycle.as_str(),
                        now,
                        terminal_at,
                        terminal_reason,
                    ],
                )
                .map_err(goal_store_error)?;
        }
        record_idempotent_goal(
            &transaction,
            principal,
            OP_UPDATE_GOAL,
            &idempotency_key,
            &request_hash,
            goal_id,
            now,
        )?;
        let goal = load_owned_goal(&transaction, principal, goal_id)?;
        transaction.commit().map_err(goal_store_error)?;
        Ok(GoalMutation {
            goal,
            created: false,
            replayed: false,
            state_changed,
        })
    }

    pub fn checkpoint_goal(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        expected_revision: i64,
        checkpoint: GoalCheckpoint,
        idempotency_key: &str,
    ) -> Result<GoalMutation, GoalStoreError> {
        self.checkpoint_goal_at(
            principal,
            goal_id,
            expected_revision,
            checkpoint,
            idempotency_key,
            now_unix_ms(),
        )
    }

    pub(crate) fn checkpoint_goal_at(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        expected_revision: i64,
        checkpoint: GoalCheckpoint,
        idempotency_key: &str,
        now: i64,
    ) -> Result<GoalMutation, GoalStoreError> {
        validate_goal_principal(principal)?;
        validate_goal_id(goal_id)?;
        if expected_revision < 1 {
            return Err(GoalStoreError::new(
                "invalid_goal_revision",
                "expected_revision must be at least 1",
            ));
        }
        let checkpoint = checkpoint.normalized()?;
        let key = validate_idempotency_key(idempotency_key)?;
        let hash = goal_request_hash(&json!({
            "goal_id": goal_id,
            "expected_revision": expected_revision,
            "checkpoint": checkpoint,
        }));
        let mut conn = self.lock_connection(crate::StoreDomain::Goal);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(goal_store_error)?;
        let current = load_owned_goal(&transaction, principal, goal_id)?;
        if let Some(replayed_goal_id) =
            lookup_idempotent_goal(&transaction, principal, OP_CHECKPOINT_GOAL, &key, &hash)?
        {
            if replayed_goal_id != goal_id {
                return Err(GoalStoreError::new(
                    "goal_idempotency_conflict",
                    "Checkpoint replay points at a different Goal",
                ));
            }
            transaction.commit().map_err(goal_store_error)?;
            return Ok(GoalMutation {
                goal: current,
                created: false,
                replayed: true,
                state_changed: false,
            });
        }
        if current.summary.lifecycle.terminal() {
            return Err(GoalStoreError::new(
                "goal_terminal",
                "Terminal Goal state is immutable",
            ));
        }
        if current.summary.revision != expected_revision {
            return Err(GoalStoreError::revision_changed(current.summary.revision));
        }
        let next_revision = expected_revision
            .checked_add(1)
            .ok_or_else(persisted_goal_state_error)?;
        let now = now.max(current.summary.updated_at_unix_ms);
        // Validate the complete candidate before any mutation. A new checkpoint is
        // one recovery point even when its summary or selected steps are unchanged.
        let plan = current.plan.checkpoint(&checkpoint, now)?;
        transaction.execute(
            "UPDATE wc_goals SET plan_json = ?2, revision = ?3, updated_at_unix_ms = ?4 WHERE goal_id = ?1 AND revision = ?5",
            params![goal_id, plan.persisted_json()?, next_revision, now, expected_revision],
        ).map_err(goal_store_error)?;
        record_idempotent_goal(
            &transaction,
            principal,
            OP_CHECKPOINT_GOAL,
            &key,
            &hash,
            goal_id,
            now,
        )?;
        let goal = load_owned_goal(&transaction, principal, goal_id)?;
        transaction.commit().map_err(goal_store_error)?;
        Ok(GoalMutation {
            goal,
            created: false,
            replayed: false,
            state_changed: true,
        })
    }

    pub fn associate_goal_agent_task(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        task_id: &str,
        idempotency_key: &str,
    ) -> Result<GoalMutation, GoalStoreError> {
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")
            .map_err(map_communication_validation_error)?;
        self.associate_goal_reference(
            principal,
            goal_id,
            GoalCorrelationKind::AgentTask,
            task_id,
            idempotency_key,
            now_unix_ms(),
        )
    }

    pub fn associate_goal_workflow_session(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        session_id: &str,
        idempotency_key: &str,
    ) -> Result<GoalMutation, GoalStoreError> {
        validate_workflow_session_id(session_id).map_err(map_communication_validation_error)?;
        self.associate_goal_reference(
            principal,
            goal_id,
            GoalCorrelationKind::WorkflowSession,
            session_id,
            idempotency_key,
            now_unix_ms(),
        )
    }

    #[cfg(test)]
    pub(crate) fn associate_goal_reference_at(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        kind: GoalCorrelationKind,
        reference_id: &str,
        idempotency_key: &str,
        now: i64,
    ) -> Result<GoalMutation, GoalStoreError> {
        match kind {
            GoalCorrelationKind::AgentTask => {
                validate_id(reference_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")
                    .map_err(map_communication_validation_error)?
            }
            GoalCorrelationKind::WorkflowSession => validate_workflow_session_id(reference_id)
                .map_err(map_communication_validation_error)?,
        }
        self.associate_goal_reference(principal, goal_id, kind, reference_id, idempotency_key, now)
    }

    fn associate_goal_reference(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        kind: GoalCorrelationKind,
        reference_id: &str,
        idempotency_key: &str,
        now: i64,
    ) -> Result<GoalMutation, GoalStoreError> {
        validate_goal_principal(principal)?;
        validate_goal_id(goal_id)?;
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        let operation = match kind {
            GoalCorrelationKind::AgentTask => OP_ASSOCIATE_GOAL_AGENT_TASK,
            GoalCorrelationKind::WorkflowSession => OP_ASSOCIATE_GOAL_WORKFLOW_SESSION,
        };
        let request_hash = goal_request_hash(&json!({
            "goal_id": goal_id,
            "kind": kind,
            "reference_id": reference_id,
        }));
        let mut conn = self.lock_connection(crate::StoreDomain::Goal);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(goal_store_error)?;
        if let Some(replayed_goal_id) = lookup_idempotent_goal(
            &transaction,
            principal,
            operation,
            &idempotency_key,
            &request_hash,
        )? {
            if replayed_goal_id != goal_id {
                return Err(GoalStoreError::new(
                    "goal_idempotency_conflict",
                    "Goal association idempotency replay points at a different Goal",
                ));
            }
            let goal = load_owned_goal(&transaction, principal, goal_id)?;
            transaction.commit().map_err(goal_store_error)?;
            return Ok(GoalMutation {
                goal,
                created: false,
                replayed: true,
                state_changed: false,
            });
        }
        let current = load_owned_goal(&transaction, principal, goal_id)?;
        if current.summary.lifecycle.terminal() {
            return Err(GoalStoreError::new(
                "goal_terminal",
                "Terminal Goal state is immutable",
            ));
        }
        let exists = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM wc_goal_correlations
                    WHERE goal_id = ?1 AND kind = ?2 AND reference_id = ?3
                 )",
                params![goal_id, kind.as_str(), reference_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(goal_store_error)?;
        let state_changed = if exists {
            false
        } else {
            let count = transaction
                .query_row(
                    "SELECT COUNT(*) FROM wc_goal_correlations WHERE goal_id = ?1",
                    [goal_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(goal_store_error)?;
            if count >= MAX_GOAL_CORRELATIONS {
                return Err(GoalStoreError::new(
                    "goal_correlation_capacity_exceeded",
                    format!("Goal correlation count is limited to {MAX_GOAL_CORRELATIONS}"),
                ));
            }
            if kind == GoalCorrelationKind::AgentTask {
                let active_goal_fanout = transaction
                    .query_row(
                        "SELECT COUNT(*)
                         FROM wc_goal_correlations c
                         JOIN wc_goals g ON g.goal_id = c.goal_id
                         WHERE c.kind = 'agent_task' AND c.reference_id = ?1
                           AND g.lifecycle = 'active'
                           AND g.owner_principal_kind = ?2
                           AND g.owner_principal_digest = ?3",
                        params![reference_id, principal.kind, principal.digest],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(goal_store_error)?;
                if active_goal_fanout >= MAX_GOAL_CORRELATIONS {
                    return Err(GoalStoreError::new(
                        "goal_agent_task_fanout_capacity_exceeded",
                        format!(
                            "One AgentTask may be correlated to at most {MAX_GOAL_CORRELATIONS} active Goals per owner"
                        ),
                    ));
                }
            }
            transaction
                .execute(
                    "INSERT INTO wc_goal_correlations (
                        goal_id, kind, reference_id, created_at_unix_ms
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![goal_id, kind.as_str(), reference_id, now],
                )
                .map_err(goal_store_error)?;
            transaction
                .execute(
                    "UPDATE wc_goals
                     SET revision = revision + 1, updated_at_unix_ms = ?2
                     WHERE goal_id = ?1",
                    params![goal_id, now],
                )
                .map_err(goal_store_error)?;
            true
        };
        record_idempotent_goal(
            &transaction,
            principal,
            operation,
            &idempotency_key,
            &request_hash,
            goal_id,
            now,
        )?;
        let goal = load_owned_goal(&transaction, principal, goal_id)?;
        transaction.commit().map_err(goal_store_error)?;
        Ok(GoalMutation {
            goal,
            created: false,
            replayed: false,
            state_changed,
        })
    }
}

fn validate_goal_principal(principal: &CommunicationPrincipal) -> Result<(), GoalStoreError> {
    validate_communication_principal(principal).map_err(map_communication_validation_error)
}

fn map_communication_validation_error(
    error: super::communication::CommunicationStoreError,
) -> GoalStoreError {
    GoalStoreError::new(error.code(), error.message())
}

fn validate_goal_id(goal_id: &str) -> Result<(), GoalStoreError> {
    validate_id(goal_id, GOAL_ID_PREFIX, "invalid_goal_id")
        .map_err(map_communication_validation_error)
}

fn validate_optional_controller_agent_id(
    value: Option<&str>,
) -> Result<Option<String>, GoalStoreError> {
    let Some(value) = value else {
        return Ok(None);
    };
    validate_id(value, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")
        .map_err(map_communication_validation_error)?;
    Ok(Some(value.to_string()))
}

fn require_owned_controller_agent(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    agent_id: &str,
) -> Result<(), GoalStoreError> {
    validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")
        .map_err(map_communication_validation_error)?;
    let owned = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM wc_agent_identities
                WHERE agent_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3
             )",
            params![agent_id, principal.kind, principal.digest],
            |row| row.get::<_, bool>(0),
        )
        .map_err(goal_store_error)?;
    if !owned {
        return Err(GoalStoreError::new(
            "agent_not_found",
            "Agent identity does not exist",
        ));
    }
    Ok(())
}

fn validate_title(value: &str) -> Result<String, GoalStoreError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_GOAL_TITLE_CHARS {
        return Err(GoalStoreError::new(
            "invalid_goal_title",
            format!("Goal title must contain 1..={MAX_GOAL_TITLE_CHARS} characters"),
        ));
    }
    Ok(value.to_string())
}

fn validate_objective(value: &str) -> Result<String, GoalStoreError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_GOAL_OBJECTIVE_BYTES {
        return Err(GoalStoreError::new(
            "invalid_goal_objective",
            format!("Goal objective must contain 1..={MAX_GOAL_OBJECTIVE_BYTES} UTF-8 bytes"),
        ));
    }
    Ok(value.to_string())
}

fn validate_terminal_reason(value: &str) -> Result<String, GoalStoreError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_GOAL_TERMINAL_REASON_BYTES {
        return Err(GoalStoreError::new(
            "invalid_goal_terminal_reason",
            format!(
                "Goal terminal_reason must contain 1..={MAX_GOAL_TERMINAL_REASON_BYTES} UTF-8 bytes"
            ),
        ));
    }
    Ok(value.to_string())
}

fn validate_idempotency_key(value: &str) -> Result<String, GoalStoreError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_GOAL_IDEMPOTENCY_KEY_CHARS {
        return Err(GoalStoreError::new(
            "invalid_goal_idempotency_key",
            format!("idempotency_key must contain 1..={MAX_GOAL_IDEMPOTENCY_KEY_CHARS} characters"),
        ));
    }
    Ok(value.to_string())
}

fn goal_request_hash(value: &serde_json::Value) -> String {
    digest_json("webcodex.goal.request.v2", value).expect("Goal request serializes")
}

fn lookup_idempotent_goal(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    operation: &str,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<Option<String>, GoalStoreError> {
    let key_hash = digest_text("webcodex.goal.idempotency-key.v1", idempotency_key);
    let existing: Option<(String, String)> = transaction
        .query_row(
            "SELECT request_hash, goal_id FROM wc_goal_idempotency
             WHERE principal_digest = ?1 AND operation = ?2 AND key_hash = ?3",
            params![principal.digest, operation, key_hash],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(goal_store_error)?;
    match existing {
        None => Ok(None),
        Some((existing_request_hash, goal_id)) if existing_request_hash == request_hash => {
            Ok(Some(goal_id))
        }
        Some(_) => Err(GoalStoreError::new(
            "goal_idempotency_conflict",
            "Idempotency key was already used with a different Goal request",
        )),
    }
}

fn record_idempotent_goal(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    operation: &str,
    idempotency_key: &str,
    request_hash: &str,
    goal_id: &str,
    now: i64,
) -> Result<(), GoalStoreError> {
    let key_hash = digest_text("webcodex.goal.idempotency-key.v1", idempotency_key);
    transaction
        .execute(
            "INSERT INTO wc_goal_idempotency (
                principal_digest, operation, key_hash, request_hash, goal_id, created_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                principal.digest,
                operation,
                key_hash,
                request_hash,
                goal_id,
                now,
            ],
        )
        .map_err(goal_store_error)?;
    Ok(())
}

fn persisted_goal_state_error() -> GoalStoreError {
    tracing::warn!("durable Goal store contains invalid persisted bounded state");
    GoalStoreError::new(
        "goal_store_unavailable",
        "Durable Goal store contains invalid persisted state",
    )
}

fn validate_loaded_goal_summary(summary: &GoalSummary) -> Result<(), GoalStoreError> {
    let normalized_title =
        validate_title(&summary.title).map_err(|_| persisted_goal_state_error())?;
    if normalized_title != summary.title
        || summary.revision < 1
        || summary.agent_task_count < 0
        || summary.workflow_session_count < 0
        || summary
            .agent_task_count
            .saturating_add(summary.workflow_session_count)
            > MAX_GOAL_CORRELATIONS
        || (summary.lifecycle == GoalLifecycle::Active && summary.terminal_at_unix_ms.is_some())
        || (summary.lifecycle.terminal() && summary.terminal_at_unix_ms.is_none())
    {
        return Err(persisted_goal_state_error());
    }
    Ok(())
}

fn validate_loaded_goal_detail(detail: &GoalDetail) -> Result<(), GoalStoreError> {
    validate_loaded_goal_summary(&detail.summary)?;
    detail
        .plan
        .validate_persisted(
            detail.summary.created_at_unix_ms,
            detail.summary.updated_at_unix_ms,
        )
        .map_err(|_| persisted_goal_state_error())?;
    if detail.summary.lifecycle == GoalLifecycle::Completed && !detail.plan.complete() {
        return Err(persisted_goal_state_error());
    }
    let normalized_objective =
        validate_objective(&detail.objective).map_err(|_| persisted_goal_state_error())?;
    if normalized_objective != detail.objective
        || detail
            .controller_agent_id
            .as_deref()
            .is_some_and(|agent_id| {
                validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id").is_err()
            })
        || (detail.summary.lifecycle == GoalLifecycle::Active && detail.terminal_reason.is_some())
    {
        return Err(persisted_goal_state_error());
    }
    if let Some(reason) = detail.terminal_reason.as_deref() {
        let normalized_reason =
            validate_terminal_reason(reason).map_err(|_| persisted_goal_state_error())?;
        if normalized_reason != reason {
            return Err(persisted_goal_state_error());
        }
    }
    if detail.correlations.len() > MAX_GOAL_CORRELATIONS as usize {
        return Err(persisted_goal_state_error());
    }
    for correlation in &detail.correlations {
        let validation = match correlation.kind {
            GoalCorrelationKind::AgentTask => validate_id(
                &correlation.reference_id,
                AGENT_TASK_ID_PREFIX,
                "invalid_agent_task_id",
            ),
            GoalCorrelationKind::WorkflowSession => {
                validate_workflow_session_id(&correlation.reference_id)
            }
        };
        if validation.is_err() {
            return Err(persisted_goal_state_error());
        }
    }
    Ok(())
}

pub(super) fn load_owned_goal(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    goal_id: &str,
) -> Result<GoalDetail, GoalStoreError> {
    validate_goal_id(goal_id)?;
    let row = conn
        .query_row(
            "SELECT goal_id, title, objective, controller_agent_id, lifecycle, revision,
                    created_at_unix_ms, updated_at_unix_ms, terminal_at_unix_ms,
                    terminal_reason,
                    (SELECT COUNT(*) FROM wc_goal_correlations c
                     WHERE c.goal_id = g.goal_id AND c.kind = 'agent_task'),
                    (SELECT COUNT(*) FROM wc_goal_correlations c
                     WHERE c.goal_id = g.goal_id AND c.kind = 'workflow_session'),
                    CASE WHEN length(CAST(plan_json AS BLOB)) <= ?4 THEN plan_json ELSE NULL END
             FROM wc_goals g
             WHERE goal_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3",
            params![
                goal_id,
                principal.kind,
                principal.digest,
                MAX_GOAL_PLAN_BYTES as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    GoalLifecycle::from_db(&row.get::<_, String>(4)?, 4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, Option<String>>(12)?,
                ))
            },
        )
        .optional()
        .map_err(goal_store_error)?
        .ok_or_else(|| GoalStoreError::new("goal_not_found", "Goal does not exist"))?;
    if row.10.saturating_add(row.11) > MAX_GOAL_CORRELATIONS {
        return Err(persisted_goal_state_error());
    }
    let mut statement = conn
        .prepare(
            "SELECT kind, reference_id, created_at_unix_ms
             FROM wc_goal_correlations
             WHERE goal_id = ?1
             ORDER BY created_at_unix_ms, kind, reference_id
             LIMIT ?2",
        )
        .map_err(goal_store_error)?;
    let correlations = statement
        .query_map(params![goal_id, MAX_GOAL_CORRELATIONS + 1], |row| {
            Ok(GoalCorrelation {
                kind: GoalCorrelationKind::from_db(&row.get::<_, String>(0)?, 0)?,
                reference_id: row.get(1)?,
                created_at_unix_ms: row.get(2)?,
            })
        })
        .map_err(goal_store_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(goal_store_error)?;
    let plan = GoalPlan::from_persisted(row.12.as_deref().ok_or_else(persisted_goal_state_error)?)
        .map_err(|_| persisted_goal_state_error())?;
    let detail = GoalDetail {
        plan,
        summary: GoalSummary {
            goal_id: row.0,
            title: row.1,
            lifecycle: row.4,
            revision: row.5,
            created_at_unix_ms: row.6,
            updated_at_unix_ms: row.7,
            terminal_at_unix_ms: row.8,
            agent_task_count: row.10,
            workflow_session_count: row.11,
        },
        objective: row.2,
        controller_agent_id: row.3,
        terminal_reason: row.9,
        correlations,
    };
    validate_loaded_goal_detail(&detail)?;
    if let Some(controller_agent_id) = detail.controller_agent_id.as_deref() {
        require_owned_controller_agent(conn, principal, controller_agent_id)
            .map_err(|_| persisted_goal_state_error())?;
    }
    Ok(detail)
}

#[cfg(test)]
mod lifecycle_contract_tests {
    use super::*;

    #[test]
    fn goal_lifecycle_and_correlation_encodings_are_closed() {
        for (lifecycle, db) in [
            (GoalLifecycle::Active, "active"),
            (GoalLifecycle::Completed, "completed"),
            (GoalLifecycle::Cancelled, "cancelled"),
        ] {
            assert_eq!(lifecycle.as_str(), db);
            assert_eq!(GoalLifecycle::from_db(db, 0).unwrap(), lifecycle);
            assert_eq!(serde_json::to_value(lifecycle).unwrap(), db);
        }
        assert!(GoalLifecycle::from_db("waiting_validation", 0).is_err());

        for (kind, db) in [
            (GoalCorrelationKind::AgentTask, "agent_task"),
            (GoalCorrelationKind::WorkflowSession, "workflow_session"),
        ] {
            assert_eq!(kind.as_str(), db);
            assert_eq!(GoalCorrelationKind::from_db(db, 0).unwrap(), kind);
            assert_eq!(serde_json::to_value(kind).unwrap(), db);
        }
        assert!(GoalCorrelationKind::from_db("job", 0).is_err());
    }
}

fn validate_workflow_session_id(value: &str) -> Result<(), CommunicationStoreError> {
    if webcodex_core::workflow_session_contract::is_valid_session_id(value) {
        Ok(())
    } else {
        Err(CommunicationStoreError::new(
            "invalid_workflow_session_id",
            "Invalid Workflow Session identity",
        ))
    }
}

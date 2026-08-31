use super::{RecoveryKind, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::db::{AgentTaskState, CommunicationStoreError, NewAgentTask, MAX_AGENT_TASK_LIST_LIMIT};
use serde::Serialize;
use serde_json::{json, to_value};

const DEFAULT_AGENT_TASK_LIST_LIMIT: usize = 50;

fn task_principal(
    auth: Option<&AuthContext>,
) -> Result<crate::db::CommunicationPrincipal, ToolResult> {
    super::communication::communication_principal(auth)
}

fn agent_task_store_unavailable() -> ToolResult {
    ToolResult::err_with_output(
        "Durable AgentTask storage is unavailable in this runtime",
        json!({
            "error_kind": "agent_task_store_unavailable",
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::UserAction, None)
}

fn agent_task_recovery_kind(
    error_kind: &str,
    store_failure_recovery: RecoveryKind,
) -> RecoveryKind {
    match error_kind {
        "communication_store_unavailable" => store_failure_recovery,
        "agent_task_attempt_stale" => RecoveryKind::Reconcile,
        "agent_task_attempt_active"
        | "agent_task_assignee_mismatch"
        | "agent_task_unassigned"
        | "agent_task_terminal"
        | "communication_idempotency_conflict" => RecoveryKind::Reobserve,
        _ => RecoveryKind::FixInput,
    }
}

fn agent_task_error(
    error: CommunicationStoreError,
    store_failure_recovery: RecoveryKind,
) -> ToolResult {
    let recovery = agent_task_recovery_kind(error.code(), store_failure_recovery);
    ToolResult::err_with_output(
        error.message(),
        json!({
            "error_kind": error.code(),
            "message": error.message(),
            "state_changed": false,
        }),
    )
    .with_recovery(recovery, None)
}

fn serialized_task_success<T: Serialize>(value: T) -> ToolResult {
    match to_value(value) {
        Ok(value) => ToolResult::ok(value),
        Err(error) => ToolResult::err_with_output(
            format!("Failed to serialize durable AgentTask result: {error}"),
            json!({
                "error_kind": "agent_task_result_serialization_failed",
                "state_changed": false,
            }),
        )
        .with_recovery(RecoveryKind::NoAction, None),
    }
}

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_agent_task(
        &self,
        auth: Option<&AuthContext>,
        title: String,
        instruction: String,
        assignee_agent_id: Option<String>,
        source_conversation_id: Option<String>,
        source_message_id: Option<String>,
        referenced_project_id: Option<String>,
        idempotency_key: String,
    ) -> ToolResult {
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        match db.create_agent_task(
            &principal,
            NewAgentTask {
                title,
                instruction,
                assignee_agent_id,
                source_conversation_id,
                source_message_id,
                referenced_project_id,
                idempotency_key,
            },
        ) {
            Ok(result) => serialized_task_success(result),
            Err(error) => agent_task_error(error, RecoveryKind::RetrySame),
        }
    }

    pub(crate) fn list_agent_tasks(
        &self,
        auth: Option<&AuthContext>,
        assignee_agent_id: Option<String>,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> ToolResult {
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(DEFAULT_AGENT_TASK_LIST_LIMIT);
        if limit == 0 || limit > MAX_AGENT_TASK_LIST_LIMIT {
            return ToolResult::err_with_output(
                format!("limit must be 1..={MAX_AGENT_TASK_LIST_LIMIT}"),
                json!({
                    "error_kind": "invalid_agent_task_list_limit",
                    "state_changed": false,
                }),
            )
            .with_recovery(RecoveryKind::FixInput, None);
        }
        match db.list_agent_tasks(&principal, assignee_agent_id.as_deref(), offset, limit) {
            Ok(result) => serialized_task_success(result),
            Err(error) => agent_task_error(error, RecoveryKind::Reobserve),
        }
    }

    pub(crate) fn read_agent_task(
        &self,
        auth: Option<&AuthContext>,
        task_id: String,
    ) -> ToolResult {
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        match db.read_agent_task(&principal, &task_id) {
            Ok(task) => serialized_task_success(json!({"task": task})),
            Err(error) => agent_task_error(error, RecoveryKind::Reobserve),
        }
    }

    pub(crate) fn assign_agent_task(
        &self,
        auth: Option<&AuthContext>,
        task_id: String,
        assignee_agent_id: String,
    ) -> ToolResult {
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        match db.assign_agent_task(&principal, &task_id, &assignee_agent_id) {
            Ok(result) => serialized_task_success(result),
            Err(error) => agent_task_error(error, RecoveryKind::Reobserve),
        }
    }

    pub(crate) fn start_agent_task_attempt(
        &self,
        auth: Option<&AuthContext>,
        task_id: String,
        assignee_agent_id: String,
        idempotency_key: String,
    ) -> ToolResult {
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        match db.start_agent_task_attempt(
            &principal,
            &task_id,
            &assignee_agent_id,
            &idempotency_key,
        ) {
            Ok(result) => serialized_task_success(result),
            Err(error) => agent_task_error(error, RecoveryKind::RetrySame),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn heartbeat_agent_task_attempt(
        &self,
        auth: Option<&AuthContext>,
        task_id: String,
        attempt_id: String,
        assignee_agent_id: String,
        attempt_fence: String,
        attempt_controller_generation: i64,
    ) -> ToolResult {
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        match db.heartbeat_agent_task_attempt(
            &principal,
            &task_id,
            &attempt_id,
            &assignee_agent_id,
            &attempt_fence,
            attempt_controller_generation,
        ) {
            Ok(result) => serialized_task_success(result),
            Err(error) => agent_task_error(error, RecoveryKind::Reconcile),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn complete_agent_task_attempt(
        &self,
        auth: Option<&AuthContext>,
        task_id: String,
        attempt_id: String,
        assignee_agent_id: String,
        attempt_fence: String,
        attempt_controller_generation: i64,
        outcome: String,
        terminal_result: Option<String>,
        terminal_reason: Option<String>,
        completion_key: String,
    ) -> ToolResult {
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let outcome = match outcome.as_str() {
            "succeeded" => AgentTaskState::Succeeded,
            "failed" => AgentTaskState::Failed,
            _ => {
                return ToolResult::err_with_output(
                    "outcome must be succeeded or failed",
                    json!({
                        "error_kind": "invalid_agent_task_completion_outcome",
                        "state_changed": false,
                    }),
                )
                .with_recovery(RecoveryKind::FixInput, None)
            }
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        match db.complete_agent_task_attempt(
            &principal,
            &task_id,
            &attempt_id,
            &assignee_agent_id,
            &attempt_fence,
            attempt_controller_generation,
            outcome,
            terminal_result.as_deref(),
            terminal_reason.as_deref(),
            &completion_key,
        ) {
            Ok(result) => serialized_task_success(result),
            Err(error) => agent_task_error(error, RecoveryKind::RetrySame),
        }
    }
}

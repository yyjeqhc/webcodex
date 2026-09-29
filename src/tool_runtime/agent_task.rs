use super::coding_agent::{
    CodingAgentPreparedStart, CodingAgentStartCertainty, CodingAgentStartFailure,
    CodingAgentTypedStartOutcome,
};
use super::{RecoveryKind, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::db::{
    AgentTaskCodingRunBindingIntent, AgentTaskCodingRunBindingRecord,
    AgentTaskCodingRunDispatchState, AgentTaskCodingRunObservation, AgentTaskState,
    CommunicationStoreError, NewAgentTask, MAX_AGENT_TASK_LIST_LIMIT,
    MAX_AGENT_TASK_TERMINAL_TEXT_BYTES,
};
use serde::Serialize;
use serde_json::{json, to_value, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use webcodex_core::coding_agent::{
    CodingAgentConfigValue, CodingAgentRunSnapshot, CodingAgentRunState,
};

const DEFAULT_AGENT_TASK_LIST_LIMIT: usize = 50;
const AGENT_TASK_ATTEMPT_REF_PREFIX: &str = "~ta";

fn format_agent_task_attempt_ref(index: u64) -> String {
    format!("{AGENT_TASK_ATTEMPT_REF_PREFIX}{index}")
}

fn parse_agent_task_attempt_ref(raw: &str) -> Option<u64> {
    let digits = raw.strip_prefix(AGENT_TASK_ATTEMPT_REF_PREFIX)?;
    if digits.is_empty()
        || digits.len() > 19
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
    {
        return None;
    }
    let index = digits.parse::<u64>().ok()?;
    (index > 0).then_some(index)
}

fn attempt_selector_error(error_kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message,
        json!({
            "error_kind": error_kind,
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::FixInput)
}

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
    .with_recovery(RecoveryKind::UserAction)
}

fn agent_task_recovery_kind(
    error_kind: &str,
    store_failure_recovery: RecoveryKind,
) -> RecoveryKind {
    match error_kind {
        "communication_store_unavailable" => store_failure_recovery,
        "agent_task_attempt_stale"
        | "agent_task_execution_active"
        | "agent_task_execution_outcome_unknown"
        | "agent_task_coding_run_identity_mismatch"
        | "agent_task_coding_run_observation_conflict"
        | "agent_task_coding_run_observation_stale" => RecoveryKind::Reconcile,
        "agent_task_attempt_active"
        | "agent_task_assignee_mismatch"
        | "agent_task_unassigned"
        | "agent_task_terminal"
        | "communication_idempotency_conflict" => RecoveryKind::Reobserve,
        _ => RecoveryKind::FixInput,
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    use webcodex_core::coding_agent::CodingAgentExecutionState;

    #[test]
    fn coding_run_observation_revision_overflow_fails_closed() {
        let run = CodingAgentRunSnapshot {
            run_id: "wc_agent_run_revision_overflow".to_string(),
            intent_fingerprint: "intent-overflow".to_string(),
            authority_fingerprint: "auth_overflow".to_string(),
            runtime_project_id: "agent:test:overflow".to_string(),
            provider_id: "codex".to_string(),
            provider_instance_id: "provider-overflow".to_string(),
            state: CodingAgentRunState::Running,
            execution_state: CodingAgentExecutionState::Started,
            observation_revision: u64::MAX,
            created_at: 1,
            updated_at: 1,
            terminal: None,
        };
        let error = coding_run_observation(&run).unwrap_err();
        assert!(!error.success);
        assert_eq!(
            error.output["error_kind"],
            "agent_task_coding_run_revision_out_of_range"
        );
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
    .with_recovery(recovery)
}

fn serialized_task_success<T: Serialize>(value: T) -> ToolResult {
    match to_value(value) {
        Ok(value) => ToolResult::ok(value),
        Err(error) => agent_task_serialization_error(error),
    }
}

fn agent_task_serialization_error(error: impl std::fmt::Display) -> ToolResult {
    ToolResult::err_with_output(
        format!("Failed to serialize durable AgentTask result: {error}"),
        json!({
            "error_kind": "agent_task_result_serialization_failed",
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::NoAction)
}

fn coding_run_replay_key(attempt_id: &str) -> String {
    format!("agent-task-coding-run:v1:{attempt_id}")
}

fn hash_binding_field(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn coding_run_binding_fingerprint(
    task_id: &str,
    attempt_id: &str,
    prepared: &CodingAgentPreparedStart,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.agent-task-coding-run-binding.v1\0");
    for value in [
        task_id,
        attempt_id,
        prepared.run_id.as_str(),
        prepared.runtime_project_id.as_str(),
        prepared.provider_id.as_str(),
        prepared.intent_fingerprint.as_str(),
    ] {
        hash_binding_field(&mut hasher, value);
    }
    format!("{:x}", hasher.finalize())
}

fn truncate_utf8_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn bounded_optional_terminal(value: Option<&str>) -> Option<String> {
    value.map(|value| truncate_utf8_bytes(value, MAX_AGENT_TASK_TERMINAL_TEXT_BYTES))
}

fn coding_run_observation(
    run: &CodingAgentRunSnapshot,
) -> Result<AgentTaskCodingRunObservation, ToolResult> {
    let observation_revision = i64::try_from(run.observation_revision).map_err(|_| {
        ToolResult::err_with_output(
            "CodingAgentRun observation revision exceeds the durable AgentTask range",
            json!({
                "error_kind": "agent_task_coding_run_revision_out_of_range",
                "run_id": run.run_id,
                "state_changed": false,
            }),
        )
        .with_recovery(RecoveryKind::Reconcile)
    })?;
    Ok(AgentTaskCodingRunObservation {
        run_id: run.run_id.clone(),
        runtime_project_id: run.runtime_project_id.clone(),
        provider_id: run.provider_id.clone(),
        provider_instance_id: run.provider_instance_id.clone(),
        authority_fingerprint: run.authority_fingerprint.clone(),
        coding_agent_intent_fingerprint: run.intent_fingerprint.clone(),
        run_state: run.state.clone(),
        execution_state: run.execution_state,
        observation_revision,
        terminal_stop_reason: bounded_optional_terminal(
            run.terminal
                .as_ref()
                .and_then(|terminal| terminal.stop_reason.as_deref()),
        ),
        terminal_error_code: bounded_optional_terminal(
            run.terminal
                .as_ref()
                .and_then(|terminal| terminal.error_code.as_deref()),
        ),
        terminal_message: bounded_optional_terminal(
            run.terminal
                .as_ref()
                .and_then(|terminal| terminal.message.as_deref()),
        ),
        completed_at_unix: run.terminal.as_ref().map(|terminal| terminal.completed_at),
    })
}

fn binding_execution_status(binding: &AgentTaskCodingRunBindingRecord) -> &'static str {
    match binding.dispatch_state {
        AgentTaskCodingRunDispatchState::Prepared | AgentTaskCodingRunDispatchState::NotStarted => {
            "not_started"
        }
        AgentTaskCodingRunDispatchState::OutcomeUnknown => "outcome_unknown",
        AgentTaskCodingRunDispatchState::Terminal => "terminal",
        AgentTaskCodingRunDispatchState::Bound => match binding.last_observed_run_state.as_ref() {
            Some(CodingAgentRunState::WaitingPermission) => "waiting_permission",
            Some(CodingAgentRunState::Lost) => "outcome_unknown",
            Some(
                CodingAgentRunState::Completed
                | CodingAgentRunState::Failed
                | CodingAgentRunState::Cancelled,
            ) => "terminal",
            _ => "active",
        },
    }
}

fn binding_recovery_kind(binding: &AgentTaskCodingRunBindingRecord) -> &'static str {
    match binding.dispatch_state {
        AgentTaskCodingRunDispatchState::Prepared
        | AgentTaskCodingRunDispatchState::NotStarted
        | AgentTaskCodingRunDispatchState::Terminal => "none",
        AgentTaskCodingRunDispatchState::OutcomeUnknown => "reconcile",
        AgentTaskCodingRunDispatchState::Bound => match binding.last_observed_run_state.as_ref() {
            Some(
                CodingAgentRunState::Lost
                | CodingAgentRunState::Completed
                | CodingAgentRunState::Failed
                | CodingAgentRunState::Cancelled,
            ) => "reconcile",
            _ => "observe",
        },
    }
}

fn coding_run_binding_projection(
    binding: &AgentTaskCodingRunBindingRecord,
    state_changed: bool,
    replayed: bool,
) -> Value {
    json!({
        "task_id": binding.task_id,
        "attempt_id": binding.attempt_id,
        "run_id": binding.run_id,
        "project": binding.runtime_project_id,
        "provider_id": binding.provider_id,
        "dispatch_state": binding.dispatch_state.as_str(),
        "run_state": binding.last_observed_run_state.as_ref(),
        "execution_state": binding.last_observed_execution_state,
        "execution_status": binding_execution_status(binding),
        "execution_recovery": binding_recovery_kind(binding),
        "terminal": {
            "stop_reason": binding.terminal_stop_reason,
            "error_code": binding.terminal_error_code,
            "message": binding.terminal_message,
            "completed_at": binding.completed_at_unix,
        },
        "state_changed": state_changed,
        "replayed": replayed,
    })
}

fn coding_run_failure_result(
    task_id: &str,
    attempt_id: &str,
    failure: CodingAgentStartFailure,
) -> ToolResult {
    let execution_status = match failure.certainty {
        CodingAgentStartCertainty::NotStarted => "not_started",
        CodingAgentStartCertainty::OutcomeUnknown => "outcome_unknown",
    };
    ToolResult::err_with_output(
        failure.message.clone(),
        json!({
            "error_kind": failure.kind,
            "task_id": task_id,
            "attempt_id": attempt_id,
            "run_id": failure.run_id,
            "execution_status": execution_status,
            "recovery_kind": failure.recovery.as_str(),
            "state_changed": false,
        }),
    )
    .with_recovery(failure.recovery)
}

fn coding_run_terminal_result(run: &CodingAgentRunSnapshot) -> String {
    let mut result = format!("CodingAgentRun {} {}", run.run_id, run.state.as_str());
    if let Some(terminal) = run.terminal.as_ref() {
        if let Some(message) = terminal.message.as_deref() {
            result.push_str(": ");
            result.push_str(message);
        } else if let Some(error_code) = terminal.error_code.as_deref() {
            result.push_str(": ");
            result.push_str(error_code);
        } else if let Some(stop_reason) = terminal.stop_reason.as_deref() {
            result.push_str(": ");
            result.push_str(stop_reason);
        }
    }
    truncate_utf8_bytes(&result, MAX_AGENT_TASK_TERMINAL_TEXT_BYTES)
}

fn coding_run_terminal_reason(run: &CodingAgentRunSnapshot) -> String {
    let reason = match run.state {
        CodingAgentRunState::Completed => "coding_agent_completed".to_string(),
        CodingAgentRunState::Failed => run
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.clone())
            .unwrap_or_else(|| "coding_agent_failed".to_string()),
        CodingAgentRunState::Cancelled => "coding_agent_cancelled".to_string(),
        CodingAgentRunState::Starting
        | CodingAgentRunState::Running
        | CodingAgentRunState::WaitingPermission
        | CodingAgentRunState::Lost => "coding_agent_not_terminal".to_string(),
    };
    truncate_utf8_bytes(&reason, MAX_AGENT_TASK_TERMINAL_TEXT_BYTES)
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
        if limit == 0 {
            return ToolResult::err_with_output(
                format!("limit must be 1..={MAX_AGENT_TASK_LIST_LIMIT}"),
                json!({
                    "error_kind": "invalid_agent_task_list_limit",
                    "state_changed": false,
                }),
            )
            .with_recovery(RecoveryKind::FixInput);
        }
        let limit = limit.min(MAX_AGENT_TASK_LIST_LIMIT);
        match db.list_agent_tasks(&principal, assignee_agent_id.as_deref(), offset, limit) {
            Ok(result) => {
                let mut output = match to_value(&result) {
                    Ok(value) => value,
                    Err(error) => return agent_task_serialization_error(error),
                };
                if let Some(tasks) = output.get_mut("tasks").and_then(Value::as_array_mut) {
                    for task in tasks {
                        let Some(summary) = task.as_object_mut() else {
                            continue;
                        };
                        self.attach_live_attempt_ref(&principal, summary);
                    }
                }
                ToolResult::ok(output)
            }
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
            Ok(task) => {
                let mut output = match to_value(json!({"task": task})) {
                    Ok(value) => value,
                    Err(error) => return agent_task_serialization_error(error),
                };
                if let Some(summary) = output
                    .pointer_mut("/task/summary")
                    .and_then(Value::as_object_mut)
                {
                    self.attach_live_attempt_ref(&principal, summary);
                }
                ToolResult::ok(output)
            }
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
            Ok(result) => {
                // A keyed start replay names the original start operation, whose
                // Attempt controller generation is always 1. The current Attempt
                // snapshot may report a later replacement generation, but silently
                // upgrading the selector would violate start idempotency and turn a
                // stale ref into a different continuation identity.
                let reference_generation = if result.replayed {
                    1
                } else {
                    result.attempt.attempt_controller_generation
                };
                let attempt_ref = self.issue_agent_task_attempt_ref(
                    &principal,
                    &task_id,
                    &result.attempt.attempt_id,
                    &assignee_agent_id,
                    &result.attempt_fence,
                    reference_generation,
                );
                let mut output = match to_value(&result) {
                    Ok(value) => value,
                    Err(error) => {
                        return ToolResult::err_with_output(
                            format!("Failed to serialize durable AgentTask result: {error}"),
                            json!({
                                "error_kind": "agent_task_result_serialization_failed",
                                "state_changed": false,
                            }),
                        )
                        .with_recovery(RecoveryKind::NoAction)
                    }
                };
                if let Some(attempt_ref) = attempt_ref {
                    if let Some(object) = output.as_object_mut() {
                        object.insert("attempt_ref".to_string(), Value::String(attempt_ref));
                    }
                }
                ToolResult::ok(output)
            }
            Err(error) => agent_task_error(error, RecoveryKind::RetrySame),
        }
    }

    fn issue_agent_task_attempt_ref(
        &self,
        principal: &crate::db::CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
    ) -> Option<String> {
        let db = self.communication_db.as_ref()?;
        match db.get_or_create_agent_task_attempt_reference(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            chrono::Utc::now().timestamp_millis(),
        ) {
            Ok(record) => Some(format_agent_task_attempt_ref(record.ref_index)),
            Err(error) => {
                tracing::warn!(
                    error_kind = error.code(),
                    "agent task attempt ref was not issued"
                );
                None
            }
        }
    }

    fn attach_live_attempt_ref(
        &self,
        principal: &crate::db::CommunicationPrincipal,
        summary: &mut serde_json::Map<String, Value>,
    ) {
        let Some(task_id) = summary
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let Some(attempt) = summary.get("latest_attempt").and_then(Value::as_object) else {
            return;
        };
        if attempt.get("lease_active").and_then(Value::as_bool) != Some(true) {
            return;
        }
        let Some(attempt_id) = attempt.get("attempt_id").and_then(Value::as_str) else {
            return;
        };
        let Some(assignee_agent_id) = attempt.get("assignee_agent_id").and_then(Value::as_str)
        else {
            return;
        };
        let Some(generation) = attempt
            .get("attempt_controller_generation")
            .and_then(Value::as_i64)
        else {
            return;
        };
        let Some(db) = self.communication_db.as_ref() else {
            return;
        };
        let pin = match db.live_agent_task_attempt_pin(principal, &task_id) {
            Ok(pin) => pin,
            Err(error) => {
                tracing::warn!(
                    error_kind = error.code(),
                    "agent task attempt ref was not issued for the observed Task"
                );
                return;
            }
        };
        let Some(pin) = pin else {
            return;
        };
        if pin.attempt_id != attempt_id
            || pin.assignee_agent_id != assignee_agent_id
            || pin.attempt_controller_generation != generation
        {
            return;
        }
        let Some(attempt_ref) = self.issue_agent_task_attempt_ref(
            principal,
            &task_id,
            &pin.attempt_id,
            &pin.assignee_agent_id,
            &pin.attempt_fence,
            pin.attempt_controller_generation,
        ) else {
            return;
        };
        summary.insert("attempt_ref".to_string(), Value::String(attempt_ref));
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_agent_task_attempt_selector(
        &self,
        auth: Option<&AuthContext>,
        attempt_ref: Option<String>,
        task_id: Option<String>,
        attempt_id: Option<String>,
        assignee_agent_id: Option<String>,
        attempt_fence: Option<String>,
        attempt_controller_generation: Option<i64>,
    ) -> Result<(String, String, String, String, i64), ToolResult> {
        let tuple_supplied = task_id.is_some()
            || attempt_id.is_some()
            || assignee_agent_id.is_some()
            || attempt_fence.is_some()
            || attempt_controller_generation.is_some();
        if let Some(attempt_ref) = attempt_ref {
            if tuple_supplied {
                return Err(attempt_selector_error(
                    "ambiguous_agent_task_attempt_selector",
                    "Pass attempt_ref or the exact task_id, attempt_id, assignee_agent_id, attempt_fence, and attempt_controller_generation, not both.",
                ));
            }
            let ref_index = match parse_agent_task_attempt_ref(&attempt_ref) {
                Some(index) => index,
                None => {
                    return Err(attempt_selector_error(
                        "invalid_agent_task_attempt_ref",
                        "attempt_ref must be a server-issued ~ta selector from start_agent_task_attempt, read_agent_task, or list_agent_tasks.",
                    ))
                }
            };
            let principal = match task_principal(auth) {
                Ok(principal) => principal,
                Err(result) => return Err(result),
            };
            let Some(db) = self.communication_db.as_ref() else {
                return Err(agent_task_store_unavailable());
            };
            let record = match db.lookup_agent_task_attempt_reference(&principal, ref_index) {
                Ok(record) => record,
                Err(error) => return Err(agent_task_error(error, RecoveryKind::UserAction)),
            };
            let Some(record) = record else {
                return Err(attempt_selector_error(
                    "unknown_agent_task_attempt_ref",
                    "attempt_ref does not name an Attempt for this caller. Read the owned Task or call start_agent_task_attempt again.",
                ));
            };
            return Ok((
                record.task_id,
                record.attempt_id,
                record.assignee_agent_id,
                record.attempt_fence,
                record.attempt_controller_generation,
            ));
        }
        match (
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        ) {
            (
                Some(task_id),
                Some(attempt_id),
                Some(assignee_agent_id),
                Some(attempt_fence),
                Some(attempt_controller_generation),
            ) => Ok((
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            )),
            _ => Err(attempt_selector_error(
                "incomplete_agent_task_attempt_selector",
                "Pass attempt_ref or all of task_id, attempt_id, assignee_agent_id, attempt_fence, and attempt_controller_generation.",
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_agent_task_endpoint_continuation_with_selector(
        &self,
        auth: Option<&AuthContext>,
        attempt_ref: Option<String>,
        task_id: Option<String>,
        attempt_id: Option<String>,
        assignee_agent_id: Option<String>,
        attempt_fence: Option<String>,
        attempt_controller_generation: Option<i64>,
    ) -> ToolResult {
        let (task_id, attempt_id, assignee_agent_id, attempt_fence, attempt_controller_generation) =
            match self.resolve_agent_task_attempt_selector(
                auth,
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            ) {
                Ok(selector) => selector,
                Err(result) => return result,
            };
        self.start_agent_task_endpoint_continuation(
            auth,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_agent_task_endpoint_continuation(
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
        match db.start_agent_task_endpoint_continuation(
            &principal,
            &task_id,
            &attempt_id,
            &assignee_agent_id,
            &attempt_fence,
            attempt_controller_generation,
        ) {
            Ok(result) => {
                if let Some(controller) = self.agent_continuations.as_ref() {
                    controller.schedule_agent(&assignee_agent_id);
                }
                serialized_task_success(result)
            }
            Err(error) => agent_task_error(error, RecoveryKind::RetrySame),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_agent_task_coding_run_with_selector(
        &self,
        auth: Option<&AuthContext>,
        project: String,
        attempt_ref: Option<String>,
        task_id: Option<String>,
        attempt_id: Option<String>,
        assignee_agent_id: Option<String>,
        attempt_fence: Option<String>,
        attempt_controller_generation: Option<i64>,
        provider_id: String,
        config: Option<BTreeMap<String, CodingAgentConfigValue>>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        let (task_id, attempt_id, assignee_agent_id, attempt_fence, attempt_controller_generation) =
            match self.resolve_agent_task_attempt_selector(
                auth,
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            ) {
                Ok(selector) => selector,
                Err(result) => return result,
            };
        // Resolve only identity. The canonical path still rechecks Project/provider authority,
        // live Attempt fencing and immutable binding intent before any external dispatch.
        self.start_agent_task_coding_run(
            auth,
            project,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            provider_id,
            config,
            timeout_secs,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_agent_task_coding_run(
        &self,
        auth: Option<&AuthContext>,
        project: String,
        task_id: String,
        attempt_id: String,
        assignee_agent_id: String,
        attempt_fence: String,
        attempt_controller_generation: i64,
        provider_id: String,
        config: Option<BTreeMap<String, CodingAgentConfigValue>>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        let context = match db.agent_task_coding_run_start_context(
            &principal,
            &project,
            &task_id,
            &attempt_id,
            &assignee_agent_id,
            &attempt_fence,
            attempt_controller_generation,
        ) {
            Ok(context) => context,
            Err(error) => return agent_task_error(error, RecoveryKind::Reconcile),
        };
        let replay_key = coding_run_replay_key(&attempt_id);
        let prepared = match self
            .prepare_coding_agent_start(
                project.clone(),
                provider_id,
                replay_key,
                context.task.instruction.clone(),
                config,
                timeout_secs,
                auth,
            )
            .await
        {
            Ok(prepared) => prepared,
            Err(result) => return result,
        };
        if prepared.runtime_project_id != project {
            return ToolResult::err_with_output(
                "Resolved CodingAgent Project does not match AgentTask execution intent",
                json!({
                    "error_kind": "agent_task_project_mismatch",
                    "task_id": task_id,
                    "attempt_id": attempt_id,
                    "state_changed": false,
                }),
            )
            .with_recovery(RecoveryKind::FixInput);
        }
        let binding_intent_fingerprint =
            coding_run_binding_fingerprint(&task_id, &attempt_id, &prepared);
        let binding_intent = AgentTaskCodingRunBindingIntent {
            run_id: prepared.run_id.clone(),
            runtime_project_id: prepared.runtime_project_id.clone(),
            provider_id: prepared.provider_id.clone(),
            provider_instance_id: prepared.provider_instance_id.clone(),
            authority_fingerprint: prepared.authority_fingerprint.clone(),
            coding_agent_intent_fingerprint: prepared.intent_fingerprint.clone(),
            binding_intent_fingerprint: binding_intent_fingerprint.clone(),
        };
        let prepared_binding = match db.prepare_agent_task_coding_run(
            &principal,
            &project,
            &task_id,
            &attempt_id,
            &assignee_agent_id,
            &attempt_fence,
            attempt_controller_generation,
            &binding_intent,
        ) {
            Ok(prepared) => prepared,
            Err(error) => return agent_task_error(error, RecoveryKind::Reconcile),
        };
        let claim = match db.claim_agent_task_coding_run_dispatch(
            &principal,
            &task_id,
            &attempt_id,
            &assignee_agent_id,
            &attempt_fence,
            attempt_controller_generation,
            &binding_intent_fingerprint,
        ) {
            Ok(claim) => claim,
            Err(error) => return agent_task_error(error, RecoveryKind::Reconcile),
        };
        if !claim.may_dispatch {
            return self
                .reconcile_agent_task_coding_run(auth, task_id, attempt_id)
                .await;
        }

        match self
            .dispatch_prepared_coding_agent_start(prepared, None, auth)
            .await
        {
            CodingAgentTypedStartOutcome::Run(run) => {
                let observation = match coding_run_observation(&run) {
                    Ok(observation) => observation,
                    Err(result) => return result,
                };
                match db.record_agent_task_coding_run_observation(
                    &principal,
                    &task_id,
                    &attempt_id,
                    &observation,
                ) {
                    Ok(binding) => ToolResult::ok(coding_run_binding_projection(
                        &binding,
                        false,
                        prepared_binding.replayed,
                    )),
                    Err(error) => agent_task_error(error, RecoveryKind::Reconcile),
                }
            }
            CodingAgentTypedStartOutcome::Failure(failure) => {
                if failure.certainty == CodingAgentStartCertainty::NotStarted {
                    if let Err(error) = db.record_agent_task_coding_run_not_started(
                        &principal,
                        &task_id,
                        &attempt_id,
                        &failure.run_id,
                    ) {
                        return agent_task_error(error, RecoveryKind::Reconcile);
                    }
                }
                coding_run_failure_result(&task_id, &attempt_id, failure)
            }
        }
    }

    pub(crate) async fn reconcile_agent_task_coding_run(
        &self,
        auth: Option<&AuthContext>,
        task_id: String,
        attempt_id: String,
    ) -> ToolResult {
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        let binding = match db.read_agent_task_coding_run_binding(&principal, &task_id, &attempt_id)
        {
            Ok(binding) => binding,
            Err(error) => return agent_task_error(error, RecoveryKind::Reconcile),
        };
        let snapshot = match self
            .reconcile_coding_agent_run_snapshot(&binding.run_id, auth)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(result) => return result,
        };
        let Some(run) = snapshot else {
            let binding = if matches!(
                binding.dispatch_state,
                AgentTaskCodingRunDispatchState::Bound
                    | AgentTaskCodingRunDispatchState::OutcomeUnknown
            ) {
                match db.mark_agent_task_coding_run_reconcile_unavailable(
                    &principal,
                    &task_id,
                    &attempt_id,
                ) {
                    Ok(binding) => binding,
                    Err(error) => return agent_task_error(error, RecoveryKind::Reconcile),
                }
            } else {
                binding
            };
            return ToolResult::ok(coding_run_binding_projection(&binding, false, false));
        };

        let observation = match coding_run_observation(&run) {
            Ok(observation) => observation,
            Err(result) => return result,
        };
        let observed_binding = match db.record_agent_task_coding_run_observation(
            &principal,
            &task_id,
            &attempt_id,
            &observation,
        ) {
            Ok(binding) => binding,
            Err(error) => return agent_task_error(error, RecoveryKind::Reconcile),
        };
        if observed_binding.last_observation_revision != Some(observation.observation_revision)
            || !matches!(
                observed_binding.last_observed_run_state.as_ref(),
                Some(
                    CodingAgentRunState::Completed
                        | CodingAgentRunState::Failed
                        | CodingAgentRunState::Cancelled
                )
            )
        {
            return ToolResult::ok(coding_run_binding_projection(
                &observed_binding,
                false,
                false,
            ));
        }

        let terminal_result = coding_run_terminal_result(&run);
        let terminal_reason = coding_run_terminal_reason(&run);
        match db.terminalize_agent_task_coding_run(
            &principal,
            &task_id,
            &attempt_id,
            &observation,
            Some(&terminal_result),
            Some(&terminal_reason),
        ) {
            Ok(mutation) => {
                if mutation.state_changed {
                    if let Some(controller) = self.agent_continuations.as_ref() {
                        for agent_id in &mutation.attention_target_agent_ids {
                            controller.schedule_agent(agent_id);
                        }
                    }
                }
                if mutation.state_changed {
                    if let Some(controller) = self.agent_continuations.as_ref() {
                        for agent_id in &mutation.wait_target_agent_ids {
                            controller.schedule_agent(agent_id);
                        }
                    }
                }
                let mut output =
                    coding_run_binding_projection(&mutation.binding, mutation.state_changed, false);
                if let Some(object) = output.as_object_mut() {
                    object.insert(
                        "task_state".to_string(),
                        json!(mutation.task.state.as_str()),
                    );
                    object.insert(
                        "attempt_state".to_string(),
                        json!(mutation.attempt.state.as_str()),
                    );
                }
                ToolResult::ok(output)
            }
            Err(error) => agent_task_error(error, RecoveryKind::Reconcile),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn heartbeat_agent_task_attempt_with_selector(
        &self,
        auth: Option<&AuthContext>,
        attempt_ref: Option<String>,
        task_id: Option<String>,
        attempt_id: Option<String>,
        assignee_agent_id: Option<String>,
        attempt_fence: Option<String>,
        attempt_controller_generation: Option<i64>,
        active_turn_wake_id: Option<String>,
        active_turn_consume_token: Option<String>,
    ) -> ToolResult {
        let (task_id, attempt_id, assignee_agent_id, attempt_fence, attempt_controller_generation) =
            match self.resolve_agent_task_attempt_selector(
                auth,
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            ) {
                Ok(selector) => selector,
                Err(result) => return result,
            };
        self.heartbeat_agent_task_attempt(
            auth,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            active_turn_wake_id,
            active_turn_consume_token,
        )
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
        active_turn_wake_id: Option<String>,
        active_turn_consume_token: Option<String>,
    ) -> ToolResult {
        if active_turn_wake_id.is_some() != active_turn_consume_token.is_some() {
            return ToolResult::err_with_output(
                "active_turn_wake_id and active_turn_consume_token must be provided together",
                json!({
                    "error_kind": "invalid_agent_task_active_turn_proof",
                    "state_changed": false,
                }),
            )
            .with_recovery(RecoveryKind::FixInput);
        }
        let principal = match task_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return agent_task_store_unavailable();
        };
        match db.heartbeat_agent_task_attempt_with_active_turn_proof(
            &principal,
            &task_id,
            &attempt_id,
            &assignee_agent_id,
            &attempt_fence,
            attempt_controller_generation,
            active_turn_wake_id.as_deref(),
            active_turn_consume_token.as_deref(),
        ) {
            Ok(result) => serialized_task_success(result),
            Err(error) => agent_task_error(error, RecoveryKind::Reconcile),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn complete_agent_task_attempt_with_selector(
        &self,
        auth: Option<&AuthContext>,
        attempt_ref: Option<String>,
        task_id: Option<String>,
        attempt_id: Option<String>,
        assignee_agent_id: Option<String>,
        attempt_fence: Option<String>,
        attempt_controller_generation: Option<i64>,
        outcome: String,
        terminal_result: Option<String>,
        terminal_reason: Option<String>,
        completion_key: String,
    ) -> ToolResult {
        let (task_id, attempt_id, assignee_agent_id, attempt_fence, attempt_controller_generation) =
            match self.resolve_agent_task_attempt_selector(
                auth,
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            ) {
                Ok(selector) => selector,
                Err(result) => return result,
            };
        self.complete_agent_task_attempt(
            auth,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
            outcome,
            terminal_result,
            terminal_reason,
            completion_key,
        )
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
                .with_recovery(RecoveryKind::FixInput)
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
            Ok(result) => {
                if result.state_changed {
                    if let Some(controller) = self.agent_continuations.as_ref() {
                        for agent_id in &result.attention_target_agent_ids {
                            controller.schedule_agent(agent_id);
                        }
                    }
                }
                if result.state_changed {
                    if let Some(controller) = self.agent_continuations.as_ref() {
                        for agent_id in &result.wait_target_agent_ids {
                            controller.schedule_agent(agent_id);
                        }
                    }
                }
                serialized_task_success(result)
            }
            Err(error) => agent_task_error(error, RecoveryKind::RetrySame),
        }
    }
}

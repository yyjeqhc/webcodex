use super::communication::{
    agent_continuation_projection, communication_error, communication_principal,
    communication_store_unavailable, serialized_success,
};
use super::{AgentWaitEventSelectorCall, AgentWaitModeCall, RecoveryKind, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::db::{AgentWaitEventSelector, AgentWaitMode, NewAgentWait};
use serde_json::json;

/// Project only after canonical recording. Store, audit and the App-only state
/// reader keep the complete durable Wait, including revision and target fences.
pub(super) fn agent_wait_model_projection(result: &mut ToolResult) {
    if !result.success {
        return;
    }
    let Some(wait) = result
        .output
        .get_mut("agent_wait")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    wait.retain(|key, _| {
        matches!(
            key.as_str(),
            "wait_id"
                | "goal_id"
                | "state"
                | "mode"
                | "source_count"
                | "match_count"
                | "sources"
                | "matches"
        )
    });
    if let Some(sources) = wait
        .get_mut("sources")
        .and_then(serde_json::Value::as_array_mut)
    {
        for source in sources {
            if let Some(source) = source.as_object_mut() {
                source.retain(|key, _| matches!(key.as_str(), "kind" | "task_id"));
            }
        }
    }
    if let Some(matches) = wait
        .get_mut("matches")
        .and_then(serde_json::Value::as_array_mut)
    {
        for matched in matches {
            if let Some(matched) = matched.as_object_mut() {
                matched.retain(|key, _| {
                    matches!(
                        key.as_str(),
                        "task_id" | "task_attempt_id" | "terminal_task_state"
                    )
                });
            }
        }
    }
}

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn wait_for_agent_events(
        &self,
        auth: Option<&AuthContext>,
        agent_id: String,
        endpoint_id: String,
        expected_controller_generation: i64,
        mode: AgentWaitModeCall,
        goal_id: Option<String>,
        events: Vec<AgentWaitEventSelectorCall>,
        idempotency_key: String,
    ) -> ToolResult {
        let principal = match communication_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return communication_store_unavailable();
        };
        let mode = match mode {
            AgentWaitModeCall::Any => AgentWaitMode::Any,
            AgentWaitModeCall::All => AgentWaitMode::All,
        };
        let events = events
            .into_iter()
            .map(|event| AgentWaitEventSelector {
                kind: event.kind,
                task_id: event.task_id,
            })
            .collect();
        let mutation = match db.create_agent_wait(
            &principal,
            NewAgentWait {
                target_agent_id: agent_id.clone(),
                goal_id,
                endpoint_id: endpoint_id.clone(),
                expected_controller_generation,
                mode,
                events,
                idempotency_key,
            },
        ) {
            Ok(mutation) => mutation,
            Err(error) => return communication_error(error, RecoveryKind::RetrySame),
        };
        if mutation.schedule_required {
            if let Some(controller) = self.agent_continuations.as_ref() {
                controller.schedule_agent(&agent_id);
            }
        }

        let bootstrap = match db.bootstrap_agent_conversation(
            &principal,
            &agent_id,
            &endpoint_id,
            expected_controller_generation,
            None,
            None,
        ) {
            Ok(bootstrap) => bootstrap,
            Err(error) => return communication_error(error, RecoveryKind::Reconcile),
        };
        let binding = self
            .agent_continuations
            .as_ref()
            .map(|controller| {
                controller.binding_status(&agent_id, &endpoint_id, expected_controller_generation)
            })
            .unwrap_or(crate::agent_wake::AgentHostBindingStatus {
                adapter_registered: false,
                adapter_kind: None,
                production_auto_resume_available: false,
            });
        let observation = self.agent_continuations.as_ref().and_then(|controller| {
            controller.mcp_app_binding_observation(
                &agent_id,
                &endpoint_id,
                expected_controller_generation,
            )
        });
        let projection = agent_continuation_projection(bootstrap, binding, observation, None);
        ToolResult::ok(json!({
            "agent_wait": mutation.agent_wait,
            "agent_continuation": projection["agent_continuation"].clone(),
            "replayed": mutation.replayed,
            "state_changed": mutation.state_changed,
        }))
    }

    pub(crate) fn read_agent_wait(
        &self,
        auth: Option<&AuthContext>,
        wait_id: String,
    ) -> ToolResult {
        let principal = match communication_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return communication_store_unavailable();
        };
        match db.read_agent_wait(&principal, &wait_id) {
            Ok(agent_wait) => serialized_success(json!({"agent_wait": agent_wait})),
            Err(error) => communication_error(error, RecoveryKind::Reobserve),
        }
    }

    pub(crate) fn list_goal_agent_waits_for_console(
        &self,
        auth: Option<&AuthContext>,
        goal_id: String,
    ) -> ToolResult {
        let principal = match communication_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return communication_store_unavailable();
        };
        match db.list_agent_waits_for_goal(
            &principal,
            &goal_id,
            crate::db::MAX_GOAL_AGENT_WAIT_LIST_LIMIT,
        ) {
            Ok((waits, truncated)) => serialized_success(json!({
                "waits": waits,
                "truncated": truncated,
            })),
            Err(error) => communication_error(error, RecoveryKind::Reobserve),
        }
    }

    pub(crate) fn cancel_agent_wait(
        &self,
        auth: Option<&AuthContext>,
        wait_id: String,
        idempotency_key: String,
    ) -> ToolResult {
        let principal = match communication_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return communication_store_unavailable();
        };
        match db.cancel_agent_wait(&principal, &wait_id, &idempotency_key) {
            Ok(mutation) => serialized_success(json!({
                "agent_wait": mutation.agent_wait,
                "replayed": mutation.replayed,
                "state_changed": mutation.state_changed,
            })),
            Err(error) => communication_error(error, RecoveryKind::Reconcile),
        }
    }

    pub(crate) fn agent_wait_state(
        &self,
        auth: Option<&AuthContext>,
        wait_id: String,
    ) -> ToolResult {
        self.read_agent_wait(auth, wait_id)
    }
}

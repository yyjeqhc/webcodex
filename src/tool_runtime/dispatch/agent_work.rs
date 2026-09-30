//! Authorized agent work routing; outer governance owns admission.

use super::*;

impl ToolRuntime {
    pub(super) async fn dispatch_agent_work_authorized(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::WaitForAgentEvents {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                mode,
                goal_id,
                events,
                idempotency_key,
            } => self.wait_for_agent_events(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                mode,
                goal_id,
                events,
                idempotency_key,
            ),

            ToolCall::ReadAgentWait { wait_id } => self.read_agent_wait(auth, wait_id),

            ToolCall::CancelAgentWait {
                wait_id,
                idempotency_key,
            } => self.cancel_agent_wait(auth, wait_id, idempotency_key),

            ToolCall::AgentWaitState { wait_id } => self.agent_wait_state(auth, wait_id),

            ToolCall::CreateAgentTask {
                title,
                instruction,
                assignee_agent_id,
                source_conversation_id,
                source_message_id,
                referenced_project_id,
                idempotency_key,
            } => self.create_agent_task(
                auth,
                title,
                instruction,
                assignee_agent_id,
                source_conversation_id,
                source_message_id,
                referenced_project_id,
                idempotency_key,
            ),

            ToolCall::ListAgentTasks {
                assignee_agent_id,
                offset,
                limit,
            } => self.list_agent_tasks(auth, assignee_agent_id, offset, limit),

            ToolCall::ReadAgentTask { task_id } => self.read_agent_task(auth, task_id),

            ToolCall::AssignAgentTask {
                task_id,
                assignee_agent_id,
            } => self.assign_agent_task(auth, task_id, assignee_agent_id),

            ToolCall::StartAgentTaskAttempt {
                task_id,
                assignee_agent_id,
                idempotency_key,
            } => self.start_agent_task_attempt(auth, task_id, assignee_agent_id, idempotency_key),

            ToolCall::StartAgentTaskEndpointContinuation {
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            } => self.start_agent_task_endpoint_continuation_with_selector(
                auth,
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            ),

            ToolCall::StartAgentTaskCodingRun {
                project,
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                provider_id,
                config,
                timeout_secs,
            } => {
                // Keep this relatively large orchestration future off the shared dispatch
                // future's inline state so unrelated tool calls do not inherit its stack cost.
                Box::pin(self.start_agent_task_coding_run_with_selector(
                    auth,
                    project,
                    attempt_ref,
                    task_id,
                    attempt_id,
                    assignee_agent_id,
                    attempt_fence,
                    attempt_controller_generation,
                    provider_id,
                    config,
                    timeout_secs,
                ))
                .await
            }

            ToolCall::ReconcileAgentTaskCodingRun {
                task_id,
                attempt_id,
            } => Box::pin(self.reconcile_agent_task_coding_run(auth, task_id, attempt_id)).await,

            ToolCall::HeartbeatAgentTaskAttempt {
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                active_turn_wake_id,
                active_turn_consume_token,
            } => self.heartbeat_agent_task_attempt_with_selector(
                auth,
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                active_turn_wake_id,
                active_turn_consume_token,
            ),

            ToolCall::CompleteAgentTaskAttempt {
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                outcome,
                terminal_result,
                terminal_reason,
                completion_key,
            } => self.complete_agent_task_attempt_with_selector(
                auth,
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                outcome,
                terminal_result,
                terminal_reason,
                completion_key,
            ),
            _ => ToolResult::err("tool does not belong to the agent_work dispatch family"),
        }
    }
}

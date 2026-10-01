//! Authorized goals routing; outer governance owns admission.

use super::*;

impl ToolRuntime {
    pub(super) async fn dispatch_goals_authorized(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        match call {
            ToolCall::PrepareGoalWorkflow {
                session_id,
                title,
                objective,
                controller_agent_id,
                completion_conditions,
                steps,
                idempotency_key,
            } => {
                self.prepare_goal_workflow(
                    auth,
                    session_id,
                    crate::db::NewGoal {
                        title,
                        objective,
                        controller_agent_id,
                        completion_conditions,
                        idempotency_key,
                        steps: steps
                            .into_iter()
                            .map(|step| crate::db::NewGoalStep {
                                id: step.id,
                                title: step.title,
                            })
                            .collect(),
                    },
                )
                .await
            }

            ToolCall::CreateGoal {
                title,
                objective,
                controller_agent_id,
                completion_conditions,
                steps,
                idempotency_key,
            } => self.create_goal_with_plan(
                auth,
                crate::db::NewGoal {
                    title,
                    objective,
                    controller_agent_id,
                    completion_conditions,
                    idempotency_key,
                    steps: steps
                        .into_iter()
                        .map(|step| crate::db::NewGoalStep {
                            id: step.id,
                            title: step.title,
                        })
                        .collect(),
                },
            ),

            ToolCall::CheckpointGoal {
                goal_id,
                expected_revision,
                completed_step_ids,
                current_step_id,
                summary,
                idempotency_key,
            } => self.checkpoint_goal(
                auth,
                goal_id,
                expected_revision,
                crate::db::GoalCheckpoint {
                    completed_step_ids,
                    current_step_id,
                    summary,
                },
                idempotency_key,
            ),

            ToolCall::GetGoal { goal_id } => self.get_goal(auth, goal_id),

            ToolCall::PresentGoalPlan { goal_id } => self.present_goal_plan(auth, goal_id).await,

            ToolCall::GoalPlanSync { goal_id } => {
                self.goal_plan_sync_for_window(auth, window, goal_id).await
            }

            ToolCall::ListGoals {
                query,
                lifecycle,
                offset,
                limit,
            } => self.list_goals(
                auth,
                lifecycle.map(|value| value.as_str().to_string()),
                query,
                offset,
                limit,
            ),

            ToolCall::UpdateGoal {
                goal_id,
                expected_revision,
                title,
                objective,
                controller_agent_id,
                lifecycle,
                terminal_reason,
                idempotency_key,
            } => self.update_goal_with_controller(
                auth,
                goal_id,
                expected_revision,
                title,
                objective,
                controller_agent_id,
                lifecycle.map(|value| value.as_str().to_string()),
                terminal_reason,
                idempotency_key,
            ),

            ToolCall::AssociateGoalAgentTask {
                goal_id,
                task_id,
                idempotency_key,
            } => self.associate_goal_agent_task(auth, goal_id, task_id, idempotency_key),

            ToolCall::AssociateGoalWorkflowSession {
                goal_id,
                session_id,
                idempotency_key,
            } => {
                self.associate_goal_workflow_session(auth, goal_id, session_id, idempotency_key)
                    .await
            }
            _ => ToolResult::err("tool does not belong to the goals dispatch family"),
        }
    }
}

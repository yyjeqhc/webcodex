//! Authorized agents routing; outer governance owns admission.

use super::*;

impl ToolRuntime {
    pub(super) async fn dispatch_agents_authorized(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        match call {
            ToolCall::CreateAgentIdentity(input) => self.create_agent_identity(auth, input),

            ToolCall::ListAgentIdentities(input) => self.list_agent_identities(auth, input),

            ToolCall::UpdateAgentIdentity(input) => self.update_agent_identity(auth, input),

            ToolCall::RotateAgentContinuationEndpoint(input) => {
                self.attach_agent_endpoint(auth, input)
            }

            ToolCall::PresentAgentContinuation {
                agent_continuation_ref,
                agent_id,
                endpoint_id,
                expected_controller_generation,
            } => self.present_agent_continuation_with_selector(
                auth,
                agent_continuation_ref,
                agent_id,
                endpoint_id,
                expected_controller_generation,
            ),

            ToolCall::AgentContinuationBind {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_bind_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::AgentContinuationRecoverEndpoint {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_recover_endpoint_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::AgentContinuationState {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_state_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::AgentContinuationWakeAcquire {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_wake_acquire_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::AgentContinuationWakePrepare {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
                wake_id,
                attempt_id,
            } => self.agent_continuation_wake_prepare_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
                wake_id,
                attempt_id,
            ),

            ToolCall::AgentContinuationWakeFinish {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
                wake_id,
                attempt_id,
                outcome,
            } => self.agent_continuation_wake_finish_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
                wake_id,
                attempt_id,
                outcome,
            ),

            ToolCall::AgentContinuationUnbind {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_unbind_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::DetachAgentEndpoint(input) => self.detach_agent_endpoint(auth, input),

            ToolCall::CreateConversation(input) => self.create_conversation(auth, input),

            ToolCall::ListConversations(input) => self.list_conversations(auth, input),

            ToolCall::ReadConversation(input) => self.read_conversation(auth, input),

            ToolCall::PostConversationMessage(input) => self.post_conversation_message(auth, input),

            ToolCall::ListAgentInbox(input) => self.list_agent_inbox(auth, input),

            ToolCall::ConsumeAgentDeliveries(input) => self.consume_agent_deliveries(auth, input),

            ToolCall::BootstrapAgentConversation {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                conversation_id,
                wake_id,
                activation_idempotency_key,
            } => self.bootstrap_agent_conversation(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                conversation_id,
                wake_id,
                activation_idempotency_key,
            ),

            ToolCall::ConsumeAgentWake {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                wake_id,
                consume_token,
            } => self.consume_agent_wake(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                wake_id,
                consume_token,
            ),
            _ => ToolResult::err("tool does not belong to the agents dispatch family"),
        }
    }
}

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
            ToolCall::CreateAgentIdentity {
                handle,
                display_name,
                description,
                specialty_labels,
                idempotency_key,
            } => self.create_agent_identity(
                auth,
                handle,
                display_name,
                description,
                specialty_labels,
                idempotency_key,
            ),

            ToolCall::ListAgentIdentities {
                agent_id,
                offset,
                limit,
            } => self.list_agent_identities(auth, agent_id, offset, limit),

            ToolCall::UpdateAgentIdentity {
                agent_id,
                expected_profile_revision,
                handle,
                display_name,
                description,
                specialty_labels,
            } => self.update_agent_identity(
                auth,
                agent_id,
                expected_profile_revision,
                handle,
                display_name,
                description,
                specialty_labels,
            ),

            ToolCall::RotateAgentContinuationEndpoint {
                agent_id,
                host,
                client_attachment_id,
                idempotency_key,
            } => self.attach_agent_endpoint(
                auth,
                agent_id,
                host,
                client_attachment_id,
                idempotency_key,
            ),
            #[cfg(feature = "legacy-gpt-actions")]
            ToolCall::AttachAgentEndpoint {
                agent_id,
                host,
                client_attachment_id,
                idempotency_key,
            } => self.attach_agent_endpoint(
                auth,
                agent_id,
                host,
                client_attachment_id,
                idempotency_key,
            ),

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

            ToolCall::DetachAgentEndpoint { endpoint_id } => {
                self.detach_agent_endpoint(auth, endpoint_id)
            }

            ToolCall::CreateConversation {
                title,
                agent_ids,
                idempotency_key,
            } => self.create_conversation(auth, title, agent_ids, idempotency_key),

            ToolCall::ListConversations {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                offset,
                limit,
            } => self.list_conversations(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                offset,
                limit,
            ),

            ToolCall::ReadConversation {
                conversation_id,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_seq,
                limit,
            } => self.read_conversation(
                auth,
                conversation_id,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_seq,
                limit,
            ),

            ToolCall::PostConversationMessage {
                conversation_id,
                body,
                author_agent_id,
                endpoint_id,
                expected_controller_generation,
                recipient_agent_ids,
                reply_to,
                idempotency_key,
                wake_reply_id,
                reply_operation_index,
            } => self.post_conversation_message(
                auth,
                conversation_id,
                body,
                author_agent_id,
                endpoint_id,
                expected_controller_generation,
                recipient_agent_ids,
                reply_to,
                idempotency_key,
                wake_reply_id,
                reply_operation_index,
            ),

            ToolCall::ListAgentInbox {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_delivery_order,
                limit,
            } => self.list_agent_inbox(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_delivery_order,
                limit,
            ),

            ToolCall::ConsumeAgentDeliveries {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                delivery_ids,
            } => self.consume_agent_deliveries(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                delivery_ids,
            ),

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

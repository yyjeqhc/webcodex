use salvo::prelude::*;
use serde::Deserialize;

use salvo::writing::Scribe;
use serde_json::Value;
use webcodex_tool_contracts::tool_call::{
    ConsumeAgentDeliveriesInput, CreateAgentIdentityInput, CreateConversationInput,
    DetachAgentEndpointInput, ListAgentIdentitiesInput, ListAgentInboxInput,
    ListConversationsInput, PostConversationMessageInput, ReadConversationInput,
    RotateAgentContinuationEndpointInput, UpdateAgentIdentityInput,
};

use super::{
    parse_console_json, prepared, require_communication_manage, require_communication_read,
    RuntimeConsoleError,
};

pub(super) struct CommunicationResponse(pub(super) crate::tool_runtime::ToolResult);

impl Scribe for CommunicationResponse {
    fn render(self, res: &mut Response) {
        let result = self.0;
        if result.success {
            res.render(Json(result.output));
            return;
        }
        let error_kind = result
            .output
            .get("error_kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let status = match error_kind {
            "communication_store_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
            "communication_idempotency_conflict"
            | "agent_profile_changed"
            | "conversation_closed" => StatusCode::CONFLICT,
            "agent_not_found"
            | "endpoint_not_found"
            | "conversation_not_found"
            | "message_not_found"
            | "reply_message_not_found"
            | "delivery_not_found" => StatusCode::NOT_FOUND,
            "communication_principal_unavailable" => StatusCode::FORBIDDEN,
            _ => StatusCode::BAD_REQUEST,
        };
        res.status_code(status);
        res.render(Json(result.output));
    }
}

type CommunicationResult = Result<CommunicationResponse, RuntimeConsoleError>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationEndpointRenewInput {
    endpoint_id: String,
    expected_controller_generation: i64,
}

#[handler]
pub(super) async fn communication_agents(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_read(&auth)?;
    let input: ListAgentIdentitiesInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.list_agent_identities(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_agent_create(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: CreateAgentIdentityInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.create_agent_identity(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_agent_update(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: UpdateAgentIdentityInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.update_agent_identity(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_endpoint_attach(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: RotateAgentContinuationEndpointInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.attach_agent_endpoint(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_endpoint_renew(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: CommunicationEndpointRenewInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.renew_agent_endpoint(
        Some(&auth),
        input.endpoint_id,
        input.expected_controller_generation,
    )))
}

#[handler]
pub(super) async fn communication_endpoint_detach(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: DetachAgentEndpointInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.detach_agent_endpoint(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_conversations(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_read(&auth)?;
    let input: ListConversationsInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.list_conversations(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_conversation_create(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: CreateConversationInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.create_conversation(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_conversation(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_read(&auth)?;
    let input: ReadConversationInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.read_conversation(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_message_post(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: PostConversationMessageInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.post_conversation_message(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_inbox(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_read(&auth)?;
    let input: ListAgentInboxInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.list_agent_inbox(Some(&auth), input),
    ))
}

#[handler]
pub(super) async fn communication_inbox_consume(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: ConsumeAgentDeliveriesInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.consume_agent_deliveries(Some(&auth), input),
    ))
}

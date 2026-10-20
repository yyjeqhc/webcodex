use salvo::prelude::*;
use serde::Deserialize;

use salvo::writing::Scribe;
use serde_json::Value;

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
struct CommunicationAgentsInput {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationAgentCreateInput {
    handle: String,
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    specialty_labels: Vec<String>,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationAgentUpdateInput {
    agent_id: String,
    expected_profile_revision: i64,
    #[serde(default)]
    handle: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    specialty_labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationEndpointAttachInput {
    agent_id: String,
    host: String,
    #[serde(default)]
    client_attachment_id: Option<String>,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationEndpointDetachInput {
    endpoint_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationEndpointRenewInput {
    endpoint_id: String,
    expected_controller_generation: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationConversationsInput {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    endpoint_id: Option<String>,
    #[serde(default)]
    expected_controller_generation: Option<i64>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationConversationCreateInput {
    #[serde(default)]
    title: Option<String>,
    agent_ids: Vec<String>,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationConversationInput {
    conversation_id: String,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    endpoint_id: Option<String>,
    #[serde(default)]
    expected_controller_generation: Option<i64>,
    #[serde(default)]
    after_seq: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationMessagePostInput {
    conversation_id: String,
    body: String,
    #[serde(default)]
    author_agent_id: Option<String>,
    #[serde(default)]
    endpoint_id: Option<String>,
    #[serde(default)]
    expected_controller_generation: Option<i64>,
    #[serde(default)]
    recipient_agent_ids: Option<Vec<String>>,
    #[serde(default)]
    reply_to: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    wake_reply_id: Option<String>,
    #[serde(default)]
    reply_operation_index: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationInboxInput {
    agent_id: String,
    endpoint_id: String,
    expected_controller_generation: i64,
    #[serde(default)]
    after_delivery_order: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationInboxConsumeInput {
    agent_id: String,
    endpoint_id: String,
    expected_controller_generation: i64,
    delivery_ids: Vec<String>,
}

#[handler]
pub(super) async fn communication_agents(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_read(&auth)?;
    let input: CommunicationAgentsInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.list_agent_identities(
        Some(&auth),
        input.agent_id,
        input.offset,
        input.limit,
    )))
}

#[handler]
pub(super) async fn communication_agent_create(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: CommunicationAgentCreateInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.create_agent_identity(
        Some(&auth),
        input.handle,
        input.display_name,
        input.description,
        input.specialty_labels,
        input.idempotency_key,
    )))
}

#[handler]
pub(super) async fn communication_agent_update(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: CommunicationAgentUpdateInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.update_agent_identity(
        Some(&auth),
        input.agent_id,
        input.expected_profile_revision,
        input.handle,
        input.display_name,
        input.description,
        input.specialty_labels,
    )))
}

#[handler]
pub(super) async fn communication_endpoint_attach(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: CommunicationEndpointAttachInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.attach_agent_endpoint(
        Some(&auth),
        input.agent_id,
        input.host,
        input.client_attachment_id,
        input.idempotency_key,
    )))
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
    let input: CommunicationEndpointDetachInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(
        runtime.detach_agent_endpoint(Some(&auth), input.endpoint_id),
    ))
}

#[handler]
pub(super) async fn communication_conversations(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_read(&auth)?;
    let input: CommunicationConversationsInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.list_conversations(
        Some(&auth),
        input.agent_id,
        input.endpoint_id,
        input.expected_controller_generation,
        input.offset,
        input.limit,
    )))
}

#[handler]
pub(super) async fn communication_conversation_create(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: CommunicationConversationCreateInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.create_conversation(
        Some(&auth),
        input.title,
        input.agent_ids,
        input.idempotency_key,
    )))
}

#[handler]
pub(super) async fn communication_conversation(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_read(&auth)?;
    let input: CommunicationConversationInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.read_conversation(
        Some(&auth),
        input.conversation_id,
        input.agent_id,
        input.endpoint_id,
        input.expected_controller_generation,
        input.after_seq,
        input.limit,
    )))
}

#[handler]
pub(super) async fn communication_message_post(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: CommunicationMessagePostInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.post_conversation_message(
        Some(&auth),
        input.conversation_id,
        input.body,
        input.author_agent_id,
        input.endpoint_id,
        input.expected_controller_generation,
        input.recipient_agent_ids,
        input.reply_to,
        input.idempotency_key,
        input.wake_reply_id,
        input.reply_operation_index,
    )))
}

#[handler]
pub(super) async fn communication_inbox(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_read(&auth)?;
    let input: CommunicationInboxInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.list_agent_inbox(
        Some(&auth),
        input.agent_id,
        input.endpoint_id,
        input.expected_controller_generation,
        input.after_delivery_order,
        input.limit,
    )))
}

#[handler]
pub(super) async fn communication_inbox_consume(
    req: &mut Request,
    depot: &mut Depot,
) -> CommunicationResult {
    let (runtime, auth) = prepared(req, depot).await?;
    require_communication_manage(&auth)?;
    let input: CommunicationInboxConsumeInput = parse_console_json(req).await?;
    Ok(CommunicationResponse(runtime.consume_agent_deliveries(
        Some(&auth),
        input.agent_id,
        input.endpoint_id,
        input.expected_controller_generation,
        input.delivery_ids,
    )))
}

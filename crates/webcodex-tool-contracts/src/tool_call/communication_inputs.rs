// Shared communication payloads for tool calls and the Console.
// Authorization and transport context remain separate from these inputs.

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateAgentIdentityInput {
    /// Mutable non-unique Agent handle. It is self-description metadata, not canonical identity or
    /// authority.
    #[schemars(length(min = 1, max = 64))]
    #[schemars(regex(pattern = "^[A-Za-z0-9._-]+$"))]
    pub handle: String,
    /// Mutable non-unique Agent display name.
    #[schemars(length(min = 1, max = 128))]
    pub display_name: String,
    #[schemars(extend("default" = ""))]
    /// Optional Agent Card description, bounded by 2048 UTF-8 bytes server-side.
    #[schemars(length(max = 2048))]
    #[serde(default)]
    pub description: Option<String>,
    /// Bounded self-description labels. Labels never grant authority.
    #[schemars(length(max = 16))]
    #[schemars(inner(length(min = 1, max = 64)))]
    #[serde(default)]
    pub specialty_labels: Vec<String>,
    /// Caller-generated operation key. Exact replay under the same communication principal returns the
    /// original durable resource; reuse with changed input is rejected.
    #[schemars(length(min = 1, max = 128))]
    pub idempotency_key: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListAgentIdentitiesInput {
    /// Optional exact canonical Agent id owned by the current communication principal.
    #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
    #[serde(default)]
    pub agent_id: Option<String>,
    #[schemars(extend("default" = 0))]
    /// Zero-based bounded page offset.
    #[schemars(range(min = 0))]
    #[serde(default)]
    pub offset: Option<usize>,
    #[schemars(extend("default" = 50))]
    /// Maximum records to return. Values above 100 are accepted and clamped to 100.
    #[schemars(range(min = 1))]
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateAgentIdentityInput {
    /// Canonical durable Agent id to update.
    #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
    pub agent_id: String,
    /// Exact profile revision fence. A stale value is rejected without mutation.
    #[schemars(range(min = 1))]
    pub expected_profile_revision: i64,
    #[schemars(length(min = 1, max = 64))]
    #[schemars(regex(pattern = "^[A-Za-z0-9._-]+$"))]
    #[serde(default)]
    pub handle: Option<String>,
    /// Replacement display name.
    #[schemars(length(min = 1, max = 128))]
    #[serde(default)]
    pub display_name: Option<String>,
    /// Replacement description, bounded by 2048 UTF-8 bytes server-side.
    #[schemars(length(max = 2048))]
    #[serde(default)]
    pub description: Option<String>,
    /// Bounded self-description labels. Labels never grant authority.
    #[schemars(length(max = 16))]
    #[schemars(inner(length(min = 1, max = 64)))]
    #[serde(default)]
    pub specialty_labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotateAgentContinuationEndpointInput {
    /// Canonical durable Agent id owned by the current communication principal.
    #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
    pub agent_id: String,
    /// Server-local continuation adapter label, for example ChatGPT. This is recorded metadata only: it
    /// does not initiate, configure, or authorize an external connection.
    #[schemars(length(min = 1, max = 64))]
    pub host: String,
    /// Optional opaque host-local attachment label copied into server-local Endpoint metadata. It is
    /// not durable Agent identity, remote-host authority, or a connection selector.
    #[schemars(length(max = 128))]
    #[serde(default)]
    pub client_attachment_id: Option<String>,
    /// Caller-generated operation key. Exact replay under the same communication principal returns the
    /// original durable resource; reuse with changed input is rejected.
    #[schemars(length(min = 1, max = 128))]
    pub idempotency_key: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DetachAgentEndpointInput {
    /// Canonical Agent Endpoint id attached by the current communication principal.
    #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
    pub endpoint_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateConversationInput {
    /// Optional mutable room title.
    #[schemars(length(max = 200))]
    #[serde(default)]
    pub title: Option<String>,
    /// Owned Agent participants to add with the current Human principal.
    #[schemars(length(min = 1, max = 16))]
    #[schemars(inner(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$")))]
    pub agent_ids: Vec<String>,
    /// Caller-generated operation key. Exact replay under the same communication principal returns the
    /// original durable resource; reuse with changed input is rejected.
    #[schemars(length(min = 1, max = 128))]
    pub idempotency_key: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListConversationsInput {
    /// Optional Agent view. Must be paired with exact Endpoint fencing; omit all three fields for the
    /// current Human principal.
    #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Active Endpoint proving the optional Agent view.
    #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
    #[serde(default)]
    pub endpoint_id: Option<String>,
    /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
    #[schemars(range(min = 1))]
    #[serde(default)]
    pub expected_controller_generation: Option<i64>,
    #[schemars(extend("default" = 0))]
    /// Zero-based bounded page offset.
    #[schemars(range(min = 0))]
    #[serde(default)]
    pub offset: Option<usize>,
    #[schemars(extend("default" = 50))]
    /// Maximum records to return. Values above 100 are accepted and clamped to 100.
    #[schemars(range(min = 1))]
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadConversationInput {
    /// Canonical Conversation id.
    #[schemars(regex(pattern = "^wc_conv_[A-Za-z0-9_-]{16}$"))]
    pub conversation_id: String,
    /// Optional Agent view. Must be paired with exact Endpoint fencing; omit all three fields for the
    /// current Human principal.
    #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Active Endpoint proving the optional Agent view.
    #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
    #[serde(default)]
    pub endpoint_id: Option<String>,
    /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
    #[schemars(range(min = 1))]
    #[serde(default)]
    pub expected_controller_generation: Option<i64>,
    #[schemars(extend("default" = 0))]
    /// Return append-only transcript messages with seq greater than this cursor.
    #[schemars(range(min = 0))]
    #[serde(default)]
    pub after_seq: Option<i64>,
    #[schemars(extend("default" = 50))]
    /// Maximum records to return. Values above 100 are accepted and clamped to 100.
    #[schemars(range(min = 1))]
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PostConversationMessageInput {
    /// Canonical Conversation id.
    #[schemars(regex(pattern = "^wc_conv_[A-Za-z0-9_-]{16}$"))]
    pub conversation_id: String,
    /// Append-only message body, bounded by 4096 UTF-8 bytes server-side.
    #[schemars(length(min = 1, max = 4096))]
    pub body: String,
    /// Agent author provenance. Omit for the current Human principal; Agent authors require
    /// endpoint_id.
    #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
    #[serde(default)]
    pub author_agent_id: Option<String>,
    /// Active Endpoint proving an Agent-authored message. Omit for Human authors.
    #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
    #[serde(default)]
    pub endpoint_id: Option<String>,
    /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
    #[schemars(range(min = 1))]
    #[serde(default)]
    pub expected_controller_generation: Option<i64>,
    /// Optional explicit Agent Inbox recipients. Omit to deliver to every Agent participant except the
    /// author; an explicit empty array posts to the transcript/room without Agent deliveries.
    #[schemars(length(min = 0, max = 16))]
    #[schemars(inner(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$")))]
    #[serde(default)]
    pub recipient_agent_ids: Option<Vec<String>>,
    /// Optional parent Message in the same Conversation.
    #[schemars(regex(pattern = "^wc_cmsg_[A-Za-z0-9_-]{16}$"))]
    #[serde(default)]
    pub reply_to: Option<String>,
    /// Caller-generated operation key. Exact replay under the same communication principal returns the
    /// original durable resource; reuse with changed input is rejected.
    #[schemars(length(min = 1, max = 128))]
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Exact durable Wake that already crossed a Host dispatch or explicit-activation fence and
    /// provides stable resumed-turn reply replay identity. Use with reply_operation_index instead of
    /// idempotency_key.
    #[schemars(regex(pattern = "^wc_wake_[A-Za-z0-9_-]{16}$"))]
    #[serde(default)]
    pub wake_reply_id: Option<String>,
    /// Stable per-send index within one Wake. Reuse the same index only for an exact uncertain retry;
    /// use a different index for each intentional additional Message.
    #[schemars(range(min = 0, max = 31))]
    #[serde(default)]
    pub reply_operation_index: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListAgentInboxInput {
    /// Canonical recipient Agent id.
    #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
    pub agent_id: String,
    /// Active Endpoint proving access to this Agent Inbox.
    #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
    pub endpoint_id: String,
    /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
    #[schemars(range(min = 1))]
    pub expected_controller_generation: i64,
    #[schemars(extend("default" = 0))]
    /// Return queued deliveries with durable delivery_order greater than this cursor.
    #[schemars(range(min = 0))]
    #[serde(default)]
    pub after_delivery_order: Option<i64>,
    #[schemars(extend("default" = 50))]
    /// Maximum records to return. Values above 100 are accepted and clamped to 100.
    #[schemars(range(min = 1))]
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsumeAgentDeliveriesInput {
    /// Canonical recipient Agent id.
    #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
    pub agent_id: String,
    /// Active Endpoint proving access to this Agent Inbox.
    #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
    pub endpoint_id: String,
    /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
    #[schemars(range(min = 1))]
    pub expected_controller_generation: i64,
    /// Deliveries to mark consumed. Repeating already-consumed ids is a safe desired-state retry.
    #[schemars(length(min = 1, max = 100))]
    #[schemars(inner(regex(pattern = "^wc_delivery_[A-Za-z0-9_-]{16}$")))]
    pub delivery_ids: Vec<String>,
}

use super::common::{array_schema, schema_type, wrapped_output_schema};
use serde_json::{json, Value};

fn nullable_integer(description: &str) -> Value {
    json!({
        "anyOf": [{"type": "integer"}, {"type": "null"}],
        "description": description
    })
}

fn nullable_string(description: &str) -> Value {
    json!({
        "anyOf": [{"type": "string"}, {"type": "null"}],
        "description": description
    })
}

fn agent_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "agent_id": schema_type("string", "Canonical durable Agent identity."),
            "handle": schema_type("string", "Mutable non-unique Agent handle."),
            "display_name": schema_type("string", "Mutable non-unique display name."),
            "description": schema_type("string", "Agent Card description."),
            "specialty_labels": array_schema(schema_type("string", "Self-description label."), "Bounded self-description labels; never authority."),
            "profile_revision": schema_type("integer", "Monotonic Agent Card revision."),
            "created_at_unix_ms": schema_type("integer", "Creation time in Unix milliseconds."),
            "updated_at_unix_ms": schema_type("integer", "Latest profile update time in Unix milliseconds."),
            "active_endpoint_count": schema_type("integer", "Current attached Endpoint count."),
            "queued_delivery_count": schema_type("integer", "Queued Inbox deliveries for this Agent.")
        },
        "required": [
            "agent_id", "handle", "display_name", "description", "specialty_labels",
            "profile_revision", "created_at_unix_ms", "updated_at_unix_ms",
            "active_endpoint_count", "queued_delivery_count"
        ]
    })
}

fn endpoint_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "endpoint_id": schema_type("string", "Canonical current attachment id."),
            "agent_id": schema_type("string", "Durable Agent carried by this Endpoint."),
            "host": schema_type("string", "Host adapter name."),
            "client_attachment_id": nullable_string("Optional host-local attachment id."),
            "wake_capable": schema_type("boolean", "Declared adapter capability only; it does not invoke model wake."),
            "controller_generation": nullable_string("Optional continuation-controller generation metadata."),
            "attached_at_unix_ms": schema_type("integer", "Attachment time in Unix milliseconds."),
            "last_seen_at_unix_ms": schema_type("integer", "Latest communication activity time in Unix milliseconds."),
            "detached_at_unix_ms": nullable_integer("Detach time, or null while active.")
        },
        "required": [
            "endpoint_id", "agent_id", "host", "client_attachment_id", "wake_capable",
            "controller_generation", "attached_at_unix_ms", "last_seen_at_unix_ms",
            "detached_at_unix_ms"
        ]
    })
}

fn participant_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "participant_id": schema_type("string", "Canonical Conversation participant record id."),
            "participant_kind": {"type": "string", "enum": ["human", "agent"]},
            "agent_id": nullable_string("Canonical Agent id for Agent participants."),
            "handle": nullable_string("Current Agent handle projection."),
            "display_name": nullable_string("Current Agent display-name projection."),
            "principal_kind": nullable_string("Bounded credential class for Human provenance; no secret identity is returned."),
            "joined_at_unix_ms": schema_type("integer", "Join time in Unix milliseconds.")
        },
        "required": [
            "participant_id", "participant_kind", "agent_id", "handle",
            "display_name", "principal_kind", "joined_at_unix_ms"
        ]
    })
}

fn author_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "participant_kind": {"type": "string", "enum": ["human", "agent"]},
            "agent_id": nullable_string("Canonical Agent author id when participant_kind is agent."),
            "handle": nullable_string("Current Agent handle projection."),
            "display_name": nullable_string("Current Agent display-name projection."),
            "principal_kind": nullable_string("Bounded Human credential class; no credential digest is returned.")
        },
        "required": ["participant_kind", "agent_id", "handle", "display_name", "principal_kind"]
    })
}

fn delivery_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "delivery_order": schema_type("integer", "Durable monotonic Inbox ordering cursor."),
            "delivery_id": schema_type("string", "Canonical recipient-specific Delivery id."),
            "recipient_agent_id": schema_type("string", "Recipient Agent identity."),
            "state": {"type": "string", "enum": ["queued", "consumed"]},
            "created_at_unix_ms": schema_type("integer", "Delivery creation time in Unix milliseconds."),
            "consumed_at_unix_ms": nullable_integer("Consumption time, or null while queued.")
        },
        "required": [
            "delivery_order", "delivery_id", "recipient_agent_id", "state",
            "created_at_unix_ms", "consumed_at_unix_ms"
        ]
    })
}

fn message_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "message_id": schema_type("string", "Canonical append-only Conversation Message id."),
            "conversation_id": schema_type("string", "Owning Conversation id."),
            "seq": schema_type("integer", "Stable monotonic sequence within the Conversation."),
            "author": author_schema(),
            "body": schema_type("string", "Message body."),
            "reply_to": nullable_string("Optional parent Message in the same Conversation."),
            "created_at_unix_ms": schema_type("integer", "Creation time in Unix milliseconds."),
            "deliveries": array_schema(delivery_schema(), "Recipient-specific delivery state, distinct from the Message transcript record.")
        },
        "required": [
            "message_id", "conversation_id", "seq", "author", "body",
            "reply_to", "created_at_unix_ms", "deliveries"
        ]
    })
}

fn conversation_summary_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "conversation_id": schema_type("string", "Canonical durable Conversation id."),
            "title": nullable_string("Optional Conversation title."),
            "lifecycle": {"type": "string", "enum": ["open", "closed"]},
            "created_at_unix_ms": schema_type("integer", "Creation time in Unix milliseconds."),
            "updated_at_unix_ms": schema_type("integer", "Latest transcript update time in Unix milliseconds."),
            "participant_count": schema_type("integer", "Current participant count."),
            "message_count": schema_type("integer", "Append-only transcript message count."),
            "last_seq": schema_type("integer", "Latest stable transcript sequence."),
            "queued_delivery_count": nullable_integer("Queued deliveries for an Agent view; null for a Human view.")
        },
        "required": [
            "conversation_id", "title", "lifecycle", "created_at_unix_ms",
            "updated_at_unix_ms", "participant_count", "message_count", "last_seq",
            "queued_delivery_count"
        ]
    })
}

fn conversation_detail_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "conversation": conversation_summary_schema(),
            "participants": array_schema(participant_schema(), "Human and Agent participant provenance."),
            "messages": array_schema(message_schema(), "Ordered append-only transcript page."),
            "after_seq": schema_type("integer", "Requested exclusive transcript cursor."),
            "next_after_seq": nullable_integer("Continuation cursor when truncated."),
            "truncated": schema_type("boolean", "True when more transcript messages remain.")
        },
        "required": [
            "conversation", "participants", "messages", "after_seq",
            "next_after_seq", "truncated"
        ]
    })
}

pub(crate) fn output_schema_for_tool(name: &str) -> Option<Value> {
    let schema = match name {
        "create_agent_identity" | "update_agent_identity" => wrapped_output_schema(vec![
            ("agent", agent_schema()),
            (
                "created",
                schema_type(
                    "boolean",
                    "True only for first creation; false for profile updates.",
                ),
            ),
            (
                "replayed",
                schema_type("boolean", "True for exact idempotent creation replay."),
            ),
            (
                "state_changed",
                schema_type("boolean", "Whether durable state changed."),
            ),
        ]),
        "list_agent_identities" => wrapped_output_schema(vec![
            (
                "total_count",
                schema_type(
                    "integer",
                    "Total Agents visible to the current communication principal.",
                ),
            ),
            ("offset", schema_type("integer", "Returned page offset.")),
            (
                "next_offset",
                nullable_integer("Next page offset when truncated."),
            ),
            (
                "truncated",
                schema_type("boolean", "True when more Agents remain."),
            ),
            (
                "agents",
                array_schema(agent_schema(), "Bounded Agent Card page."),
            ),
        ]),
        "attach_agent_endpoint" | "detach_agent_endpoint" => wrapped_output_schema(vec![
            ("endpoint", endpoint_schema()),
            (
                "created",
                schema_type(
                    "boolean",
                    "True only for first attachment; false for detach.",
                ),
            ),
            (
                "replayed",
                schema_type("boolean", "True for exact idempotent attach replay."),
            ),
            (
                "state_changed",
                schema_type("boolean", "Whether attachment state changed."),
            ),
        ]),
        "create_conversation" => wrapped_output_schema(vec![
            ("conversation", conversation_detail_schema()),
            (
                "created",
                schema_type("boolean", "True only for first committed creation."),
            ),
            (
                "replayed",
                schema_type("boolean", "True for exact idempotent replay."),
            ),
            (
                "state_changed",
                schema_type("boolean", "Whether durable state changed."),
            ),
        ]),
        "list_conversations" => wrapped_output_schema(vec![
            (
                "total_count",
                schema_type(
                    "integer",
                    "Total Conversations visible to this Human or Agent view.",
                ),
            ),
            ("offset", schema_type("integer", "Returned page offset.")),
            (
                "next_offset",
                nullable_integer("Next page offset when truncated."),
            ),
            (
                "truncated",
                schema_type("boolean", "True when more Conversations remain."),
            ),
            (
                "conversations",
                array_schema(conversation_summary_schema(), "Bounded Conversation page."),
            ),
        ]),
        "read_conversation" => wrapped_output_schema(vec![
            ("conversation", conversation_summary_schema()),
            (
                "participants",
                array_schema(participant_schema(), "Conversation participants."),
            ),
            (
                "messages",
                array_schema(message_schema(), "Ordered transcript page."),
            ),
            (
                "after_seq",
                schema_type("integer", "Requested exclusive sequence cursor."),
            ),
            (
                "next_after_seq",
                nullable_integer("Continuation cursor when truncated."),
            ),
            (
                "truncated",
                schema_type("boolean", "True when more messages remain."),
            ),
        ]),
        "post_conversation_message" => wrapped_output_schema(vec![
            ("message", message_schema()),
            (
                "replayed",
                schema_type("boolean", "True for exact idempotent replay."),
            ),
            (
                "state_changed",
                schema_type("boolean", "True only for first append."),
            ),
        ]),
        "list_agent_inbox" => wrapped_output_schema(vec![
            ("agent_id", schema_type("string", "Recipient Agent id.")),
            (
                "total_queued_count",
                schema_type("integer", "Total queued deliveries for this Agent."),
            ),
            (
                "after_delivery_order",
                schema_type("integer", "Requested exclusive delivery cursor."),
            ),
            (
                "next_after_delivery_order",
                nullable_integer("Continuation cursor when truncated."),
            ),
            (
                "truncated",
                schema_type("boolean", "True when more queued deliveries remain."),
            ),
            (
                "deliveries",
                array_schema(
                    json!({
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "delivery_id": schema_type("string", "Canonical Delivery id."),
                            "state": {"type": "string", "enum": ["queued"]},
                            "conversation_id": schema_type("string", "Owning Conversation id."),
                            "conversation_title": nullable_string("Optional Conversation title."),
                            "message": message_schema()
                        },
                        "required": ["delivery_id", "state", "conversation_id", "conversation_title", "message"]
                    }),
                    "Bounded queued Inbox page.",
                ),
            ),
        ]),
        "consume_agent_deliveries" => wrapped_output_schema(vec![
            ("agent_id", schema_type("string", "Recipient Agent id.")),
            (
                "consumed_delivery_ids",
                array_schema(
                    schema_type("string", "Delivery changed from queued to consumed."),
                    "Deliveries changed by this call.",
                ),
            ),
            (
                "already_consumed_delivery_ids",
                array_schema(
                    schema_type("string", "Delivery already in the desired consumed state."),
                    "Desired-state replay results.",
                ),
            ),
            (
                "state_changed",
                schema_type("boolean", "True when at least one delivery changed."),
            ),
        ]),
        _ => return None,
    };
    Some(schema)
}

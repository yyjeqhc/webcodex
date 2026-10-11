//! Canonical MCP invocation metadata. Parse `_wc` directly into runtime types;
//! the business arguments and runtime authorization remain separate.
use super::{
    is_host_continuation_app_tool_name, stateless_window_reply_supported, ToolInvocationMetadata,
    ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME, WORK_RESULT_THREAD_ENTRYPOINT_TOOL_NAME,
};
use serde_json::Value;

pub(super) const MCP_INVOCATION_ENVELOPE_FIELD: &str = "_wc";

#[derive(Debug, Default)]
pub(in crate::mcp) struct McpInvocationEnvelope {
    pub(in crate::mcp) recording_session_selector: Option<String>,
    pub(in crate::mcp) metadata: ToolInvocationMetadata,
}

pub(super) fn mcp_invocation_envelope_supported_fields(tool: &str) -> Vec<&'static str> {
    if matches!(
        tool,
        "sync_goal_plan"
            | "get_work_result_state"
            | "read_app_artifact_chunk"
            | "read_changed_file_diff"
            | "search_mentions"
    ) || tool == WORK_RESULT_THREAD_ENTRYPOINT_TOOL_NAME
        || is_host_continuation_app_tool_name(tool)
    {
        return Vec::new();
    }
    if matches!(
        tool,
        "open_webcodex_workbench" | "search_webcodex_resources" | "read_webcodex_resource"
    ) {
        return vec!["ack", "ack_ref", "context"];
    }
    if matches!(
        tool,
        crate::mcp_gateway::MCP_TOOL_NAME
            | crate::plugin_gateway::PLUGIN_TOOL_NAME
            | crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME
    ) {
        return vec!["record", "ack", "ack_ref"];
    }
    let mut fields = vec!["record", "ack", "ack_ref", "resolve", "context"];
    if tool == ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
        || crate::tool_runtime::execution_control::supported(tool)
    {
        fields.push("compact_execution");
    }
    if stateless_window_reply_supported(Some(tool)) {
        fields.push("reply");
    }
    if tool == ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
        || crate::tool_runtime::control_sidecar::supports_control_sidecars(tool)
    {
        fields.push("control");
    }
    fields
}

pub(super) fn validate_mcp_invocation_envelope_for_target(
    envelope: &McpInvocationEnvelope,
    tool: &str,
) -> Result<(), String> {
    let allowed = mcp_invocation_envelope_supported_fields(tool);
    let metadata = &envelope.metadata;
    for (field, present) in [
        ("compact_execution", metadata.compact_execution),
        ("record", envelope.recording_session_selector.is_some()),
        ("ack", !metadata.ack_session_message_ids.is_empty()),
        ("ack_ref", metadata.ack_ref.is_some()),
        ("reply", metadata.window_reply.is_some()),
        ("resolve", metadata.session_message_resolution.is_some()),
        ("context", !metadata.context_request.is_empty()),
        ("control", metadata.control.is_some()),
    ] {
        if present && !allowed.contains(&field) {
            return Err(format!("unsupported _wc field '{field}' for tool '{tool}'"));
        }
    }
    Ok(())
}

pub(in crate::mcp) fn parse_mcp_invocation_envelope(
    tool_name: &str,
    arguments: &mut Value,
    stateless_2026: bool,
) -> Result<McpInvocationEnvelope, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Ok(McpInvocationEnvelope::default());
    };
    if !stateless_2026 {
        if object.contains_key(MCP_INVOCATION_ENVELOPE_FIELD) {
            return Err("field '_wc' is unavailable on this MCP protocol surface".to_string());
        }
        return Ok(McpInvocationEnvelope::default());
    }

    for legacy in [
        "_session_id",
        crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD,
        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD,
        crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD,
        crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD,
        crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD,
        crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD,
        crate::tool_runtime::control_sidecar::CONTROL_FIELD,
    ] {
        if object.contains_key(legacy) {
            return Err(format!(
                "legacy MCP invocation field '{legacy}' is no longer supported; use '_wc'"
            ));
        }
    }

    let Some(value) = object.remove(MCP_INVOCATION_ENVELOPE_FIELD) else {
        return Ok(McpInvocationEnvelope::default());
    };
    let Value::Object(mut envelope) = value else {
        return Err("field '_wc' must be a closed invocation metadata object".to_string());
    };
    let allowed = mcp_invocation_envelope_supported_fields(tool_name);
    if let Some(key) = envelope.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!(
            "unsupported _wc field '{key}' for tool '{tool_name}'"
        ));
    }

    // Preserve validation precedence when a caller supplies several invalid fields.
    let control = match envelope.remove("control") {
        Some(value) => crate::tool_runtime::control_sidecar::parse_control_sidecars(
            value,
            tool_name == ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
                || crate::tool_runtime::control_sidecar::supports_control_sidecars(tool_name),
        )
        .map_err(|error| format!("_wc.control {}", error.description()))?,
        None => None,
    };
    let compact_execution = match envelope.remove("compact_execution") {
        None => false,
        Some(Value::Bool(value)) => value,
        Some(_) => return Err("_wc.compact_execution must be a boolean".to_string()),
    };
    let parsed = McpInvocationEnvelope {
        recording_session_selector: parse_record(envelope.remove("record"))?,
        metadata: ToolInvocationMetadata {
            compact_execution,
            control,
            ack_session_message_ids: parse_ack(envelope.remove("ack"))?,
            ack_ref: parse_ack_ref(envelope.remove("ack_ref"))?,
            session_message_resolution: parse_resolution(envelope.remove("resolve"))?,
            window_reply: parse_reply(envelope.remove("reply"))?,
            context_request: parse_context(envelope.remove("context"))?,
        },
    };
    debug_assert!(envelope.is_empty());
    Ok(parsed)
}

fn parse_record(value: Option<Value>) -> Result<Option<String>, String> {
    match value {
        None => Ok(None),
        Some(Value::String(value)) => {
            let value = value.trim();
            if value.is_empty() {
                return Err("field '_wc.record' must be a non-empty string".to_string());
            }
            Ok(Some(value.to_string()))
        }
        Some(_) => Err("field '_wc.record' must be a non-empty string".to_string()),
    }
}

fn parse_ack(value: Option<Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Value::Array(values) = value else {
        return Err("field '_wc.ack' must be an array of wc_msg_* ids".to_string());
    };
    if values.len() > crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_MESSAGE_IDS {
        return Err(format!(
            "field '_wc.ack' accepts at most {} message ids",
            crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_MESSAGE_IDS
        ));
    }
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = std::collections::HashSet::new();
    for value in values {
        let Value::String(value) = value else {
            return Err("field '_wc.ack' must contain only wc_msg_* strings".to_string());
        };
        let value = value.trim();
        if !webcodex_core::workflow_session_contract::is_valid_session_message_id(value) {
            return Err("field '_wc.ack' must contain only valid wc_msg_* ids".to_string());
        }
        if seen.insert(value.to_string()) {
            normalized.push(value.to_string());
        }
    }
    Ok(normalized)
}

fn parse_ack_ref(value: Option<Value>) -> Result<Option<String>, String> {
    match value {
        None => Ok(None),
        Some(Value::String(value)) => {
            let value = value.trim();
            if value.is_empty()
                || value.len() > crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_REF_CHARS
            {
                return Err("field '_wc.ack_ref' must be a non-empty bounded string".to_string());
            }
            Ok(Some(value.to_string()))
        }
        Some(_) => Err("field '_wc.ack_ref' must be a non-empty bounded string".to_string()),
    }
}

fn parse_resolution(
    value: Option<Value>,
) -> Result<Option<crate::tool_runtime::sessions::ToolCallSessionMessageResolution>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Value::Object(mut fields) = value else {
        return Err(
            "field '_wc.resolve' must be an object with message_id and resolution".to_string(),
        );
    };
    if fields.len() != 2 || !fields.contains_key("message_id") || !fields.contains_key("resolution")
    {
        return Err("field '_wc.resolve' accepts exactly message_id and resolution".to_string());
    }
    let Some(Value::String(message_id)) = fields.remove("message_id") else {
        return Err("_wc.resolve.message_id must be a wc_msg_* string".to_string());
    };
    let message_id = message_id.trim().to_string();
    if !webcodex_core::workflow_session_contract::is_valid_session_message_id(&message_id) {
        return Err("_wc.resolve.message_id must be a valid wc_msg_* id".to_string());
    }
    let Some(Value::String(resolution)) = fields.remove("resolution") else {
        return Err("_wc.resolve.resolution must be a string".to_string());
    };
    let resolution = resolution.trim().to_string();
    if resolution.is_empty() {
        return Err("_wc.resolve.resolution must not be empty".to_string());
    }
    if resolution.chars().count() > crate::tool_runtime::sessions::MAX_MESSAGE_RESOLUTION_CHARS {
        return Err(format!(
            "_wc.resolve.resolution exceeds {} chars",
            crate::tool_runtime::sessions::MAX_MESSAGE_RESOLUTION_CHARS
        ));
    }
    Ok(Some(
        crate::tool_runtime::sessions::ToolCallSessionMessageResolution {
            message_id,
            resolution,
        },
    ))
}

fn parse_reply(
    value: Option<Value>,
) -> Result<Option<crate::tool_runtime::window_collaboration::ToolCallWindowReply>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Value::Object(mut fields) = value else {
        return Err("field '_wc.reply' must be an object with reply_to and message".to_string());
    };
    if fields.len() != 2 || !fields.contains_key("reply_to") || !fields.contains_key("message") {
        return Err("field '_wc.reply' accepts exactly reply_to and message".to_string());
    }
    let Some(Value::String(reply_to)) = fields.remove("reply_to") else {
        return Err("_wc.reply.reply_to must be a wc_msg_* string".to_string());
    };
    let reply_to = reply_to.trim().to_string();
    if !webcodex_core::workflow_session_contract::is_valid_session_message_id(&reply_to) {
        return Err("_wc.reply.reply_to must be a valid wc_msg_* id".to_string());
    }
    let Some(Value::String(message)) = fields.remove("message") else {
        return Err("_wc.reply.message must be a string".to_string());
    };
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("_wc.reply.message must not be empty".to_string());
    }
    if message.chars().count() > crate::tool_runtime::window_collaboration::MAX_WINDOW_REPLY_CHARS {
        return Err(format!(
            "_wc.reply.message exceeds {} chars",
            crate::tool_runtime::window_collaboration::MAX_WINDOW_REPLY_CHARS
        ));
    }
    Ok(Some(
        crate::tool_runtime::window_collaboration::ToolCallWindowReply {
            reply_to_message_id: reply_to,
            message,
        },
    ))
}

fn parse_context(value: Option<Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Value::Array(values) = value else {
        return Err(
            "field '_wc.context' must be an array of bounded context material keys".to_string(),
        );
    };
    if values.len() > crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_ITEMS {
        return Err(format!(
            "field '_wc.context' accepts at most {} context material keys",
            crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_ITEMS
        ));
    }
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = std::collections::HashSet::new();
    for value in values {
        let Value::String(value) = value else {
            return Err("field '_wc.context' must contain only strings".to_string());
        };
        let key = value.trim();
        let valid = !key.is_empty()
            && key.chars().count()
                <= crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_KEY_CHARS
            && key
                .chars()
                .all(|ch| !ch.is_control() && !ch.is_whitespace());
        if !valid {
            return Err(format!(
                "field '_wc.context' keys must be non-empty, at most {} characters, and contain no whitespace or control characters",
                crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_KEY_CHARS
            ));
        }
        if seen.insert(key.to_string()) {
            normalized.push(key.to_string());
        }
    }
    Ok(normalized)
}

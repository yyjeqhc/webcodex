//! Bounded, transport-neutral protocol for the Runner-owned stdio MCP gateway.
//!
//! This is intentionally not a raw JSON-RPC tunnel. Provider inventory rides
//! normal Runner registration; request traffic contains only passive provider
//! lifecycle status, `tools/list`, and `tools/call`.

use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const MCP_GATEWAY_MAX_PROVIDERS: usize = 8;
pub const MCP_GATEWAY_MAX_ENV_MAPPINGS: usize = 64;
pub const MCP_GATEWAY_MAX_PROVIDER_ID_BYTES: usize = 64;
pub const MCP_GATEWAY_MAX_PROVIDER_NAME_BYTES: usize = 128;
pub const MCP_GATEWAY_MAX_TOOL_COUNT: usize = 128;
pub const MCP_GATEWAY_MAX_TOOL_NAME_BYTES: usize = 128;
pub const MCP_GATEWAY_MAX_DESCRIPTION_BYTES: usize = 4 * 1024;
pub const MCP_GATEWAY_MAX_SCHEMA_BYTES: usize = 64 * 1024;
pub const MCP_GATEWAY_MAX_ARGUMENT_BYTES: usize = 64 * 1024;
pub const MCP_GATEWAY_MAX_STRUCTURED_CONTENT_BYTES: usize = 512 * 1024;
pub const MCP_GATEWAY_MAX_TEXT_CONTENT_BYTES: usize = 512 * 1024;
/// Existing non-image aggregate result budget. Image base64 is accounted separately.
pub const MCP_GATEWAY_MAX_RESULT_BYTES: usize = 512 * 1024;
pub const MCP_GATEWAY_MAX_IMAGE_BYTES: usize = crate::artifact_policy::MAX_MCP_IMAGE_BYTES;
pub const MCP_GATEWAY_MAX_IMAGE_BASE64_BYTES: usize = ((MCP_GATEWAY_MAX_IMAGE_BYTES + 2) / 3) * 4;
pub const MCP_GATEWAY_MAX_IMAGE_MIME_BYTES: usize = 64;
/// Maximum serialized typed tool result: existing result budget plus one aggregate image budget.
pub const MCP_GATEWAY_MAX_RESULT_WIRE_BYTES: usize =
    MCP_GATEWAY_MAX_RESULT_BYTES + MCP_GATEWAY_MAX_IMAGE_BASE64_BYTES;
/// Small fixed allowance for the typed Runner<->Server response envelope around a tool result.
pub const MCP_GATEWAY_MAX_TOOL_RESULT_MESSAGE_BYTES: usize =
    MCP_GATEWAY_MAX_RESULT_WIRE_BYTES + 4 * 1024;
/// Provider stdout must be bounded before JSON parsing; only a valid tools/call image result may survive the larger inbound allowance.
pub const MCP_GATEWAY_MAX_PROVIDER_MESSAGE_BYTES: usize =
    MCP_GATEWAY_MAX_TOOL_RESULT_MESSAGE_BYTES + 4 * 1024;
pub const MCP_GATEWAY_MAX_CONTENT_ITEMS: usize = 32;
pub const MCP_GATEWAY_MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MCP_GATEWAY_MAX_JSON_DEPTH: usize = 16;
pub const MCP_GATEWAY_MAX_JSON_NODES: usize = 4_096;
pub const MCP_GATEWAY_MAX_JSON_STRING_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum McpGatewayRequest {
    ProviderStatus {
        provider_id: String,
        provider_instance_id: String,
    },
    ToolsList {
        provider_id: String,
        provider_instance_id: String,
    },
    ToolsCall {
        provider_id: String,
        provider_instance_id: String,
        name: String,
        arguments: Value,
        expected_schema: McpGatewaySchemaObservation,
    },
}

impl McpGatewayRequest {
    pub fn provider_id(&self) -> &str {
        match self {
            Self::ProviderStatus { provider_id, .. }
            | Self::ToolsList { provider_id, .. }
            | Self::ToolsCall { provider_id, .. } => provider_id,
        }
    }

    pub fn provider_instance_id(&self) -> &str {
        match self {
            Self::ProviderStatus {
                provider_instance_id,
                ..
            }
            | Self::ToolsList {
                provider_instance_id,
                ..
            }
            | Self::ToolsCall {
                provider_instance_id,
                ..
            } => provider_instance_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpGatewayDispatchState {
    NotStarted,
    OutcomeUnknown,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpGatewayProviderState {
    NeverStarted,
    Healthy,
    ConnectionRetired,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpGatewayProvider {
    pub provider_id: String,
    pub provider_instance_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpGatewayTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(
        default,
        rename = "outputSchema",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpGatewaySchemaObservation {
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(
        default,
        rename = "outputSchema",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

impl McpGatewayTool {
    pub fn schema_observation(&self) -> McpGatewaySchemaObservation {
        McpGatewaySchemaObservation {
            input_schema: self.input_schema.clone(),
            output_schema: self.output_schema.clone(),
            annotations: self.annotations.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum McpGatewayContent {
    Text {
        text: String,
    },
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpGatewayToolResult {
    pub content: Vec<McpGatewayContent>,
    #[serde(
        default,
        rename = "structuredContent",
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<Value>,
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum McpGatewayResponsePayload {
    ProviderStatus { state: McpGatewayProviderState },
    Tools { tools: Vec<McpGatewayTool> },
    ToolResult { result: McpGatewayToolResult },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpGatewayError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpGatewayResponse {
    pub dispatch_state: McpGatewayDispatchState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<McpGatewayResponsePayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<McpGatewayError>,
}

impl McpGatewayResponse {
    pub fn success(payload: McpGatewayResponsePayload) -> Self {
        Self {
            dispatch_state: McpGatewayDispatchState::Completed,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn error(
        dispatch_state: McpGatewayDispatchState,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            dispatch_state,
            payload: None,
            error: Some(McpGatewayError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

pub fn validate_provider_id(value: &str) -> Result<(), String> {
    validate_identifier(
        value,
        "provider_id",
        MCP_GATEWAY_MAX_PROVIDER_ID_BYTES,
        true,
    )
}

pub fn validate_provider_instance_id(value: &str) -> Result<(), String> {
    validate_identifier(
        value,
        "provider_instance_id",
        MCP_GATEWAY_MAX_PROVIDER_ID_BYTES,
        false,
    )
}

pub fn validate_tool_name(value: &str) -> Result<(), String> {
    validate_identifier(value, "tool name", MCP_GATEWAY_MAX_TOOL_NAME_BYTES, false)
}

fn validate_identifier(
    value: &str,
    field: &str,
    max_bytes: usize,
    lowercase_only: bool,
) -> Result<(), String> {
    if value.is_empty() || value.len() > max_bytes {
        return Err(format!("{field} must contain 1..={max_bytes} bytes"));
    }
    let valid = value.bytes().all(|byte| {
        (if lowercase_only {
            byte.is_ascii_lowercase()
        } else {
            byte.is_ascii_alphanumeric()
        }) || byte.is_ascii_digit()
            || matches!(byte, b'_' | b'-' | b'.')
    });
    if !valid {
        let letters = if lowercase_only {
            "lowercase ASCII letters"
        } else {
            "ASCII letters"
        };
        return Err(format!(
            "{field} may contain only {letters}, digits, '_', '-', and '.'"
        ));
    }
    Ok(())
}

pub fn validate_provider_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > MCP_GATEWAY_MAX_PROVIDER_NAME_BYTES {
        return Err(format!(
            "provider name must contain 1..={} bytes",
            MCP_GATEWAY_MAX_PROVIDER_NAME_BYTES
        ));
    }
    if value.chars().any(char::is_control) {
        return Err("provider name contains unsupported control characters".to_string());
    }
    Ok(())
}

pub fn validate_request(request: &McpGatewayRequest) -> Result<(), String> {
    match request {
        McpGatewayRequest::ProviderStatus {
            provider_id,
            provider_instance_id,
        }
        | McpGatewayRequest::ToolsList {
            provider_id,
            provider_instance_id,
        } => {
            validate_provider_id(provider_id)?;
            validate_provider_instance_id(provider_instance_id)
        }
        McpGatewayRequest::ToolsCall {
            provider_id,
            provider_instance_id,
            name,
            arguments,
            expected_schema,
        } => {
            validate_provider_id(provider_id)?;
            validate_provider_instance_id(provider_instance_id)?;
            validate_tool_name(name)?;
            if !arguments.is_object() {
                return Err("tool arguments must be a JSON object".to_string());
            }
            validate_json_value(arguments, MCP_GATEWAY_MAX_ARGUMENT_BYTES, "tool arguments")?;
            validate_schema_observation(expected_schema)?;
            Ok(())
        }
    }
}

pub fn validate_response(response: &McpGatewayResponse) -> Result<(), String> {
    let encoded = serde_json::to_vec(response)
        .map_err(|_| "bridge response could not be serialized".to_string())?;
    let max_message_bytes = if matches!(
        response.payload.as_ref(),
        Some(McpGatewayResponsePayload::ToolResult { .. })
    ) {
        MCP_GATEWAY_MAX_TOOL_RESULT_MESSAGE_BYTES
    } else {
        MCP_GATEWAY_MAX_MESSAGE_BYTES
    };
    if encoded.len() > max_message_bytes {
        return Err(format!(
            "bridge response exceeds maximum {max_message_bytes} bytes"
        ));
    }
    match (&response.payload, &response.error) {
        (Some(_), Some(_)) | (None, None) => {
            return Err("bridge response must contain exactly one payload or error".to_string())
        }
        (Some(_), None) if response.dispatch_state != McpGatewayDispatchState::Completed => {
            return Err("bridge response payload requires completed dispatch state".to_string())
        }
        (None, Some(error)) => validate_error(error)?,
        _ => {}
    }
    match response.payload.as_ref() {
        Some(McpGatewayResponsePayload::ProviderStatus { .. }) => Ok(()),
        Some(McpGatewayResponsePayload::Tools { tools }) => validate_tools(tools),
        Some(McpGatewayResponsePayload::ToolResult { result }) => validate_tool_result(result),
        None => Ok(()),
    }
}

fn validate_error(error: &McpGatewayError) -> Result<(), String> {
    validate_identifier(&error.code, "bridge error code", 80, true)?;
    if error.message.is_empty() || error.message.len() > 512 {
        return Err("bridge error message must contain 1..=512 bytes".to_string());
    }
    validate_text_controls(&error.message, "bridge error message")
}

pub fn validate_providers(providers: &[McpGatewayProvider]) -> Result<(), String> {
    if providers.len() > MCP_GATEWAY_MAX_PROVIDERS {
        return Err(format!(
            "provider count exceeds maximum {}",
            MCP_GATEWAY_MAX_PROVIDERS
        ));
    }
    let mut provider_ids = HashSet::new();
    let mut instance_ids = HashSet::new();
    for provider in providers {
        validate_provider_id(&provider.provider_id)?;
        validate_provider_instance_id(&provider.provider_instance_id)?;
        validate_provider_name(&provider.name)?;
        if !provider_ids.insert(provider.provider_id.as_str()) {
            return Err("duplicate bridge provider id".to_string());
        }
        if !instance_ids.insert(provider.provider_instance_id.as_str()) {
            return Err("duplicate bridge provider instance identity".to_string());
        }
    }
    Ok(())
}

pub fn validate_tools(tools: &[McpGatewayTool]) -> Result<(), String> {
    if tools.len() > MCP_GATEWAY_MAX_TOOL_COUNT {
        return Err(format!(
            "tool count exceeds maximum {}",
            MCP_GATEWAY_MAX_TOOL_COUNT
        ));
    }
    let mut names = HashSet::new();
    for tool in tools {
        validate_tool_name(&tool.name)?;
        if !names.insert(tool.name.as_str()) {
            return Err(format!("duplicate tool name '{}'", tool.name));
        }
        if let Some(title) = tool.title.as_deref() {
            if title.is_empty() || title.len() > MCP_GATEWAY_MAX_PROVIDER_NAME_BYTES {
                return Err("tool title is empty or too long".to_string());
            }
            validate_text_controls(title, "tool title")?;
        }
        if let Some(description) = tool.description.as_deref() {
            if description.len() > MCP_GATEWAY_MAX_DESCRIPTION_BYTES {
                return Err(format!(
                    "tool description exceeds maximum {} bytes",
                    MCP_GATEWAY_MAX_DESCRIPTION_BYTES
                ));
            }
            validate_text_controls(description, "tool description")?;
        }
        validate_schema_observation(&tool.schema_observation())?;
        if let Some(meta) = tool.meta.as_ref() {
            if !meta.is_object() {
                return Err("tool _meta must be a JSON object".to_string());
            }
            validate_json_value(meta, MCP_GATEWAY_MAX_SCHEMA_BYTES, "tool _meta")?;
        }
    }
    Ok(())
}

pub fn validate_schema_observation(
    observation: &McpGatewaySchemaObservation,
) -> Result<(), String> {
    if !observation.input_schema.is_object() {
        return Err("tool inputSchema must be a JSON object".to_string());
    }
    validate_json_value(
        &observation.input_schema,
        MCP_GATEWAY_MAX_SCHEMA_BYTES,
        "tool inputSchema",
    )?;
    if let Some(output_schema) = observation.output_schema.as_ref() {
        if !output_schema.is_object() {
            return Err("tool outputSchema must be a JSON object".to_string());
        }
        validate_json_value(
            output_schema,
            MCP_GATEWAY_MAX_SCHEMA_BYTES,
            "tool outputSchema",
        )?;
    }
    if let Some(annotations) = observation.annotations.as_ref() {
        if !annotations.is_object() {
            return Err("tool annotations must be a JSON object".to_string());
        }
        validate_json_value(
            annotations,
            MCP_GATEWAY_MAX_SCHEMA_BYTES,
            "tool annotations",
        )?;
    }
    Ok(())
}

pub fn validate_tool_result(result: &McpGatewayToolResult) -> Result<(), String> {
    if result.content.len() > MCP_GATEWAY_MAX_CONTENT_ITEMS {
        return Err(format!(
            "tool result content exceeds maximum {} items",
            MCP_GATEWAY_MAX_CONTENT_ITEMS
        ));
    }
    let mut text_bytes = 0usize;
    let mut image_bytes = 0usize;
    let mut image_base64_bytes = 0usize;
    for content in &result.content {
        match content {
            McpGatewayContent::Text { text } => {
                if text.len() > MCP_GATEWAY_MAX_TEXT_CONTENT_BYTES {
                    return Err(format!(
                        "tool result text exceeds maximum {} bytes",
                        MCP_GATEWAY_MAX_TEXT_CONTENT_BYTES
                    ));
                }
                validate_text_controls(text, "tool result text")?;
                text_bytes = text_bytes.saturating_add(text.len());
            }
            McpGatewayContent::Image { data, mime_type } => {
                if data.is_empty() || data.len() > MCP_GATEWAY_MAX_IMAGE_BASE64_BYTES {
                    return Err(format!(
                        "tool result image base64 must contain 1..={} bytes",
                        MCP_GATEWAY_MAX_IMAGE_BASE64_BYTES
                    ));
                }
                if mime_type.is_empty()
                    || mime_type.len() > MCP_GATEWAY_MAX_IMAGE_MIME_BYTES
                    || mime_type.chars().any(char::is_control)
                {
                    return Err("tool result image mimeType is invalid".to_string());
                }
                if !matches!(
                    mime_type.as_str(),
                    "image/png" | "image/jpeg" | "image/webp"
                ) {
                    return Err(format!(
                        "tool result image mimeType '{mime_type}' is unsupported"
                    ));
                }
                let decoded = general_purpose::STANDARD
                    .decode(data.as_bytes())
                    .map_err(|_| {
                        "tool result image data is not valid standard base64".to_string()
                    })?;
                if decoded.is_empty() || decoded.len() > MCP_GATEWAY_MAX_IMAGE_BYTES {
                    return Err(format!(
                        "tool result image exceeds maximum {} decoded bytes",
                        MCP_GATEWAY_MAX_IMAGE_BYTES
                    ));
                }
                let detected_mime = sniff_supported_image_mime(&decoded).ok_or_else(|| {
                    "tool result image data is not a supported PNG, JPEG, or WebP image".to_string()
                })?;
                if detected_mime != mime_type {
                    return Err(format!(
                        "tool result image mimeType '{mime_type}' does not match decoded content '{detected_mime}'"
                    ));
                }
                image_bytes = image_bytes.saturating_add(decoded.len());
                image_base64_bytes = image_base64_bytes.saturating_add(data.len());
                if image_bytes > MCP_GATEWAY_MAX_IMAGE_BYTES
                    || image_base64_bytes > MCP_GATEWAY_MAX_IMAGE_BASE64_BYTES
                {
                    return Err(format!(
                        "tool result images exceed aggregate maximum {} decoded bytes",
                        MCP_GATEWAY_MAX_IMAGE_BYTES
                    ));
                }
            }
        }
    }
    if let Some(structured) = result.structured_content.as_ref() {
        if !structured.is_object() {
            return Err("structured tool result must be a JSON object".to_string());
        }
        validate_json_value(
            structured,
            MCP_GATEWAY_MAX_STRUCTURED_CONTENT_BYTES,
            "structured tool result",
        )?;
    }
    let encoded = serde_json::to_vec(result)
        .map_err(|_| "tool result could not be serialized".to_string())?;
    let non_image_wire_bytes = encoded.len().saturating_sub(image_base64_bytes);
    if encoded.len() > MCP_GATEWAY_MAX_RESULT_WIRE_BYTES
        || non_image_wire_bytes > MCP_GATEWAY_MAX_RESULT_BYTES
        || text_bytes > MCP_GATEWAY_MAX_RESULT_BYTES
    {
        return Err(format!(
            "tool result exceeds maximum {} non-image bytes plus bounded image data",
            MCP_GATEWAY_MAX_RESULT_BYTES
        ));
    }
    Ok(())
}

fn sniff_supported_image_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

pub fn validate_json_value(value: &Value, max_bytes: usize, field: &str) -> Result<(), String> {
    let encoded =
        serde_json::to_vec(value).map_err(|_| format!("{field} could not be serialized"))?;
    if encoded.len() > max_bytes {
        return Err(format!("{field} exceeds maximum {max_bytes} bytes"));
    }
    let mut nodes = 0usize;
    validate_json_node(value, 0, &mut nodes, field)
}

fn validate_json_node(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    field: &str,
) -> Result<(), String> {
    if depth > MCP_GATEWAY_MAX_JSON_DEPTH {
        return Err(format!(
            "{field} exceeds maximum JSON depth {}",
            MCP_GATEWAY_MAX_JSON_DEPTH
        ));
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > MCP_GATEWAY_MAX_JSON_NODES {
        return Err(format!(
            "{field} exceeds maximum JSON node count {}",
            MCP_GATEWAY_MAX_JSON_NODES
        ));
    }
    match value {
        Value::String(text) => {
            if text.len() > MCP_GATEWAY_MAX_JSON_STRING_BYTES {
                return Err(format!(
                    "{field} contains a string larger than {} bytes",
                    MCP_GATEWAY_MAX_JSON_STRING_BYTES
                ));
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_json_node(value, depth + 1, nodes, field)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if key.len() > 1_024 || key.chars().any(char::is_control) {
                    return Err(format!("{field} contains an invalid object key"));
                }
                validate_json_node(value, depth + 1, nodes, field)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn validate_text_controls(value: &str, field: &str) -> Result<(), String> {
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(format!("{field} contains unsupported control characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_recursive_and_excessive_values() {
        let mut value = json!({});
        for _ in 0..=MCP_GATEWAY_MAX_JSON_DEPTH {
            value = json!({"next": value});
        }
        assert!(
            validate_json_value(&value, MCP_GATEWAY_MAX_SCHEMA_BYTES, "schema")
                .unwrap_err()
                .contains("depth")
        );

        let arguments = json!({"value": "x".repeat(MCP_GATEWAY_MAX_ARGUMENT_BYTES)});
        assert!(
            validate_json_value(&arguments, MCP_GATEWAY_MAX_ARGUMENT_BYTES, "arguments").is_err()
        );
    }

    #[test]
    fn tool_result_bounds_keep_non_image_limits_and_add_narrow_image_budget() {
        assert_eq!(MCP_GATEWAY_MAX_ARGUMENT_BYTES, 64 * 1024);
        assert_eq!(MCP_GATEWAY_MAX_SCHEMA_BYTES, 64 * 1024);
        assert_eq!(MCP_GATEWAY_MAX_TEXT_CONTENT_BYTES, 512 * 1024);
        assert_eq!(MCP_GATEWAY_MAX_STRUCTURED_CONTENT_BYTES, 512 * 1024);
        assert_eq!(MCP_GATEWAY_MAX_RESULT_BYTES, 512 * 1024);
        assert_eq!(MCP_GATEWAY_MAX_JSON_STRING_BYTES, 512 * 1024);
        assert_eq!(MCP_GATEWAY_MAX_MESSAGE_BYTES, 1024 * 1024);
        assert_eq!(MCP_GATEWAY_MAX_IMAGE_BYTES, 4 * 1024 * 1024);
        assert_eq!(MCP_GATEWAY_MAX_IMAGE_BASE64_BYTES, 5_592_408);
        assert!(
            MCP_GATEWAY_MAX_PROVIDER_MESSAGE_BYTES
                < crate::runner_protocol::RUNNER_ENVELOPE_MAX_BYTES
        );
        assert!(
            MCP_GATEWAY_MAX_TOOL_RESULT_MESSAGE_BYTES
                < crate::runner_protocol::RUNNER_ENVELOPE_MAX_BYTES
        );

        let large_text = McpGatewayToolResult {
            content: vec![McpGatewayContent::Text {
                text: "x".repeat(384 * 1024),
            }],
            structured_content: None,
            is_error: false,
        };
        validate_tool_result(&large_text).unwrap();

        let large_structured = McpGatewayToolResult {
            content: vec![],
            structured_content: Some(json!({"payload": "x".repeat(384 * 1024)})),
            is_error: false,
        };
        validate_tool_result(&large_structured).unwrap();

        let aggregate_oversized = McpGatewayToolResult {
            content: vec![McpGatewayContent::Text {
                text: "x".repeat(300 * 1024),
            }],
            structured_content: Some(json!({"payload": "y".repeat(300 * 1024)})),
            is_error: false,
        };
        assert!(validate_tool_result(&aggregate_oversized).is_err());

        let oversized_text = McpGatewayToolResult {
            content: vec![McpGatewayContent::Text {
                text: "x".repeat(MCP_GATEWAY_MAX_TEXT_CONTENT_BYTES + 1),
            }],
            structured_content: None,
            is_error: false,
        };
        assert!(validate_tool_result(&oversized_text).is_err());

        let oversized_structured = McpGatewayToolResult {
            content: vec![],
            structured_content: Some(json!({
                "payload": "x".repeat(MCP_GATEWAY_MAX_STRUCTURED_CONTENT_BYTES + 1)
            })),
            is_error: false,
        };
        assert!(validate_tool_result(&oversized_structured).is_err());

        let oversized_arguments = json!({"value": "x".repeat(MCP_GATEWAY_MAX_ARGUMENT_BYTES)});
        assert!(validate_json_value(
            &oversized_arguments,
            MCP_GATEWAY_MAX_ARGUMENT_BYTES,
            "arguments"
        )
        .is_err());
        let oversized_schema = json!({"description": "x".repeat(MCP_GATEWAY_MAX_SCHEMA_BYTES)});
        assert!(
            validate_json_value(&oversized_schema, MCP_GATEWAY_MAX_SCHEMA_BYTES, "schema").is_err()
        );
    }

    #[test]
    fn image_content_round_trips_standard_wire_for_supported_mimes() {
        for (mime_type, data) in [
            ("image/png", "iVBORw0KGgo="),
            ("image/jpeg", "/9j/"),
            ("image/webp", "UklGRgAAAABXRUJQ"),
        ] {
            let result = McpGatewayToolResult {
                content: vec![McpGatewayContent::Image {
                    data: data.to_string(),
                    mime_type: mime_type.to_string(),
                }],
                structured_content: None,
                is_error: false,
            };
            validate_tool_result(&result).unwrap();
            let wire = serde_json::to_value(&result).unwrap();
            assert_eq!(wire["content"][0]["type"], "image");
            assert_eq!(wire["content"][0]["data"], data);
            assert_eq!(wire["content"][0]["mimeType"], mime_type);
            assert!(wire["content"][0].get("mime_type").is_none());
            let decoded: McpGatewayToolResult = serde_json::from_value(wire).unwrap();
            assert_eq!(decoded, result);
        }
    }

    #[test]
    fn image_validation_rejects_bad_base64_mime_and_decoded_or_aggregate_oversize() {
        let invalid = |data: String, mime_type: &str| McpGatewayToolResult {
            content: vec![McpGatewayContent::Image {
                data,
                mime_type: mime_type.to_string(),
            }],
            structured_content: None,
            is_error: false,
        };
        assert!(
            validate_tool_result(&invalid("%%%".to_string(), "image/png"))
                .unwrap_err()
                .contains("base64")
        );
        assert!(
            validate_tool_result(&invalid("AA==".to_string(), "text/plain"))
                .unwrap_err()
                .contains("mimeType")
        );
        assert!(
            validate_tool_result(&invalid("iVBORw0KGgo=".to_string(), "image/jpeg"))
                .unwrap_err()
                .contains("does not match")
        );

        let mut max_image_bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        max_image_bytes.resize(MCP_GATEWAY_MAX_IMAGE_BYTES, 0);
        let max_image = general_purpose::STANDARD.encode(max_image_bytes);
        assert_eq!(max_image.len(), MCP_GATEWAY_MAX_IMAGE_BASE64_BYTES);
        let max_result = invalid(max_image, "image/png");
        validate_tool_result(&max_result).unwrap();
        validate_response(&McpGatewayResponse::success(
            McpGatewayResponsePayload::ToolResult { result: max_result },
        ))
        .unwrap();

        let oversized =
            general_purpose::STANDARD.encode(vec![0u8; MCP_GATEWAY_MAX_IMAGE_BYTES + 1]);
        assert!(validate_tool_result(&invalid(oversized, "image/png")).is_err());

        let non_image_over_budget = McpGatewayToolResult {
            content: vec![
                McpGatewayContent::Text {
                    text: "t".repeat(300 * 1024),
                },
                McpGatewayContent::Image {
                    data: "iVBORw0KGgo=".to_string(),
                    mime_type: "image/png".to_string(),
                },
            ],
            structured_content: Some(json!({"payload": "s".repeat(300 * 1024)})),
            is_error: false,
        };
        assert!(validate_tool_result(&non_image_over_budget)
            .unwrap_err()
            .contains("non-image"));

        let first_image_bytes = MCP_GATEWAY_MAX_IMAGE_BYTES / 2;
        let second_image_bytes = MCP_GATEWAY_MAX_IMAGE_BYTES - first_image_bytes + 1;
        let mut png_chunk = b"\x89PNG\r\n\x1a\n".to_vec();
        png_chunk.resize(first_image_bytes, 0);
        let mut jpeg_chunk = vec![0xff, 0xd8, 0xff];
        jpeg_chunk.resize(second_image_bytes, 0);
        let aggregate = McpGatewayToolResult {
            content: vec![
                McpGatewayContent::Image {
                    data: general_purpose::STANDARD.encode(png_chunk),
                    mime_type: "image/png".to_string(),
                },
                McpGatewayContent::Image {
                    data: general_purpose::STANDARD.encode(jpeg_chunk),
                    mime_type: "image/jpeg".to_string(),
                },
            ],
            structured_content: None,
            is_error: false,
        };
        assert!(validate_tool_result(&aggregate)
            .unwrap_err()
            .contains("aggregate maximum"));
    }

    #[test]
    fn mixed_text_image_and_structured_content_preserve_order_and_budget() {
        let result = McpGatewayToolResult {
            content: vec![
                McpGatewayContent::Text {
                    text: "before".to_string(),
                },
                McpGatewayContent::Image {
                    data: "iVBORw0KGgo=".to_string(),
                    mime_type: "image/png".to_string(),
                },
                McpGatewayContent::Text {
                    text: "after".to_string(),
                },
            ],
            structured_content: Some(json!({"kind": "mixed"})),
            is_error: true,
        };
        validate_tool_result(&result).unwrap();
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(wire["content"][0]["text"], "before");
        assert_eq!(wire["content"][1]["type"], "image");
        assert_eq!(wire["content"][2]["text"], "after");
        assert_eq!(wire["structuredContent"]["kind"], "mixed");
        assert_eq!(wire["isError"], true);
    }

    #[test]
    fn provider_status_wire_is_bounded_and_contains_no_local_process_details() {
        let request = McpGatewayRequest::ProviderStatus {
            provider_id: "provider".to_string(),
            provider_instance_id: "instance".to_string(),
        };
        validate_request(&request).unwrap();
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["operation"], "provider_status");
        assert!(encoded.get("pid").is_none());
        assert!(encoded.get("executable").is_none());

        let response = McpGatewayResponse::success(McpGatewayResponsePayload::ProviderStatus {
            state: McpGatewayProviderState::NeverStarted,
        });
        validate_response(&response).unwrap();
        let encoded = serde_json::to_value(response).unwrap();
        assert_eq!(encoded["payload"]["state"], "never_started");
    }

    #[test]
    fn tools_call_wire_has_no_caller_meta_but_tool_descriptor_meta_is_preserved() {
        let request = McpGatewayRequest::ToolsCall {
            provider_id: "provider".to_string(),
            provider_instance_id: "instance".to_string(),
            name: "echo".to_string(),
            arguments: json!({"value": "hello"}),
            expected_schema: McpGatewaySchemaObservation {
                input_schema: json!({"type": "object"}),
                output_schema: None,
                annotations: None,
            },
        };
        let encoded = serde_json::to_value(&request).unwrap();
        assert!(encoded.get("_meta").is_none());
        let mut injected = encoded;
        injected["_meta"] = json!({"progressToken": "outer"});
        assert!(serde_json::from_value::<McpGatewayRequest>(injected).is_err());

        let tool = McpGatewayTool {
            name: "echo".to_string(),
            title: None,
            description: None,
            input_schema: json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            meta: Some(json!({"providerExtension": true})),
        };
        validate_tools(std::slice::from_ref(&tool)).unwrap();
        assert_eq!(
            serde_json::to_value(tool).unwrap()["_meta"]["providerExtension"],
            true
        );
    }

    #[test]
    fn rejects_duplicate_tools_and_unsupported_content() {
        let tool = McpGatewayTool {
            name: "echo".to_string(),
            title: None,
            description: None,
            input_schema: json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            meta: None,
        };
        assert!(validate_tools(&[tool.clone(), tool]).is_err());

        let raw = json!({
            "content": [{"type": "audio", "data": "AA==", "mimeType": "audio/wav"}],
            "isError": false
        });
        assert!(serde_json::from_value::<McpGatewayToolResult>(raw).is_err());
    }
}

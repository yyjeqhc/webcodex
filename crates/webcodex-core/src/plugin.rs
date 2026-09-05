//! Bounded transport contracts for Runner-owned native Tool Plugins.
//!
//! Plugins speak the WebCodex Plugin Protocol over local stdio.  This module is
//! deliberately independent from MCP: MCP is one adapter that may expose an
//! admitted startup catalog, while these types describe the native Runner
//! protocol and the closed Server <-> Runner gateway.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const PLUGIN_PROTOCOL_VERSION: &str = "webcodex-plugin-v1";
pub const PLUGIN_MAX_PROVIDERS: usize = 8;
pub const PLUGIN_MAX_PROVIDER_ID_BYTES: usize = 64;
pub const PLUGIN_MAX_PROVIDER_NAME_BYTES: usize = 128;
pub const PLUGIN_MAX_TOOL_COUNT: usize = 128;
pub const PLUGIN_STARTUP_MAX_DIRECT_TOOLS: usize = 64;
pub const PLUGIN_MAX_TOOL_NAME_BYTES: usize = 128;
pub const PLUGIN_MAX_DESCRIPTION_BYTES: usize = 4 * 1024;
pub const PLUGIN_MAX_SCHEMA_BYTES: usize = 64 * 1024;
pub const PLUGIN_STARTUP_MAX_SCHEMA_BYTES: usize = 32 * 1024;
pub const PLUGIN_MAX_ARGUMENT_BYTES: usize = 64 * 1024;
pub const PLUGIN_MAX_STRUCTURED_CONTENT_BYTES: usize = 128 * 1024;
pub const PLUGIN_MAX_TEXT_CONTENT_BYTES: usize = 64 * 1024;
pub const PLUGIN_MAX_RESULT_BYTES: usize = 256 * 1024;
pub const PLUGIN_MAX_CONTENT_ITEMS: usize = 32;
pub const PLUGIN_MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const PLUGIN_MAX_JSON_DEPTH: usize = 16;
pub const PLUGIN_MAX_JSON_NODES: usize = 4_096;
pub const PLUGIN_MAX_JSON_STRING_BYTES: usize = 64 * 1024;
pub const PLUGIN_STARTUP_CATALOG_MAX_BYTES: usize = 256 * 1024;
pub const PLUGIN_MAX_CHECK_DETAIL_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginPlane {
    Startup,
    Effective,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginGatewayRequest {
    Check {
        provider_id: String,
    },
    Reload,
    ProvidersList,
    ToolsList {
        plane: PluginPlane,
        provider_id: String,
        provider_instance_id: String,
    },
    ToolsCall {
        plane: PluginPlane,
        provider_id: String,
        provider_instance_id: String,
        name: String,
        arguments: Value,
        expected_schema: PluginSchemaObservation,
    },
}

impl PluginGatewayRequest {
    pub fn provider_binding(&self) -> Option<(&str, &str, PluginPlane)> {
        match self {
            Self::ToolsList {
                plane,
                provider_id,
                provider_instance_id,
            }
            | Self::ToolsCall {
                plane,
                provider_id,
                provider_instance_id,
                ..
            } => Some((provider_id, provider_instance_id, *plane)),
            Self::Check { .. } | Self::Reload | Self::ProvidersList => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginDispatchState {
    NotStarted,
    OutcomeUnknown,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginTool {
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSchemaObservation {
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

impl PluginTool {
    pub fn schema_observation(&self) -> PluginSchemaObservation {
        PluginSchemaObservation {
            input_schema: self.input_schema.clone(),
            output_schema: self.output_schema.clone(),
            annotations: self.annotations.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum PluginContent {
    Text { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginToolResult {
    pub content: Vec<PluginContent>,
    #[serde(
        default,
        rename = "structuredContent",
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<Value>,
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

/// Frozen, sanitized startup registration entry.  `tools` contains only the
/// all-or-nothing first-class-admitted subset for this provider (currently the
/// complete provider list or none); execution configuration never crosses the
/// Runner boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupPluginProvider {
    pub provider_id: String,
    pub provider_instance_id: String,
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default)]
    pub tools: Vec<PluginTool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginProviderView {
    pub provider_id: String,
    pub provider_instance_id: String,
    pub name: String,
    pub plane: PluginPlane,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default)]
    pub startup_direct_tool_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginReloadFailure {
    pub provider_id: String,
    pub code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCheckPhase {
    Config,
    Environment,
    Executable,
    Spawn,
    Stdio,
    Initialize,
    ToolsList,
    Validation,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCheckToolSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginStartupToolShape {
    pub eligible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCheckReport {
    pub provider_id: String,
    pub ready: bool,
    pub phase: PluginCheckPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub tool_count: usize,
    #[serde(default)]
    pub tools: Vec<PluginCheckToolSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_tool_shape: Option<PluginStartupToolShape>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginGatewayResponsePayload {
    Checked {
        report: PluginCheckReport,
    },
    Providers {
        providers: Vec<PluginProviderView>,
        first_class_restart_required: bool,
    },
    Reloaded {
        providers: Vec<PluginProviderView>,
        failures: Vec<PluginReloadFailure>,
        first_class_restart_required: bool,
    },
    Tools {
        tools: Vec<PluginTool>,
    },
    ToolResult {
        result: PluginToolResult,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginGatewayError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginGatewayResponse {
    pub dispatch_state: PluginDispatchState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<PluginGatewayResponsePayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<PluginGatewayError>,
}

impl PluginGatewayResponse {
    pub fn success(payload: PluginGatewayResponsePayload) -> Self {
        Self {
            dispatch_state: PluginDispatchState::Completed,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn error(
        dispatch_state: PluginDispatchState,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            dispatch_state,
            payload: None,
            error: Some(PluginGatewayError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

pub fn validate_provider_id(value: &str) -> Result<(), String> {
    validate_identifier(value, "provider_id", PLUGIN_MAX_PROVIDER_ID_BYTES, true)
}

pub fn validate_provider_instance_id(value: &str) -> Result<(), String> {
    validate_identifier(
        value,
        "provider_instance_id",
        PLUGIN_MAX_PROVIDER_ID_BYTES,
        false,
    )
}

pub fn validate_tool_name(value: &str) -> Result<(), String> {
    validate_identifier(value, "tool name", PLUGIN_MAX_TOOL_NAME_BYTES, false)
}

pub fn validate_provider_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > PLUGIN_MAX_PROVIDER_NAME_BYTES {
        return Err(format!(
            "provider name must contain 1..={} bytes",
            PLUGIN_MAX_PROVIDER_NAME_BYTES
        ));
    }
    validate_text_controls(value, "provider name")
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
        return Err(format!("{field} contains unsupported characters"));
    }
    Ok(())
}

pub fn validate_request(request: &PluginGatewayRequest) -> Result<(), String> {
    match request {
        PluginGatewayRequest::Check { provider_id } => validate_provider_id(provider_id),
        PluginGatewayRequest::Reload | PluginGatewayRequest::ProvidersList => Ok(()),
        PluginGatewayRequest::ToolsList {
            provider_id,
            provider_instance_id,
            ..
        } => {
            validate_provider_id(provider_id)?;
            validate_provider_instance_id(provider_instance_id)
        }
        PluginGatewayRequest::ToolsCall {
            provider_id,
            provider_instance_id,
            name,
            arguments,
            expected_schema,
            ..
        } => {
            validate_provider_id(provider_id)?;
            validate_provider_instance_id(provider_instance_id)?;
            validate_tool_name(name)?;
            if !arguments.is_object() {
                return Err("tool arguments must be a JSON object".to_string());
            }
            validate_json_value(arguments, PLUGIN_MAX_ARGUMENT_BYTES, "tool arguments")?;
            validate_schema_observation(expected_schema)
        }
    }
}

pub fn validate_tools(tools: &[PluginTool]) -> Result<(), String> {
    if tools.len() > PLUGIN_MAX_TOOL_COUNT {
        return Err(format!(
            "tool count exceeds maximum {}",
            PLUGIN_MAX_TOOL_COUNT
        ));
    }
    let mut names = HashSet::new();
    for tool in tools {
        validate_tool_name(&tool.name)?;
        if !names.insert(tool.name.as_str()) {
            return Err(format!("duplicate tool name '{}'", tool.name));
        }
        if let Some(title) = tool.title.as_deref() {
            if title.is_empty() || title.len() > PLUGIN_MAX_PROVIDER_NAME_BYTES {
                return Err("tool title is empty or too long".to_string());
            }
            validate_text_controls(title, "tool title")?;
        }
        if let Some(description) = tool.description.as_deref() {
            if description.len() > PLUGIN_MAX_DESCRIPTION_BYTES {
                return Err("tool description exceeds plugin bounds".to_string());
            }
            validate_text_controls(description, "tool description")?;
        }
        validate_schema_observation(&tool.schema_observation())?;
    }
    Ok(())
}

pub fn validate_schema_observation(observation: &PluginSchemaObservation) -> Result<(), String> {
    if !observation.input_schema.is_object() {
        return Err("tool inputSchema must be a JSON object".to_string());
    }
    validate_json_value(
        &observation.input_schema,
        PLUGIN_MAX_SCHEMA_BYTES,
        "tool inputSchema",
    )?;
    if let Some(output) = observation.output_schema.as_ref() {
        if !output.is_object() {
            return Err("tool outputSchema must be a JSON object".to_string());
        }
        validate_json_value(output, PLUGIN_MAX_SCHEMA_BYTES, "tool outputSchema")?;
    }
    if let Some(annotations) = observation.annotations.as_ref() {
        if !annotations.is_object() {
            return Err("tool annotations must be a JSON object".to_string());
        }
        validate_json_value(annotations, PLUGIN_MAX_SCHEMA_BYTES, "tool annotations")?;
    }
    Ok(())
}

pub fn validate_startup_tool(tool: &PluginTool) -> Result<(), String> {
    validate_tools(std::slice::from_ref(tool))?;
    validate_json_value(
        &tool.input_schema,
        PLUGIN_STARTUP_MAX_SCHEMA_BYTES,
        "startup tool inputSchema",
    )?;
    if let Some(output) = tool.output_schema.as_ref() {
        validate_json_value(
            output,
            PLUGIN_STARTUP_MAX_SCHEMA_BYTES,
            "startup tool outputSchema",
        )?;
    }
    if let Some(annotations) = tool.annotations.as_ref() {
        validate_json_value(
            annotations,
            PLUGIN_STARTUP_MAX_SCHEMA_BYTES,
            "startup tool annotations",
        )?;
    }
    Ok(())
}

pub fn validate_startup_catalog(providers: &[StartupPluginProvider]) -> Result<(), String> {
    if providers.len() > PLUGIN_MAX_PROVIDERS {
        return Err("startup plugin provider count exceeds bound".to_string());
    }
    let mut ids = HashSet::new();
    let mut instances = HashSet::new();
    let mut total_tools = 0usize;
    for provider in providers {
        validate_provider_id(&provider.provider_id)?;
        validate_provider_instance_id(&provider.provider_instance_id)?;
        validate_provider_name(&provider.name)?;
        if !ids.insert(provider.provider_id.as_str())
            || !instances.insert(provider.provider_instance_id.as_str())
        {
            return Err("duplicate startup plugin provider identity".to_string());
        }
        validate_status_atom(&provider.status, "startup plugin status")?;
        if let Some(code) = provider.error_code.as_deref() {
            validate_status_atom(code, "startup plugin error code")?;
        }
        total_tools = total_tools.saturating_add(provider.tools.len());
        if total_tools > PLUGIN_STARTUP_MAX_DIRECT_TOOLS {
            return Err("startup direct tool count exceeds bound".to_string());
        }
        for tool in &provider.tools {
            validate_startup_tool(tool)?;
        }
    }
    let encoded = serde_json::to_vec(providers)
        .map_err(|_| "startup plugin catalog could not be serialized".to_string())?;
    if encoded.len() > PLUGIN_STARTUP_CATALOG_MAX_BYTES {
        return Err("startup plugin catalog exceeds aggregate byte bound".to_string());
    }
    Ok(())
}

pub fn validate_tool_result(result: &PluginToolResult) -> Result<(), String> {
    if result.content.len() > PLUGIN_MAX_CONTENT_ITEMS {
        return Err("tool result content item count exceeds bound".to_string());
    }
    let mut text_bytes = 0usize;
    for content in &result.content {
        let PluginContent::Text { text } = content;
        if text.len() > PLUGIN_MAX_TEXT_CONTENT_BYTES {
            return Err("tool result text exceeds bound".to_string());
        }
        validate_text_controls(text, "tool result text")?;
        text_bytes = text_bytes.saturating_add(text.len());
    }
    if let Some(structured) = result.structured_content.as_ref() {
        if !structured.is_object() {
            return Err("structuredContent must be a JSON object".to_string());
        }
        validate_json_value(
            structured,
            PLUGIN_MAX_STRUCTURED_CONTENT_BYTES,
            "structuredContent",
        )?;
    }
    let encoded = serde_json::to_vec(result)
        .map_err(|_| "tool result could not be serialized".to_string())?;
    if encoded.len() > PLUGIN_MAX_RESULT_BYTES || text_bytes > PLUGIN_MAX_RESULT_BYTES {
        return Err("tool result exceeds aggregate bound".to_string());
    }
    Ok(())
}

pub fn validate_response(response: &PluginGatewayResponse) -> Result<(), String> {
    let encoded = serde_json::to_vec(response)
        .map_err(|_| "plugin gateway response could not be serialized".to_string())?;
    if encoded.len() > PLUGIN_MAX_MESSAGE_BYTES {
        return Err("plugin gateway response exceeds message bound".to_string());
    }
    match (&response.payload, &response.error) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(
                "plugin gateway response must contain exactly one payload or error".to_string(),
            )
        }
        (Some(_), None) if response.dispatch_state != PluginDispatchState::Completed => {
            return Err("plugin gateway payload requires completed dispatch".to_string())
        }
        (None, Some(error)) => {
            validate_status_atom(&error.code, "plugin gateway error code")?;
            if error.message.is_empty() || error.message.len() > 512 {
                return Err("plugin gateway error message is invalid".to_string());
            }
            validate_text_controls(&error.message, "plugin gateway error message")?;
        }
        _ => {}
    }
    if let Some(payload) = response.payload.as_ref() {
        match payload {
            PluginGatewayResponsePayload::Checked { report } => validate_check_report(report)?,
            PluginGatewayResponsePayload::Providers { providers, .. } => {
                validate_provider_views(providers)?
            }
            PluginGatewayResponsePayload::Reloaded {
                providers,
                failures,
                ..
            } => {
                validate_provider_views(providers)?;
                if failures.len() > PLUGIN_MAX_PROVIDERS {
                    return Err("plugin reload failure count exceeds bound".to_string());
                }
                for failure in failures {
                    validate_provider_id(&failure.provider_id)?;
                    validate_status_atom(&failure.code, "plugin reload failure code")?;
                }
            }
            PluginGatewayResponsePayload::Tools { tools } => validate_tools(tools)?,
            PluginGatewayResponsePayload::ToolResult { result } => validate_tool_result(result)?,
        }
    }
    Ok(())
}

pub fn validate_response_for_request(
    request: &PluginGatewayRequest,
    response: &PluginGatewayResponse,
) -> Result<(), String> {
    validate_response(response)?;
    if response.error.is_some() {
        return Ok(());
    }
    match (request, response.payload.as_ref()) {
        (
            PluginGatewayRequest::Check { provider_id },
            Some(PluginGatewayResponsePayload::Checked { report }),
        ) => {
            if report.provider_id != *provider_id {
                return Err("Plugin check response provider_id does not match request".to_string());
            }
        }
        (PluginGatewayRequest::Reload, Some(PluginGatewayResponsePayload::Reloaded { .. }))
        | (
            PluginGatewayRequest::ProvidersList,
            Some(PluginGatewayResponsePayload::Providers { .. }),
        )
        | (
            PluginGatewayRequest::ToolsList { .. },
            Some(PluginGatewayResponsePayload::Tools { .. }),
        )
        | (
            PluginGatewayRequest::ToolsCall { .. },
            Some(PluginGatewayResponsePayload::ToolResult { .. }),
        ) => {}
        _ => {
            return Err("Plugin gateway response kind does not match request operation".to_string())
        }
    }
    Ok(())
}

pub fn validate_check_report(report: &PluginCheckReport) -> Result<(), String> {
    validate_provider_id(&report.provider_id)?;
    if report.ready {
        if report.phase != PluginCheckPhase::Ready
            || report.code.is_some()
            || report.detail.is_some()
        {
            return Err("ready Plugin check report has inconsistent status fields".to_string());
        }
        if report.startup_tool_shape.is_none() {
            return Err("ready Plugin check report requires startup tool shape".to_string());
        }
    } else {
        if report.phase == PluginCheckPhase::Ready || report.code.is_none() {
            return Err("failed Plugin check report has inconsistent status fields".to_string());
        }
        if report.tool_count != 0 || !report.tools.is_empty() || report.startup_tool_shape.is_some()
        {
            return Err("failed Plugin check report must not retain tool inventory".to_string());
        }
    }
    if let Some(code) = report.code.as_deref() {
        validate_status_atom(code, "Plugin check code")?;
    }
    if let Some(detail) = report.detail.as_deref() {
        if detail.is_empty() || detail.len() > PLUGIN_MAX_CHECK_DETAIL_BYTES {
            return Err("Plugin check detail exceeds bound".to_string());
        }
        validate_text_controls(detail, "Plugin check detail")?;
    }
    if report.tool_count != report.tools.len() || report.tools.len() > PLUGIN_MAX_TOOL_COUNT {
        return Err("Plugin check tool summary count is invalid".to_string());
    }
    for tool in &report.tools {
        validate_tool_name(&tool.name)?;
        if let Some(title) = tool.title.as_deref() {
            if title.is_empty() || title.len() > PLUGIN_MAX_PROVIDER_NAME_BYTES {
                return Err("Plugin check tool title is invalid".to_string());
            }
            validate_text_controls(title, "Plugin check tool title")?;
        }
    }
    if let Some(shape) = report.startup_tool_shape.as_ref() {
        match (shape.eligible, shape.code.as_deref()) {
            (true, None) => {}
            (false, Some(code)) => validate_status_atom(code, "Plugin startup tool shape code")?,
            _ => return Err("Plugin startup tool shape status is inconsistent".to_string()),
        }
    }
    Ok(())
}

fn validate_provider_views(providers: &[PluginProviderView]) -> Result<(), String> {
    if providers.len() > PLUGIN_MAX_PROVIDERS {
        return Err("plugin provider count exceeds bound".to_string());
    }
    let mut ids = HashSet::new();
    for provider in providers {
        validate_provider_id(&provider.provider_id)?;
        validate_provider_instance_id(&provider.provider_instance_id)?;
        validate_provider_name(&provider.name)?;
        validate_status_atom(&provider.status, "plugin provider status")?;
        if let Some(code) = provider.error_code.as_deref() {
            validate_status_atom(code, "plugin provider error code")?;
        }
        if provider.startup_direct_tool_count > PLUGIN_STARTUP_MAX_DIRECT_TOOLS {
            return Err("startup direct tool count exceeds bound".to_string());
        }
        if !ids.insert(provider.provider_id.as_str()) {
            return Err("duplicate plugin provider id".to_string());
        }
    }
    Ok(())
}

fn validate_status_atom(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(format!("{field} is invalid"));
    }
    Ok(())
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
    if depth > PLUGIN_MAX_JSON_DEPTH {
        return Err(format!("{field} exceeds JSON depth bound"));
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > PLUGIN_MAX_JSON_NODES {
        return Err(format!("{field} exceeds JSON node bound"));
    }
    match value {
        Value::String(text) => {
            if text.len() > PLUGIN_MAX_JSON_STRING_BYTES {
                return Err(format!("{field} contains an oversized string"));
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_json_node(value, depth + 1, nodes, field)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if key.len() > 1024 || key.chars().any(char::is_control) {
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

    fn tool() -> PluginTool {
        PluginTool {
            name: "echo".to_string(),
            title: None,
            description: Some("Echo input".to_string()),
            input_schema: json!({"type":"object"}),
            output_schema: None,
            annotations: None,
        }
    }

    #[test]
    fn startup_catalog_is_aggregate_bounded() {
        let provider = StartupPluginProvider {
            provider_id: "repo-tools".to_string(),
            provider_instance_id: "instance_1".to_string(),
            name: "Repo Tools".to_string(),
            status: "ready".to_string(),
            error_code: None,
            tools: vec![tool()],
        };
        validate_startup_catalog(&[provider]).unwrap();
    }

    #[test]
    fn result_rejects_unsupported_content_at_deserialize_boundary() {
        let value = json!({"content":[{"type":"image","data":"x"}],"isError":false});
        assert!(serde_json::from_value::<PluginToolResult>(value).is_err());
    }

    #[test]
    fn request_requires_object_arguments_and_bounded_schema() {
        let request = PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Effective,
            provider_id: "repo-tools".to_string(),
            provider_instance_id: "instance_1".to_string(),
            name: "echo".to_string(),
            arguments: json!([]),
            expected_schema: tool().schema_observation(),
        };
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn check_contract_is_typed_and_report_is_bounded() {
        let request = PluginGatewayRequest::Check {
            provider_id: "repo-tools".to_string(),
        };
        validate_request(&request).unwrap();

        let report = PluginCheckReport {
            provider_id: "repo-tools".to_string(),
            ready: true,
            phase: PluginCheckPhase::Ready,
            code: None,
            detail: None,
            tool_count: 1,
            tools: vec![PluginCheckToolSummary {
                name: "echo".to_string(),
                title: Some("Echo".to_string()),
            }],
            startup_tool_shape: Some(PluginStartupToolShape {
                eligible: true,
                code: None,
            }),
        };
        validate_check_report(&report).unwrap();
        validate_response(&PluginGatewayResponse::success(
            PluginGatewayResponsePayload::Checked {
                report: report.clone(),
            },
        ))
        .unwrap();
        validate_response_for_request(
            &request,
            &PluginGatewayResponse::success(PluginGatewayResponsePayload::Checked {
                report: report.clone(),
            }),
        )
        .unwrap();

        let mut wrong_provider = report.clone();
        wrong_provider.provider_id = "other-tools".to_string();
        assert!(validate_response_for_request(
            &request,
            &PluginGatewayResponse::success(PluginGatewayResponsePayload::Checked {
                report: wrong_provider,
            }),
        )
        .is_err());
        assert!(validate_response_for_request(
            &request,
            &PluginGatewayResponse::success(PluginGatewayResponsePayload::Providers {
                providers: vec![],
                first_class_restart_required: false,
            }),
        )
        .is_err());

        let mut invalid = report;
        invalid.detail = Some("x".repeat(PLUGIN_MAX_CHECK_DETAIL_BYTES + 1));
        invalid.ready = false;
        invalid.phase = PluginCheckPhase::Validation;
        invalid.code = Some("plugin_tools_list_invalid".to_string());
        invalid.tool_count = 0;
        invalid.tools.clear();
        invalid.startup_tool_shape = None;
        assert!(validate_check_report(&invalid).is_err());
    }

    #[test]
    fn schema_and_result_bounds_fail_closed() {
        let mut oversized_schema_tool = tool();
        oversized_schema_tool.input_schema = json!({
            "type": "object",
            "description": "x".repeat(PLUGIN_MAX_SCHEMA_BYTES)
        });
        assert!(validate_tools(&[oversized_schema_tool]).is_err());

        let oversized_result = PluginToolResult {
            content: vec![PluginContent::Text {
                text: "x".repeat(PLUGIN_MAX_TEXT_CONTENT_BYTES + 1),
            }],
            structured_content: None,
            is_error: false,
        };
        assert!(validate_tool_result(&oversized_result).is_err());
    }

    #[test]
    fn startup_catalog_rejects_total_tool_and_aggregate_byte_overflow() {
        let too_many_tools = (0..=PLUGIN_STARTUP_MAX_DIRECT_TOOLS)
            .map(|index| PluginTool {
                name: format!("tool_{index}"),
                ..tool()
            })
            .collect::<Vec<_>>();
        assert!(validate_startup_catalog(&[StartupPluginProvider {
            provider_id: "repo-tools".to_string(),
            provider_instance_id: "instance_1".to_string(),
            name: "Repo Tools".to_string(),
            status: "ready".to_string(),
            error_code: None,
            tools: too_many_tools,
        }])
        .is_err());

        let aggregate_tools = (0..10)
            .map(|index| PluginTool {
                name: format!("large_{index}"),
                input_schema: json!({
                    "type": "object",
                    "description": "x".repeat(30 * 1024)
                }),
                ..tool()
            })
            .collect::<Vec<_>>();
        assert!(validate_startup_catalog(&[StartupPluginProvider {
            provider_id: "repo-tools".to_string(),
            provider_instance_id: "instance_2".to_string(),
            name: "Repo Tools".to_string(),
            status: "ready".to_string(),
            error_code: None,
            tools: aggregate_tools,
        }])
        .is_err());
    }
}

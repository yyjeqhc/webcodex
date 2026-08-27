use super::protocol::{
    request_client_capabilities, request_client_info_is_valid, request_protocol_version,
    JsonRpcRequest, McpProtocolEra, MCP_STATELESS_PROTOCOL_VERSION,
    MCP_SUPPORTED_PROTOCOL_VERSIONS,
};
use super::response::{rpc_error, rpc_error_with_data};
use base64::engine::general_purpose;
use salvo::prelude::Request;
use serde_json::{json, Value};

pub(super) const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
pub(super) const MCP_METHOD_HEADER: &str = "mcp-method";
pub(super) const MCP_NAME_HEADER: &str = "mcp-name";
pub(super) const MCP_HEADER_MISMATCH: i64 = -32020;
pub(super) const MCP_UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

pub(super) fn request_header<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
    req.headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
}

fn decode_mcp_name_header(value: &str) -> Result<String, ()> {
    let Some(encoded) = value
        .strip_prefix("=?base64?")
        .and_then(|value| value.strip_suffix("?="))
    else {
        return Ok(value.to_string());
    };
    let bytes = base64::Engine::decode(&general_purpose::STANDARD, encoded).map_err(|_| ())?;
    String::from_utf8(bytes).map_err(|_| ())
}

fn request_mcp_name(request: &JsonRpcRequest) -> Option<Option<&str>> {
    match request.method.as_str() {
        "tools/call" | "prompts/get" => Some(request.params.get("name").and_then(Value::as_str)),
        "resources/read" => Some(request.params.get("uri").and_then(Value::as_str)),
        "tasks/get" | "tasks/update" | "tasks/cancel" => {
            Some(request.params.get("taskId").and_then(Value::as_str))
        }
        _ => None,
    }
}

fn header_mismatch(id: Option<Value>, message: impl Into<String>) -> Value {
    rpc_error(id, MCP_HEADER_MISMATCH, message)
}

fn unsupported_protocol_version(id: Option<Value>, requested: &str) -> Value {
    rpc_error_with_data(
        id,
        MCP_UNSUPPORTED_PROTOCOL_VERSION,
        format!("Unsupported MCP protocol version: {requested}"),
        json!({
            "supported": MCP_SUPPORTED_PROTOCOL_VERSIONS,
            "requested": requested,
        }),
    )
}

/// Validate the HTTP-only request metadata introduced by MCP 2026-07-28.
/// Requests with no modern markers retain the existing 2025-06-18 behavior.
pub(super) fn validate_http_protocol(
    req: &Request,
    request: &JsonRpcRequest,
) -> Result<McpProtocolEra, Value> {
    let id = request.id.clone();
    let header_version = request_header(req, MCP_PROTOCOL_VERSION_HEADER);
    let body_version = request_protocol_version(&request.params);

    if let (Some(header), Some(body)) = (header_version, body_version) {
        if header != body {
            return Err(header_mismatch(
                id.clone(),
                format!(
                    "Header mismatch: {MCP_PROTOCOL_VERSION_HEADER} header value '{header}' does not match params._meta protocolVersion '{body}'"
                ),
            ));
        }
    }

    for requested in [header_version, body_version].into_iter().flatten() {
        if !MCP_SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
            return Err(unsupported_protocol_version(id.clone(), requested));
        }
    }

    let stateless = header_version == Some(MCP_STATELESS_PROTOCOL_VERSION)
        || body_version == Some(MCP_STATELESS_PROTOCOL_VERSION);
    if !stateless {
        return Ok(McpProtocolEra::Legacy);
    }

    if header_version != Some(MCP_STATELESS_PROTOCOL_VERSION) {
        return Err(header_mismatch(
            id,
            format!(
                "Header mismatch: {MCP_PROTOCOL_VERSION_HEADER} is required and must equal {MCP_STATELESS_PROTOCOL_VERSION}"
            ),
        ));
    }
    if body_version != Some(MCP_STATELESS_PROTOCOL_VERSION) {
        return Err(header_mismatch(
            id,
            format!(
                "Header mismatch: {MCP_PROTOCOL_VERSION_HEADER} does not match params._meta protocolVersion"
            ),
        ));
    }
    if request.id.is_some() {
        if !request_client_capabilities(&request.params).is_some_and(Value::is_object) {
            return Err(rpc_error(
                id.clone(),
                -32602,
                "Invalid params: MCP 2026-07-28 requests require params._meta clientCapabilities",
            ));
        }
        if !request_client_info_is_valid(&request.params) {
            return Err(rpc_error(
                id,
                -32602,
                "Invalid params: params._meta clientInfo must contain string name and version fields when present",
            ));
        }
    }

    match request_header(req, MCP_METHOD_HEADER) {
        Some(method) if method == request.method => {}
        Some(method) => {
            return Err(header_mismatch(
                id,
                format!(
                    "Header mismatch: Mcp-Method header value '{method}' does not match body value '{}'",
                    request.method
                ),
            ));
        }
        None => {
            return Err(header_mismatch(
                id,
                "Header mismatch: required Mcp-Method header is missing or malformed",
            ));
        }
    }

    if let Some(body_name) = request_mcp_name(request) {
        let header_name = request_header(req, MCP_NAME_HEADER)
            .and_then(|value| decode_mcp_name_header(value).ok());
        match (header_name.as_deref(), body_name) {
            (Some(header), Some(body)) if header == body => {}
            (Some(header), Some(body)) => {
                return Err(header_mismatch(
                    id,
                    format!(
                        "Header mismatch: Mcp-Name header value '{header}' does not match body value '{body}'"
                    ),
                ));
            }
            _ => {
                return Err(header_mismatch(
                    id,
                    "Header mismatch: required Mcp-Name header is missing, malformed, or has no matching body value",
                ));
            }
        }
    }

    Ok(McpProtocolEra::Stateless2026)
}

use crate::action_audit::{ActionAudit, ActionAuditRecord};
use crate::auth::AuthContext;
use crate::connector_runtime::{ConnectorRuntime, ConnectorRuntimeSlot, ConnectorTransport};
use crate::json_error;
use crate::model_surface::ModelSurface;
use crate::tool_request_trace::{
    estimate_json_bytes, jsonrpc_id_safe, new_trace_id, ToolRequestLifecycle,
};
use crate::tool_runtime::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallErrorStatus,
    ToolCallRequest as KernelToolCallRequest, ToolTransport,
};
use crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES;
use crate::tool_runtime::{registered_tool_specs, ToolResult, ToolRuntime, ToolSpec};
use base64::{engine::general_purpose, Engine as _};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_STATELESS_PROTOCOL_VERSION: &str = "2026-07-28";
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const MCP_METHOD_HEADER: &str = "mcp-method";
const MCP_NAME_HEADER: &str = "mcp-name";
const MCP_HEADER_MISMATCH: i64 = -32020;
const MCP_UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;
const MCP_SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &[MCP_STATELESS_PROTOCOL_VERSION, MCP_PROTOCOL_VERSION];
const MCP_UI_EXTENSION: &str = "io.modelcontextprotocol/ui";
const MCP_COMPUTER_UI_RESOURCE_URI: &str = "ui://webcodex/computer/v4";
const MCP_COMPUTER_UI_RESOURCE_LEGACY_URIS: &[&str] = &[
    "ui://webcodex/computer/v1",
    "ui://webcodex/computer/v2",
    "ui://webcodex/computer/v3",
];
const MCP_COMPUTER_UI_DOMAIN: &str = "https://sg4.yyjeqhc.cn";
const MCP_UI_RESOURCE_MIME_TYPE: &str = "text/html;profile=mcp-app";
const MCP_COMPUTER_APP_HTML: &str = include_str!("mcp_computer_app.html");

/// Single source of truth for the JSON-RPC methods advertised by `GET /mcp`.
/// Must match the dispatch arms in `handle_mcp_request_with_lifecycle`;
/// pinned by `mcp_info_advertised_methods_match_dispatch`.
const MCP_INFO_METHODS: &[&str] = &[
    "server/discover",
    "initialize",
    "ping",
    "tools/list",
    "tools/call",
    "resources/list",
    "resources/read",
    "notifications/initialized",
];
const MCP_RESERVED_SESSION_ID_FIELD: &str = "_session_id";

/// Hard upper bound on a single MCP JSON-RPC dispatch, applied in `mcp_post`.
///
/// Chosen above every per-tool wait (sync agent waits are clamped to
/// `wait_timeout_secs <= 120` plus a few seconds of margin), so it can only
/// fire when a dispatch path hangs without its own bound. Its job is to turn
/// an otherwise-permanently-silent HTTP request into an explicit JSON-RPC
/// error the client can surface.
const MCP_DISPATCH_HARD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(150);

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    jsonrpc: Option<String>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub id: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct McpToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpProtocolEra {
    Legacy,
    Stateless2026,
}

fn runtime(depot: &Depot) -> Option<Arc<ToolRuntime>> {
    depot.obtain::<Arc<ToolRuntime>>().ok().cloned()
}

fn connector_runtime_slot(depot: &Depot) -> Option<ConnectorRuntimeSlot> {
    depot.obtain::<ConnectorRuntimeSlot>().ok().cloned()
}

fn validate_model_surface_state(
    model_surface: ModelSurface,
    connector_present: bool,
) -> Result<(), String> {
    match (model_surface, connector_present) {
        (ModelSurface::CanonicalConnector, true)
        | (ModelSurface::LocalCoding, false)
        | (ModelSurface::FullOperatorRuntime, false) => Ok(()),
        (ModelSurface::CanonicalConnector, false) => Err(
            "canonical_connector surface selected but Connector runtime state is missing"
                .to_string(),
        ),
        (ModelSurface::LocalCoding, true) | (ModelSurface::FullOperatorRuntime, true) => {
            Err(format!(
                "{} surface selected but Connector runtime state is present",
                model_surface.name()
            ))
        }
    }
}

fn tool_name_from_params(params: &Value) -> Option<String> {
    params
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn project_from_tool_call_params(params: &Value) -> Option<String> {
    params["arguments"]["project"].as_str().map(str::to_string)
}

fn request_protocol_version(params: &Value) -> Option<&str> {
    params
        .get("_meta")
        .and_then(|meta| meta.get("io.modelcontextprotocol/protocolVersion"))
        .and_then(Value::as_str)
}

fn request_client_capabilities(params: &Value) -> Option<&Value> {
    params
        .get("_meta")
        .and_then(|meta| meta.get("io.modelcontextprotocol/clientCapabilities"))
}

fn request_supports_mcp_apps(params: &Value) -> bool {
    let Some(extension) = request_client_capabilities(params)
        .and_then(|capabilities| capabilities.get("extensions"))
        .and_then(|extensions| extensions.get(MCP_UI_EXTENSION))
        .and_then(Value::as_object)
    else {
        return false;
    };
    match extension.get("mimeTypes").and_then(Value::as_array) {
        Some(mime_types) => mime_types
            .iter()
            .any(|mime| mime.as_str() == Some(MCP_UI_RESOURCE_MIME_TYPE)),
        None => false,
    }
}

fn model_surface_supports_computer_app(model_surface: ModelSurface) -> bool {
    model_surface == ModelSurface::FullOperatorRuntime
}

fn request_client_info_is_valid(params: &Value) -> bool {
    let Some(meta) = params.get("_meta").and_then(Value::as_object) else {
        return true;
    };
    let Some(client_info) = meta.get("io.modelcontextprotocol/clientInfo") else {
        return true;
    };
    let Some(client_info) = client_info.as_object() else {
        return false;
    };
    client_info.get("name").is_some_and(Value::is_string)
        && client_info.get("version").is_some_and(Value::is_string)
}

fn request_header<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
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

#[cfg(test)]
fn inferred_protocol_era(request: &JsonRpcRequest) -> McpProtocolEra {
    if request_protocol_version(&request.params) == Some(MCP_STATELESS_PROTOCOL_VERSION) {
        McpProtocolEra::Stateless2026
    } else {
        McpProtocolEra::Legacy
    }
}

/// Validate the HTTP-only request metadata introduced by MCP 2026-07-28.
/// Requests with no modern markers retain the existing 2025-06-18 behavior.
fn validate_http_protocol(
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

fn mcp_stateless_result(mut result: Value, cacheable: bool) -> Value {
    let Some(object) = result.as_object_mut() else {
        return result;
    };
    object
        .entry("resultType".to_string())
        .or_insert_with(|| Value::String("complete".to_string()));
    if cacheable {
        object
            .entry("ttlMs".to_string())
            .or_insert_with(|| Value::from(0));
        object
            .entry("cacheScope".to_string())
            .or_insert_with(|| Value::String("private".to_string()));
    }
    let meta = object
        .entry("_meta".to_string())
        .or_insert_with(|| json!({}));
    if let Some(meta_object) = meta.as_object_mut() {
        meta_object
            .entry("io.modelcontextprotocol/serverInfo".to_string())
            .or_insert_with(|| {
                json!({
                    "name": "webcodex",
                    "version": env!("CARGO_PKG_VERSION")
                })
            });
    }
    result
}

/// MCP tools/list payload for the immutable startup-selected model surface.
///
/// Env adapter: resolves the `WEBCODEX_MCP_COMPACT_SCHEMAS` switch and
/// delegates to the pure renderer.
fn mcp_tools_list_payload(model_surface: ModelSurface) -> Value {
    mcp_tools_list_payload_with_compact(model_surface, crate::config::mcp_compact_schemas_enabled())
}

/// Pure tools/list rendering with an explicit compact switch; no env access.
/// Production resolves the switch from the env adapter above; tests pass an
/// explicit bool so they never need process-global env. The schema shape is
/// identical to the adapter path: `compact` only omits `outputSchema`.
fn mcp_tools_list_payload_with_compact(model_surface: ModelSurface, compact: bool) -> Value {
    mcp_tools_list_payload_with_compact_and_app(model_surface, compact, false)
}

fn mcp_tools_list_payload_with_compact_and_app(
    model_surface: ModelSurface,
    compact: bool,
    app_enabled: bool,
) -> Value {
    let specs = match model_surface {
        ModelSurface::CanonicalConnector => crate::connector_runtime::surface::capability_specs(),
        ModelSurface::LocalCoding => crate::model_surface::local_coding_tool_specs(),
        ModelSurface::FullOperatorRuntime => registered_tool_specs(),
    };
    let tools: Vec<Value> = specs
        .into_iter()
        .map(|spec| mcp_tool_spec_json(spec, compact, app_enabled))
        .collect();
    json!({ "tools": tools })
}

fn mcp_tool_spec_json(mut spec: ToolSpec, compact: bool, app_enabled: bool) -> Value {
    let tool_name = spec.name.clone();
    if tool_name == "read_project_artifact" {
        if let Some(properties) = spec.input_schema["properties"].as_object_mut() {
            properties.insert(
                "as_image".to_string(),
                json!({
                    "type": "boolean",
                    "description": "MCP-only. When true, read one complete PNG, JPEG, or WebP up to 1 MiB and return it as native image content. Cannot be combined with offset, length, or max_bytes."
                }),
            );
        }
        spec.description.push_str(
            " Over MCP, set as_image=true to return one complete PNG, JPEG, or WebP as native image content; ordinary calls keep the existing chunked base64 response.",
        );
    }
    let mut value = if compact {
        json!({
            "name": spec.name,
            "description": spec.description,
            "inputSchema": spec.input_schema,
            "annotations": spec.annotations,
        })
    } else {
        // Match ToolSpec's camelCase serde so default behavior is unchanged.
        serde_json::to_value(spec).unwrap_or_else(|_| json!({}))
    };
    if tool_name == "import_conversation_files_to_project" {
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "_meta".to_string(),
                json!({"openai/fileParams": ["openaiFileIdRefs"]}),
            );
        }
    }
    if app_enabled && tool_name == "computer_snapshot" {
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "_meta".to_string(),
                json!({
                    "ui": {
                        "resourceUri": MCP_COMPUTER_UI_RESOURCE_URI,
                        "visibility": ["model", "app"]
                    },
                    "ui/resourceUri": MCP_COMPUTER_UI_RESOURCE_URI,
                    "openai/outputTemplate": MCP_COMPUTER_UI_RESOURCE_URI
                }),
            );
        }
    }
    value
}

fn mcp_computer_app_resource_meta() -> Value {
    json!({
        "ui": {
            "prefersBorder": true,
            "domain": MCP_COMPUTER_UI_DOMAIN,
            "csp": {
                "connectDomains": [],
                "resourceDomains": []
            }
        },
        "openai/widgetDomain": MCP_COMPUTER_UI_DOMAIN
    })
}

fn mcp_computer_app_resources_list() -> Value {
    json!({
        "resources": [{
            "uri": MCP_COMPUTER_UI_RESOURCE_URI,
            "name": "WebCodex Computer",
            "description": "Read-only WebCodex Computer screenshot card with optional in-card refresh and user-initiated chat continuation.",
            "mimeType": MCP_UI_RESOURCE_MIME_TYPE,
            "_meta": mcp_computer_app_resource_meta()
        }]
    })
}

fn mcp_computer_app_resource_read(uri: &str) -> Option<Value> {
    // ChatGPT can retain an older tool descriptor across connector refreshes.
    // Keep prior computer App URIs as hidden read aliases so an already-bound
    // card can fetch the current safe template. resources/list and tools/list
    // still advertise only the canonical URI above.
    let supported =
        uri == MCP_COMPUTER_UI_RESOURCE_URI || MCP_COMPUTER_UI_RESOURCE_LEGACY_URIS.contains(&uri);
    supported.then(|| {
        json!({
            "contents": [{
                "uri": uri,
                "mimeType": MCP_UI_RESOURCE_MIME_TYPE,
                "text": MCP_COMPUTER_APP_HTML,
                "_meta": mcp_computer_app_resource_meta()
            }]
        })
    })
}

pub(crate) fn mcp_runtime_tool_result(
    tool_name: &str,
    as_image_requested: bool,
    mut result: ToolResult,
) -> Value {
    let native_image_requested = (tool_name == "read_project_artifact" && as_image_requested)
        || tool_name == "computer_snapshot";
    if native_image_requested && result.success {
        match mcp_native_image_tool_result(tool_name, &mut result) {
            Ok(value) => return value,
            Err(error) => {
                result = ToolResult::err(format!(
                    "cannot frame {tool_name} as MCP image content: {error}"
                ));
            }
        }
    }

    let text = serde_json::to_string(&json!({
        "success": result.success,
        "output": result.output.clone(),
        "error": result.error.clone(),
    }))
    .unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ],
        "structuredContent": {
            "success": result.success,
            "output": result.output,
            "error": result.error,
        },
        "isError": !result.success
    })
}

fn mcp_native_image_tool_result(tool_name: &str, result: &mut ToolResult) -> Result<Value, String> {
    let data = result
        .output
        .get("content_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing content_base64".to_string())?
        .to_string();
    let mime_type = result
        .output
        .get("mime_type")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing mime_type".to_string())?
        .to_string();
    if !matches!(
        mime_type.as_str(),
        "image/png" | "image/jpeg" | "image/webp"
    ) {
        return Err(format!("unsupported MIME type '{mime_type}'"));
    }
    let decoded = general_purpose::STANDARD
        .decode(&data)
        .map_err(|error| format!("invalid image base64: {error}"))?;
    if decoded.is_empty() || decoded.len() > crate::artifact_policy::MAX_MCP_IMAGE_BYTES {
        return Err(format!(
            "image payload exceeds {} decoded bytes",
            crate::artifact_policy::MAX_MCP_IMAGE_BYTES
        ));
    }
    let detected = if decoded.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if decoded.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if decoded.len() >= 12 && decoded.starts_with(b"RIFF") && &decoded[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    };
    if detected != Some(mime_type.as_str()) {
        return Err("image MIME does not match decoded content".to_string());
    }
    let image_label = if tool_name == "computer_snapshot" {
        result
            .output
            .pointer("/surface/surface_id")
            .and_then(Value::as_str)
            .unwrap_or("desktop surface")
    } else {
        result
            .output
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("project image")
    };
    let file_bytes = result
        .output
        .get("file_bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| "missing file_bytes".to_string())?;
    if file_bytes != decoded.len() as u64 {
        return Err("file_bytes does not match decoded image payload".to_string());
    }
    let sha256 = result
        .output
        .get("sha256")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let metadata_text = if tool_name == "computer_snapshot" {
        let width = result
            .output
            .get("width")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let height = result
            .output
            .get("height")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err("computer snapshot dimensions are invalid".to_string());
        }
        format!("Image {image_label}: {mime_type}, {width}x{height}, {file_bytes} bytes.")
    } else {
        format!("Image {image_label}: {mime_type}, {file_bytes} bytes, sha256 {sha256}.")
    };

    let output = result
        .output
        .as_object_mut()
        .ok_or_else(|| "tool output is not an object".to_string())?;
    output.remove("content_base64");
    output.insert("content_delivery".to_string(), json!("mcp_image"));
    let structured_output = result.output.clone();

    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": metadata_text
            },
            {
                "type": "image",
                "data": data,
                "mimeType": mime_type
            }
        ],
        "structuredContent": {
            "success": true,
            "output": structured_output,
            "error": Value::Null,
        },
        "isError": false
    }))
}

/// Outcome of handling a single MCP JSON-RPC request.
///
/// Carries the JSON-RPC response body alongside the HTTP status the HTTP
/// wrapper should render. Keeping this separate from `Response` makes the
/// core protocol logic testable without a live server.
#[derive(Debug)]
enum McpOutcome {
    /// A normal JSON-RPC result. HTTP 200 with the body.
    Ok(Value),
    /// A JSON-RPC protocol error. HTTP 400 with the error body.
    BadRequest(Value),
    /// A modern MCP method is not implemented. HTTP 404 with JSON-RPC -32601.
    NotFound(Value),
    /// A JSON-RPC notification (request without an `id` member). Per the
    /// JSON-RPC 2.0 and MCP specs the server MUST NOT reply with a
    /// JSON-RPC response body. The HTTP wrapper acknowledges with 202 and
    /// an empty body.
    Notification,
    /// The HTTP request authenticated, but the OAuth2 bearer token lacks the
    /// delegated scope needed by this JSON-RPC method or tool.
    Forbidden {
        body: Value,
        required_scope: Option<&'static str>,
    },
}

#[handler]
pub async fn mcp_info(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    if let Err((status, _, message)) = crate::auth::require_same_origin(req) {
        let status = StatusCode::from_u16(status).unwrap_or(StatusCode::FORBIDDEN);
        res.status_code(status);
        res.render(json_error(status, message));
        return;
    }
    if request_header(req, MCP_PROTOCOL_VERSION_HEADER) == Some(MCP_STATELESS_PROTOCOL_VERSION) {
        res.status_code(StatusCode::METHOD_NOT_ALLOWED);
        return;
    }
    let auth_required = crate::auth::get_config(depot)
        .map(|c| c.is_auth_enabled())
        .unwrap_or(false);
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let Some(connector_slot) = connector_runtime_slot(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "MCP model surface state not configured",
        ));
        return;
    };
    let model_surface = runtime.model_surface();
    if let Err(error) = validate_model_surface_state(model_surface, connector_slot.0.is_some()) {
        tracing::error!(%error, "MCP model surface state mismatch");
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, error));
        return;
    }
    res.render(Json(json!({
        "name": "webcodex",
        "version": env!("CARGO_PKG_VERSION"),
        "modelSurface": model_surface.name(),
        "protocol": "mcp",
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "transport": "streamable-http-jsonrpc",
        "endpoint": "/mcp",
        "methods": MCP_INFO_METHODS,
        "auth": {
            "type": "bearer",
            "required": auth_required,
            "header": "Authorization: Bearer <shared_key_or_wc_pat>"
        }
    })));
}

#[handler]
pub async fn mcp_post(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let mut guard = ToolRequestLifecycle::new("mcp", new_trace_id(), "-", "POST /mcp", None);
    guard.received();

    if let Err((status, _, message)) = crate::auth::require_json_same_origin(req) {
        let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST);
        guard.parsed("http_validation_error");
        guard.response_serialized(
            status.as_u16(),
            None,
            Some(false),
            None,
            "http_validation_error",
        );
        res.status_code(status);
        res.render(json_error(status, message));
        guard.handler_returned(
            status.as_u16(),
            None,
            Some(false),
            None,
            "http_validation_error",
        );
        return;
    }

    let Some(runtime) = runtime(depot) else {
        // Size unknown without building the json_error body for measurement.
        guard.response_serialized(500, None, Some(false), None, "error_runtime_missing");
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        guard.handler_returned(500, None, Some(false), None, "error_runtime_missing");
        return;
    };
    let Some(connector_slot) = connector_runtime_slot(depot) else {
        guard.response_serialized(500, None, Some(false), None, "error_surface_state_missing");
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "MCP model surface state not configured",
        ));
        guard.handler_returned(500, None, Some(false), None, "error_surface_state_missing");
        return;
    };
    let connector = connector_slot.0;
    if let Err(error) = validate_model_surface_state(runtime.model_surface(), connector.is_some()) {
        tracing::error!(%error, "MCP model surface state mismatch");
        guard.response_serialized(500, None, Some(false), None, "error_surface_state_mismatch");
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, error));
        guard.handler_returned(500, None, Some(false), None, "error_surface_state_mismatch");
        return;
    }
    let request: JsonRpcRequest = match req.parse_json().await {
        Ok(request) => request,
        Err(e) => {
            guard.set_jsonrpc_id("none");
            guard.parsed("parse_error");
            let body = rpc_error(None, -32700, format!("Parse error: {}", e));
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(400, estimated, Some(false), None, "parse_error");
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(body));
            guard.handler_returned(400, estimated, Some(false), None, "parse_error");
            return;
        }
    };

    guard.set_jsonrpc_id(jsonrpc_id_safe(request.id.as_ref()));
    guard.set_method(request.method.clone());
    let tool_name = if request.method == "tools/call" {
        tool_name_from_params(&request.params)
    } else {
        None
    };
    guard.set_tool_name(tool_name.clone());
    let protocol_era = match validate_http_protocol(req, &request) {
        Ok(protocol_era) => protocol_era,
        Err(body) => {
            guard.parsed("protocol_error");
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(400, estimated, Some(false), None, "protocol_error");
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(body));
            guard.handler_returned(400, estimated, Some(false), None, "protocol_error");
            return;
        }
    };
    guard.parsed("ok");
    let window = match protocol_era {
        McpProtocolEra::Legacy => {
            crate::client_window::mcp_window(req, request.method == "initialize")
        }
        McpProtocolEra::Stateless2026 => crate::client_window::McpWindow {
            identity: None,
            issued_session_id: None,
        },
    };

    // Chat-window MCP tool calls must land in the action audit exactly like
    // the REST surface (they were previously invisible there). Summary-level
    // only: tool name and project — never arguments or outputs. JSON-RPC
    // notifications are acknowledged but never dispatched, so they must not be
    // represented as executed actions.
    let audit = if request.method == "tools/call" && request.id.is_some() {
        Some((
            ActionAudit::start(req, depot, "/mcp", "toolsCall"),
            tool_name.clone().unwrap_or_else(|| "unknown".to_string()),
            project_from_tool_call_params(&request.params),
        ))
    } else {
        None
    };
    let record_audit = |success: bool, status: StatusCode, error: Option<String>| {
        if let Some((audit, tool, project)) = audit.as_ref() {
            let mut event = ActionAuditRecord::new(tool.clone(), success, status)
                .error(error)
                .summary(json!({ "transport": "mcp" }));
            event.project = project.clone();
            audit.record(event);
        }
    };

    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let host_file_import_trust =
        if tool_name.as_deref() == Some("import_conversation_files_to_project") {
            mcp_host_file_import_trust(depot, auth.as_ref())
        } else {
            HostFileImportTrust::Untrusted
        };
    // Defense-in-depth backstop: every tool bounds its own agent/subprocess
    // waits at <= 124s, so this outer limit never preempts a legitimate inner
    // timeout. It only fires if a dispatch path hangs without a bound (the
    // failure mode behind "MCP request never gets a reply"), converting a
    // silently dead HTTP request into an observable JSON-RPC error.
    let request_id = request.id.clone();
    let outcome = match tokio::time::timeout(
        MCP_DISPATCH_HARD_TIMEOUT,
        handle_mcp_request_with_lifecycle(
            &runtime,
            connector.as_deref(),
            request,
            auth.as_ref(),
            protocol_era,
            host_file_import_trust,
            window.identity.as_ref(),
            Some(&mut guard),
        ),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => {
            let body = rpc_error(
                request_id,
                -32000,
                format!(
                    "server-side dispatch exceeded {}s hard limit; the tool may still be running — check session/job status before retrying",
                    MCP_DISPATCH_HARD_TIMEOUT.as_secs()
                ),
            );
            record_audit(
                false,
                StatusCode::INTERNAL_SERVER_ERROR,
                Some("mcp dispatch hard timeout".to_string()),
            );
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(500, estimated, Some(false), None, "dispatch_hard_timeout");
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(body));
            guard.handler_returned(500, estimated, Some(false), None, "dispatch_hard_timeout");
            return;
        }
    };

    if matches!(outcome, McpOutcome::Ok(_)) {
        if let Some(session_id) = window.issued_session_id.as_deref() {
            crate::client_window::set_mcp_session_header(res, session_id);
        }
    }

    match outcome {
        McpOutcome::Ok(body) => {
            // Protocol success: valid JSON-RPC result envelope.
            // Tool success: only meaningful for tools/call (isError / structuredContent.success).
            let tool_success = body
                .get("result")
                .and_then(|r| r.get("structuredContent"))
                .and_then(|s| s.get("success").or_else(|| s.get("ok")))
                .and_then(|v| v.as_bool());
            let audit_success = tool_success.unwrap_or(true);
            record_audit(
                audit_success,
                StatusCode::OK,
                if audit_success {
                    None
                } else {
                    body["result"]["structuredContent"]["error"]
                        .as_str()
                        .map(str::to_string)
                },
            );
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(200, estimated, Some(true), tool_success, "ok");
            res.render(Json(body));
            guard.handler_returned(200, estimated, Some(true), tool_success, "ok");
        }
        McpOutcome::BadRequest(body) => {
            record_audit(
                false,
                StatusCode::BAD_REQUEST,
                body["error"]["message"].as_str().map(str::to_string),
            );
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(400, estimated, Some(false), None, "bad_request");
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(body));
            guard.handler_returned(400, estimated, Some(false), None, "bad_request");
        }
        McpOutcome::NotFound(body) => {
            record_audit(
                false,
                StatusCode::NOT_FOUND,
                body["error"]["message"].as_str().map(str::to_string),
            );
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(404, estimated, Some(false), None, "not_found");
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(body));
            guard.handler_returned(404, estimated, Some(false), None, "not_found");
        }
        McpOutcome::Forbidden {
            body,
            required_scope,
        } => {
            record_audit(
                false,
                StatusCode::FORBIDDEN,
                Some(format!(
                    "insufficient scope: {}",
                    required_scope.unwrap_or("unknown")
                )),
            );
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(403, estimated, Some(false), None, "forbidden");
            res.status_code(StatusCode::FORBIDDEN);
            let challenge = crate::auth::oauth_insufficient_scope_challenge(required_scope);
            if let Ok(val) = salvo::http::HeaderValue::from_str(&challenge) {
                res.headers_mut().insert("www-authenticate", val);
            }
            res.render(Json(body));
            guard.handler_returned(403, estimated, Some(false), None, "forbidden");
        }
        McpOutcome::Notification => {
            // JSON-RPC notifications carry no `id`; the server MUST NOT reply
            // with a JSON-RPC body. Acknowledge with 202 and an empty body.
            // Empty body size is known (0) without JSON serialization.
            guard.response_serialized(202, Some(0), Some(true), None, "notification");
            res.status_code(StatusCode::ACCEPTED);
            guard.handler_returned(202, Some(0), Some(true), None, "notification");
        }
    }
}

/// Core MCP JSON-RPC dispatch. Pure (no HTTP types) so it can be unit tested.
///
/// Business logic stays in `ToolRuntime`; this function only frames the
/// JSON-RPC envelope and translates tool results into MCP content blocks.
/// Test-friendly wrapper: no lifecycle hooks.
#[cfg(test)]
async fn handle_mcp_request(
    runtime: &ToolRuntime,
    request: JsonRpcRequest,
    auth: Option<&AuthContext>,
) -> McpOutcome {
    let protocol_era = inferred_protocol_era(&request);
    handle_mcp_request_with_lifecycle(
        runtime,
        None,
        request,
        auth,
        protocol_era,
        HostFileImportTrust::Untrusted,
        None,
        None,
    )
    .await
}

async fn handle_mcp_request_with_lifecycle(
    runtime: &ToolRuntime,
    connector: Option<&ConnectorRuntime>,
    request: JsonRpcRequest,
    auth: Option<&AuthContext>,
    protocol_era: McpProtocolEra,
    host_file_import_trust: HostFileImportTrust,
    window: Option<&crate::client_window::ClientWindow>,
    mut lifecycle: Option<&mut ToolRequestLifecycle>,
) -> McpOutcome {
    let is_oauth2 = auth.is_some_and(|ctx| ctx.is_oauth_token());
    let stateless_2026 = protocol_era == McpProtocolEra::Stateless2026;
    let mcp_app_enabled = stateless_2026
        && model_surface_supports_computer_app(runtime.model_surface())
        && request_supports_mcp_apps(&request.params);

    if is_oauth2
        && (matches!(
            request.method.as_str(),
            "server/discover" | "tools/list" | "resources/list" | "resources/read"
        ) || (!stateless_2026
            && matches!(
                request.method.as_str(),
                "initialize" | "ping" | "notifications/initialized"
            )))
    {
        if let Some(outcome) = require_mcp_oauth_scope(auth, crate::auth::SCOPE_RUNTIME_READ) {
            return outcome;
        }
    }

    if is_oauth2
        && !stateless_2026
        && !matches!(
            request.method.as_str(),
            "server/discover"
                | "initialize"
                | "ping"
                | "tools/list"
                | "tools/call"
                | "notifications/initialized"
        )
    {
        return oauth_forbidden(None, "OAuth2 access tokens cannot call unknown MCP methods");
    }

    // A JSON-RPC request without an `id` member is a notification. Per the
    // JSON-RPC 2.0 and MCP specs the server MUST NOT reply with a JSON-RPC
    // response body, even if the method is unknown or malformed. We accept
    // the notification silently. `notifications/initialized` is the common
    // case sent by MCP clients after `initialize` completes.
    if request.id.is_none() {
        return McpOutcome::Notification;
    }

    let jsonrpc_valid = if stateless_2026 {
        request.jsonrpc.as_deref() == Some("2.0")
    } else {
        request.jsonrpc.as_deref().unwrap_or("2.0") == "2.0"
    };
    if !jsonrpc_valid {
        return McpOutcome::BadRequest(rpc_error(request.id, -32600, "jsonrpc must be '2.0'"));
    }

    if let Err(error) = validate_model_surface_state(runtime.model_surface(), connector.is_some()) {
        return McpOutcome::BadRequest(rpc_error(request.id, -32603, error));
    }

    let id = request.id.clone();
    let response = match request.method.as_str() {
        // MCP 2026-07-28 clients discover capabilities before issuing ordinary
        // requests. WebCodex supports the stateless tools path required by
        // modern clients while retaining its existing 2025-06-18
        // initialize/session lifecycle for legacy clients.
        "server/discover" if stateless_2026 => rpc_result(
            id,
            json!({
                "resultType": "complete",
                "ttlMs": 0,
                "cacheScope": "private",
                "supportedVersions": [MCP_STATELESS_PROTOCOL_VERSION, MCP_PROTOCOL_VERSION],
                "capabilities": if model_surface_supports_computer_app(runtime.model_surface()) {
                    json!({
                        "tools": { "listChanged": false },
                        "resources": { "listChanged": false, "subscribe": false },
                        "extensions": {
                            MCP_UI_EXTENSION: {
                                "mimeTypes": [MCP_UI_RESOURCE_MIME_TYPE]
                            }
                        }
                    })
                } else {
                    json!({ "tools": { "listChanged": false } })
                },
                "_meta": {
                    "io.modelcontextprotocol/serverInfo": {
                        "name": "webcodex",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }),
        ),
        "initialize" if !stateless_2026 => rpc_result(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "webcodex",
                    "version": env!("CARGO_PKG_VERSION"),
                    "modelSurface": runtime.model_surface().name()
                }
            }),
        ),
        "ping" if !stateless_2026 => rpc_result(id, json!({})),
        "tools/list" => {
            let result = if stateless_2026 {
                mcp_tools_list_payload_with_compact_and_app(
                    runtime.model_surface(),
                    crate::config::mcp_compact_schemas_enabled(),
                    model_surface_supports_computer_app(runtime.model_surface()),
                )
            } else {
                mcp_tools_list_payload(runtime.model_surface())
            };
            rpc_result(
                id,
                if stateless_2026 {
                    mcp_stateless_result(result, true)
                } else {
                    result
                },
            )
        }
        "resources/list" if stateless_2026 => {
            let result = if mcp_app_enabled {
                mcp_computer_app_resources_list()
            } else {
                json!({ "resources": [] })
            };
            rpc_result(id, mcp_stateless_result(result, true))
        }
        "resources/read" if stateless_2026 => {
            // Tool descriptors on the full-operator surface advertise the App
            // resource independently of whether a later resource fetch repeats
            // the UI client-capability metadata. Keep resources/list negotiated,
            // but allow a client to dereference an already-advertised resource.
            if !model_surface_supports_computer_app(runtime.model_surface()) {
                return McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    "MCP App resource is unavailable on this model surface",
                ));
            }
            let Some(uri) = request.params.get("uri").and_then(Value::as_str) else {
                return McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    "Invalid params: uri is required",
                ));
            };
            let Some(result) = mcp_computer_app_resource_read(uri) else {
                return McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    format!("Resource not found: {uri}"),
                ));
            };
            rpc_result(id, mcp_stateless_result(result, true))
        }
        "tools/call" => {
            let mut params: McpToolCallParams = match serde_json::from_value(request.params) {
                Ok(params) => params,
                Err(e) => {
                    return McpOutcome::BadRequest(rpc_error(
                        id,
                        -32602,
                        format!("Invalid params: {}", e),
                    ));
                }
            };
            // Emit dispatch_started only after params parse succeeds and before
            // ToolRuntime work begins.
            if let Some(lc) = lifecycle.as_deref_mut() {
                lc.set_tool_name(Some(params.name.clone()));
                lc.dispatch_started();
            }
            // The local_coding model surface rejects tools it does not
            // advertise at the MCP boundary, before ToolRuntime dispatch. The
            // full operator runtime and the canonical Connector keep their
            // existing behavior unchanged.
            if runtime.model_surface() == ModelSurface::LocalCoding
                && !LOCAL_CODING_TOOL_NAMES.contains(&params.name.as_str())
            {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("surface_denied");
                    lc.dispatch_finished(false, Some(false), "surface_denied");
                }
                return McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    format!(
                        "tool '{}' is not available on the local_coding MCP surface; the full operator runtime must be selected explicitly with WEBCODEX_MCP_MODEL_SURFACE=full-operator-v1",
                        params.name
                    ),
                ));
            }
            if runtime.model_surface() == ModelSurface::CanonicalConnector {
                let connector = connector.expect("validated canonical Connector state");
                if !stateless_2026 && params.name == "task_start" && window.is_none() {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("window_identity_unavailable");
                        lc.dispatch_finished(false, Some(false), "window_identity_unavailable");
                    }
                    return McpOutcome::BadRequest(rpc_error(
                        id,
                        -32600,
                        "MCP session identity is unavailable; initialize the connection before starting or continuing project work",
                    ));
                }
                let outcome = connector
                    .call_for_window(
                        &params.name,
                        params.arguments,
                        auth,
                        ConnectorTransport::Mcp,
                        window,
                    )
                    .await;
                if let Some(required_scope) = outcome.required_scope {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("forbidden");
                        lc.dispatch_finished(false, Some(false), "forbidden");
                    }
                    let description = outcome
                        .body
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("connector credential lacks the required scope")
                        .to_string();
                    return oauth_forbidden(Some(required_scope), description);
                }
                if outcome.protocol_error {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("invalid_arguments");
                        lc.dispatch_finished(false, Some(false), "invalid_arguments");
                    }
                    let message = outcome
                        .body
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("invalid connector capability arguments")
                        .to_string();
                    return McpOutcome::BadRequest(rpc_error(id, -32602, message));
                }
                if let Some(lc) = lifecycle.as_deref() {
                    let category = if outcome.ok { "success" } else { "tool_error" };
                    lc.dispatch_finished(true, Some(outcome.ok), category);
                }
                let text =
                    serde_json::to_string(&outcome.body).unwrap_or_else(|_| "{}".to_string());
                let result = json!({
                    "content": [{ "type": "text", "text": text }],
                    "structuredContent": outcome.body,
                    "isError": !outcome.ok
                });
                return McpOutcome::Ok(rpc_result(
                    id,
                    if stateless_2026 {
                        mcp_stateless_result(result, false)
                    } else {
                        result
                    },
                ));
            }
            let session_id = strip_reserved_session_id(&mut params.arguments);
            let as_image_requested = params.name == "read_project_artifact"
                && params.arguments.get("as_image").and_then(Value::as_bool) == Some(true);
            let outcome = runtime
                .call_tool_with_context(
                    KernelToolCallRequest {
                        tool_name: params.name.clone(),
                        arguments: params.arguments,
                    },
                    ToolCallContext {
                        transport: ToolTransport::Mcp,
                        session_id: session_id.as_deref(),
                        auth,
                        window,
                        record_oauth_scope_denials: false,
                        host_file_import_trust,
                    },
                )
                .await;
            let result = match outcome.error_status {
                Some(ToolCallErrorStatus::InsufficientScope {
                    required_scope,
                    description,
                }) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("forbidden");
                        lc.dispatch_finished(false, Some(false), "forbidden");
                    }
                    return oauth_forbidden(required_scope, description);
                }
                Some(ToolCallErrorStatus::InvalidArguments { message }) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("invalid_arguments");
                        lc.dispatch_finished(false, Some(false), "invalid_arguments");
                    }
                    return McpOutcome::BadRequest(rpc_error(id, -32602, message));
                }
                None => outcome
                    .result
                    .expect("tool kernel outcome without error must include result"),
            };
            debug_assert_eq!(outcome.success, result.success);
            if let Some(lc) = lifecycle.as_deref() {
                // Protocol layer produced a JSON-RPC result (not -32xxx).
                // Tool kernel success is independent (isError / structuredContent).
                let category = if result.success {
                    "success"
                } else {
                    "tool_error"
                };
                if result.success {
                    lc.dispatch_finished(true, Some(true), category);
                } else {
                    lc.dispatch_finished(true, Some(false), category);
                }
            }
            let result = mcp_runtime_tool_result(&params.name, as_image_requested, result);
            rpc_result(
                id,
                if stateless_2026 {
                    mcp_stateless_result(result, false)
                } else {
                    result
                },
            )
        }
        "notifications/initialized" if !stateless_2026 => rpc_result(id, json!({})),
        _ => {
            let body = rpc_error(id, -32601, format!("Method not found: {}", request.method));
            return if stateless_2026 {
                McpOutcome::NotFound(body)
            } else {
                McpOutcome::BadRequest(body)
            };
        }
    };
    McpOutcome::Ok(response)
}

fn configured_mcp_file_redirect_uri_is_safe(uri: &str) -> bool {
    let Ok(url) = url::Url::parse(uri) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

fn mcp_host_file_import_trust_from_state(
    config: &crate::Config,
    db: &crate::Database,
    auth: Option<&AuthContext>,
) -> HostFileImportTrust {
    let Some(auth) = auth.filter(|auth| auth.is_oauth_token()) else {
        return HostFileImportTrust::Untrusted;
    };
    let Some(client_id) = auth
        .allowed_client_id
        .as_deref()
        .map(str::trim)
        .filter(|client_id| !client_id.is_empty())
    else {
        return HostFileImportTrust::Untrusted;
    };
    if !config.oauth2.enabled {
        return HostFileImportTrust::Untrusted;
    }
    let trusted_redirects = config
        .oauth2
        .trusted_mcp_file_redirect_uris
        .iter()
        .map(String::as_str)
        .filter(|uri| configured_mcp_file_redirect_uri_is_safe(uri))
        .collect::<std::collections::HashSet<_>>();
    if trusted_redirects.is_empty() {
        return HostFileImportTrust::Untrusted;
    }
    let Ok(Some(client)) = db.get_oauth_client_by_client_id(client_id) else {
        // Active lookup intentionally excludes revoked clients.
        return HostFileImportTrust::Untrusted;
    };
    let registered_redirects = client.redirect_uris_vec();
    if registered_redirects.is_empty()
        || !registered_redirects
            .iter()
            .all(|uri| trusted_redirects.contains(uri.as_str()))
    {
        return HostFileImportTrust::Untrusted;
    }
    // The operator allowlist is the trust anchor, but a second active client
    // must not gain the same trust merely by registering the same callback URI.
    // Shared active registrations are ambiguous and fail closed.
    if registered_redirects.iter().any(|uri| {
        db.count_active_oauth_clients_with_redirect_uri(uri)
            .map(|count| count != 1)
            .unwrap_or(true)
    }) {
        return HostFileImportTrust::Untrusted;
    }
    HostFileImportTrust::TrustedOAuthClient
}

fn mcp_host_file_import_trust(depot: &Depot, auth: Option<&AuthContext>) -> HostFileImportTrust {
    let Some(config) = crate::auth::get_config(depot) else {
        return HostFileImportTrust::Untrusted;
    };
    let Some(db) = crate::auth::get_db(depot) else {
        return HostFileImportTrust::Untrusted;
    };
    mcp_host_file_import_trust_from_state(config.as_ref(), db.as_ref(), auth)
}

fn require_mcp_oauth_scope(auth: Option<&AuthContext>, scope: &'static str) -> Option<McpOutcome> {
    let auth = auth?;
    if !auth.is_oauth_token() || auth.has_scope(scope) {
        return None;
    }
    Some(oauth_forbidden(
        Some(scope),
        format!("missing required scope: {}", scope),
    ))
}

fn strip_reserved_session_id(arguments: &mut Value) -> Option<String> {
    arguments
        .as_object_mut()
        .and_then(|obj| obj.remove(MCP_RESERVED_SESSION_ID_FIELD))
        .and_then(|value| value.as_str().map(str::to_string))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn oauth_forbidden(
    required_scope: Option<&'static str>,
    description: impl Into<String>,
) -> McpOutcome {
    McpOutcome::Forbidden {
        body: crate::auth::oauth_insufficient_scope_body(description),
        required_scope,
    }
}

fn rpc_result(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

fn rpc_error(id: Option<Value>, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}

fn rpc_error_with_data(
    id: Option<Value>,
    code: i64,
    message: impl Into<String>,
    data: Value,
) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message.into(),
            "data": data,
        }
    })
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;

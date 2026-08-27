mod resources;
mod tasks;

use crate::action_audit::{ActionAudit, ActionAuditRecord};
use crate::auth::AuthContext;
use crate::connector_runtime::{ConnectorRuntime, ConnectorRuntimeSlot, ConnectorTransport};
use crate::json_error;
use crate::model_surface::ModelSurface;
use crate::tool_request_trace::{
    estimate_json_bytes, jsonrpc_id_safe, new_trace_id, scope_active_trace, ToolRequestLifecycle,
};
use crate::tool_runtime::kernel::{
    check_runtime_tool_scope, HostFileImportTrust, ToolCallContext, ToolCallErrorStatus,
    ToolCallRequest as KernelToolCallRequest, ToolTransport,
};
use crate::tool_runtime::model_ergonomics_telemetry::{
    ModelErgonomicsRecord, ModelErgonomicsTimer,
};
use crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES;
#[cfg(test)]
use crate::tool_runtime::MAX_PROJECT_ARTIFACT_BYTES;
use crate::tool_runtime::{registered_tool_specs, ToolResult, ToolRuntime, ToolSpec};
#[cfg(test)]
use crate::tool_runtime::{
    validate_project_artifact_export_snapshot, ProjectArtifactExportSnapshot,
    MAX_PROJECT_ARTIFACT_EXPORT_BYTES, MAX_READ_PROJECT_ARTIFACT_LENGTH,
};
use base64::engine::general_purpose;
#[cfg(test)]
use base64::Engine as _;
use futures_util::stream;
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
#[cfg(test)]
use std::time::{Duration, Instant};
#[cfg(test)]
use tokio::sync::Semaphore;

#[cfg(test)]
use resources::*;
#[cfg(test)]
use tasks::{
    mcp_create_task_result, request_supports_tasks, MCP_MISSING_REQUIRED_CLIENT_CAPABILITY,
    MCP_TASKS_EXTENSION,
};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_CHATGPT_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_STATELESS_PROTOCOL_VERSION: &str = "2026-07-28";
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const MCP_METHOD_HEADER: &str = "mcp-method";
const MCP_NAME_HEADER: &str = "mcp-name";
const MCP_HEADER_MISMATCH: i64 = -32020;
const MCP_UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;
const MCP_SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[
    MCP_STATELESS_PROTOCOL_VERSION,
    MCP_CHATGPT_PROTOCOL_VERSION,
    MCP_PROTOCOL_VERSION,
];
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

#[cfg(test)]
fn inferred_protocol_era(request: &JsonRpcRequest) -> McpProtocolEra {
    if request_protocol_version(&request.params) == Some(MCP_STATELESS_PROTOCOL_VERSION) {
        McpProtocolEra::Stateless2026
    } else {
        McpProtocolEra::Legacy
    }
}

fn legacy_initialize_protocol_version(params: &Value) -> &'static str {
    match params.get("protocolVersion").and_then(Value::as_str) {
        Some(MCP_CHATGPT_PROTOCOL_VERSION) => MCP_CHATGPT_PROTOCOL_VERSION,
        Some(MCP_PROTOCOL_VERSION) => MCP_PROTOCOL_VERSION,
        _ => MCP_PROTOCOL_VERSION,
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

fn connector_call_tool_result(outcome: crate::connector_runtime::ConnectorCallOutcome) -> Value {
    let text = serde_json::to_string(&outcome.body).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": outcome.body,
        "isError": !outcome.ok
    })
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
    mcp_tools_list_payload_with_features(model_surface, compact, false, false)
}

fn mcp_tools_list_payload_with_compact_and_app(
    model_surface: ModelSurface,
    compact: bool,
    app_enabled: bool,
) -> Value {
    mcp_tools_list_payload_with_features(model_surface, compact, app_enabled, true)
}

fn mcp_tools_list_payload_with_features(
    model_surface: ModelSurface,
    compact: bool,
    app_enabled: bool,
    artifact_export_enabled: bool,
) -> Value {
    mcp_tools_list_payload_with_features_for_auth(
        model_surface,
        compact,
        app_enabled,
        artifact_export_enabled,
        None,
    )
}

fn mcp_tools_list_payload_with_features_for_auth(
    model_surface: ModelSurface,
    compact: bool,
    app_enabled: bool,
    artifact_export_enabled: bool,
    auth: Option<&AuthContext>,
) -> Value {
    let specs = match model_surface {
        ModelSurface::CanonicalConnector => crate::connector_runtime::surface::capability_specs(),
        ModelSurface::LocalCoding => crate::model_surface::local_coding_tool_specs(),
        ModelSurface::FullOperatorRuntime => registered_tool_specs(),
    };
    let oauth_scope_projection = auth.is_some_and(AuthContext::is_oauth_token);
    let tools: Vec<Value> = specs
        .into_iter()
        .filter(|spec| artifact_export_enabled || spec.name != "export_project_artifact")
        .filter(|spec| {
            !oauth_scope_projection || check_runtime_tool_scope(auth, &spec.name).is_ok()
        })
        .map(|spec| mcp_tool_spec_json(spec, compact, app_enabled))
        .collect();
    json!({ "tools": tools })
}

fn adapt_computer_snapshot_output_schema_for_mcp(spec: &mut ToolSpec) {
    let properties = spec
        .output_schema
        .pointer_mut("/properties/output/properties")
        .and_then(Value::as_object_mut)
        .expect("computer_snapshot output schema properties");
    properties.remove("content_base64");
    properties.insert(
        "content_delivery".to_string(),
        json!({
            "type": "string",
            "const": "mcp_image",
            "description": "MCP native-image delivery marker; binary image bytes are carried in the image ContentBlock rather than structuredContent."
        }),
    );
}

fn add_stateless_workflow_recorder_metadata(payload: &mut Value, model_surface: ModelSurface) {
    if matches!(model_surface, ModelSurface::CanonicalConnector) {
        return;
    }
    let Some(tools) = payload.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };
    for tool in tools {
        let Some(properties) = tool
            .pointer_mut("/inputSchema/properties")
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        properties.insert(
            crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD.to_string(),
            json!({
                "type": "string",
                "pattern": "^wc_sess_[A-Za-z0-9_]+$",
                "description": "MCP wrapper metadata only. Optional explicit existing Workflow Session that records this tools/call and supplies trusted collaboration provenance. It is distinct from any concrete tool business session_id, grants no authority, and is removed before concrete tool parsing."
            }),
        );
        properties.insert(
            crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD.to_string(),
            json!({
                "type": "array",
                "maxItems": crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_MESSAGE_IDS,
                "items": {
                    "type": "string",
                    "pattern": "^wc_msg_[A-Za-z0-9_]+$"
                },
                "description": "MCP wrapper metadata only. ACK means the current model context still remembers the referenced open Session message. Repeat ACK ids on subsequent calls while remembered. If omitted later, unresolved ACK-required guidance may be returned again. ACK does not resolve the message."
            }),
        );
        properties.insert(
            crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD.to_string(),
            json!({
                "type": "object",
                "description": "MCP wrapper metadata only. After one non-todo message in recording_session_id is already handled, resolve it and attach bounded resolution text on this same WebCodex call instead of making a separate resolve call. For requires_ack guidance, include the same message_id in ack_session_message_ids on this request. The target is always the exact recording Session and this object is removed before concrete tool parsing. Do not use it to predict whether the current tool call will succeed; todo completion still uses complete_session_message.",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "pattern": "^wc_msg_[A-Za-z0-9_]+$"
                    },
                    "resolution": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": crate::tool_runtime::sessions::MAX_MESSAGE_RESOLUTION_CHARS
                    }
                },
                "required": ["message_id", "resolution"],
                "additionalProperties": false
            }),
        );
        if matches!(model_surface, ModelSurface::FullOperatorRuntime) {
            properties.insert(
                crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_FIELD
                    .to_string(),
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Echo the latest Session context revision still present in the current model context. Omit it when unknown. If it is missing or behind the Session's model-facing continuity watermark, the tool still executes normally and the result may include bounded Session recovery context."
                }),
            );
        }
    }
}

fn mcp_tool_spec_json(mut spec: ToolSpec, compact: bool, _app_enabled: bool) -> Value {
    let tool_name = spec.name.clone();
    if matches!(
        tool_name.as_str(),
        "computer_snapshot" | "computer_snapshot_display"
    ) {
        adapt_computer_snapshot_output_schema_for_mcp(&mut spec);
    }
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
    value
}

pub(crate) fn mcp_runtime_tool_result(
    tool_name: &str,
    as_image_requested: bool,
    result: ToolResult,
) -> Value {
    resources::mcp_runtime_tool_result_with_snapshot_resource(
        tool_name,
        as_image_requested,
        result,
        None,
    )
}

fn mcp_runtime_tool_result_fallback(result: ToolResult) -> Value {
    let text = serde_json::to_string(&json!({
        "success": result.success,
        "output": result.output.clone(),
        "error": result.error.clone(),
    }))
    .unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": {
            "success": result.success,
            "output": result.output,
            "error": result.error,
        },
        "isError": !result.success
    })
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
    /// A preflighted artifact resource read whose JSON-RPC body is emitted
    /// incrementally by the HTTP wrapper without whole-file aggregation.
    ArtifactExportStream {
        id: Value,
        plan: resources::McpArtifactExportStreamPlan,
    },
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

fn mcp_protocol_era_label(protocol_era: McpProtocolEra) -> &'static str {
    match protocol_era {
        McpProtocolEra::Legacy => "legacy",
        McpProtocolEra::Stateless2026 => "stateless_2026",
    }
}

fn log_mcp_computer_app_resource_delivery(
    uri: &str,
    protocol_era: &str,
    ui_capability_present: bool,
    http_status: u16,
    mcp_error_code: Option<i64>,
) {
    tracing::info!(
        target: "webcodex::mcp",
        uri,
        protocol_era,
        ui_capability_present,
        http_status,
        mcp_error_code = mcp_error_code.unwrap_or(-1),
        "mcp_computer_app_resource_delivery"
    );
}

fn log_mcp_computer_app_resource_outcome(
    uri: &str,
    protocol_era: McpProtocolEra,
    ui_capability_present: bool,
    outcome: &McpOutcome,
) {
    let (http_status, mcp_error_code) = match outcome {
        McpOutcome::Ok(_) | McpOutcome::ArtifactExportStream { .. } => (200, None),
        McpOutcome::BadRequest(body) => (400, body["error"]["code"].as_i64()),
        McpOutcome::NotFound(body) => (404, body["error"]["code"].as_i64()),
        McpOutcome::Notification => (202, None),
        McpOutcome::Forbidden { .. } => (403, None),
    };
    log_mcp_computer_app_resource_delivery(
        uri,
        mcp_protocol_era_label(protocol_era),
        ui_capability_present,
        http_status,
        mcp_error_code,
    );
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
    let computer_app_resource_uri = if request.method == "resources/read" {
        request
            .params
            .get("uri")
            .and_then(Value::as_str)
            .filter(|uri| resources::is_mcp_computer_app_resource_uri(uri))
            .map(str::to_string)
    } else {
        None
    };
    let computer_app_ui_capability_present = resources::request_supports_mcp_apps(&request.params);
    // Computer App resource delivery is part of gray-card diagnosis, but its
    // durable projection must remain metadata-only. Never persist App HTML,
    // screenshot/tool-result content, tool arguments, window titles, or other
    // request payload fields here.
    let computer_app_resource_audit = computer_app_resource_uri.as_ref().and_then(|uri| {
        request.id.as_ref().map(|_| {
            (
                ActionAudit::start(req, depot, "/mcp", "resourcesRead"),
                uri.clone(),
            )
        })
    });
    let record_computer_app_resource_audit =
        |protocol_era: &str, status: StatusCode, mcp_error_code: Option<i64>| {
            if let Some((audit, uri)) = computer_app_resource_audit.as_ref() {
                audit.record(
                    ActionAuditRecord::new(
                        "computer_app_resource_read",
                        status.is_success(),
                        status,
                    )
                    .summary(json!({
                        "transport": "mcp",
                        "resource_uri": uri,
                        "resource_version": uri.rsplit('/').next().unwrap_or("unknown"),
                        "protocol_era": protocol_era,
                        "ui_capability_present": computer_app_ui_capability_present,
                        "mcp_error_code": mcp_error_code,
                    })),
                );
            }
        };
    let protocol_era = match validate_http_protocol(req, &request) {
        Ok(protocol_era) => protocol_era,
        Err(body) => {
            guard.parsed("protocol_error");
            if let Some(uri) = computer_app_resource_uri.as_deref() {
                log_mcp_computer_app_resource_delivery(
                    uri,
                    "validation_failed",
                    computer_app_ui_capability_present,
                    400,
                    body["error"]["code"].as_i64(),
                );
                record_computer_app_resource_audit(
                    "validation_failed",
                    StatusCode::BAD_REQUEST,
                    body["error"]["code"].as_i64(),
                );
            }
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
    let record_audit = |success: bool,
                        status: StatusCode,
                        error: Option<String>,
                        model_ergonomics: Option<&ModelErgonomicsRecord>| {
        if let Some((audit, tool, project)) = audit.as_ref() {
            let mut summary = json!({ "transport": "mcp" });
            if let Some(telemetry) =
                model_ergonomics.and_then(|record| serde_json::to_value(record).ok())
            {
                summary["model_ergonomics"] = telemetry;
            }
            let mut event = ActionAuditRecord::new(tool.clone(), success, status)
                .error(error)
                .summary(summary);
            event.project = project.clone();
            audit.record(event);
        }
    };

    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let host_file_import_trust =
        if tool_name.as_deref() == Some("import_conversation_files_to_project") {
            let decision = mcp_host_file_import_trust_decision(depot, auth.as_ref());
            log_mcp_host_file_import_trust_decision(auth.as_ref(), &decision);
            decision.trust
        } else {
            HostFileImportTrust::Untrusted
        };
    // Defense-in-depth backstop: every tool bounds its own agent/subprocess
    // waits at <= 124s, so this outer limit never preempts a legitimate inner
    // timeout. It only fires if a dispatch path hangs without a bound (the
    // failure mode behind "MCP request never gets a reply"), converting a
    // silently dead HTTP request into an observable JSON-RPC error.
    let request_id = request.id.clone();
    // The shared kernel timer is authoritative for completed runtime calls. Keep
    // one outer emergency timer only so the MCP hard-timeout path does not erase
    // an otherwise established runtime invocation from ergonomics telemetry.
    let mut hard_timeout_model_ergonomics =
        if runtime.model_surface() == ModelSurface::CanonicalConnector {
            None
        } else {
            tool_name.as_deref().and_then(ModelErgonomicsTimer::start)
        };
    let mut model_ergonomics = None;
    let active_trace_id = guard.active_trace_id();
    // Keep the complete MCP dispatch future off the current thread's stack. The
    // handler state spans every method arm (including Connector task polling),
    // so nesting it inline under tracing + timeout can exhaust the default
    // ~2 MiB libtest/Tokio worker stack even when a request takes another arm.
    let outcome = match tokio::time::timeout(
        MCP_DISPATCH_HARD_TIMEOUT,
        scope_active_trace(
            active_trace_id,
            Box::pin(handle_mcp_request_with_lifecycle(
                &runtime,
                connector.as_deref(),
                request,
                auth.as_ref(),
                protocol_era,
                host_file_import_trust,
                window.identity.as_ref(),
                Some(&mut guard),
                Some(&mut model_ergonomics),
            )),
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
            if let Some(uri) = computer_app_resource_uri.as_deref() {
                log_mcp_computer_app_resource_delivery(
                    uri,
                    mcp_protocol_era_label(protocol_era),
                    computer_app_ui_capability_present,
                    500,
                    Some(-32000),
                );
                record_computer_app_resource_audit(
                    mcp_protocol_era_label(protocol_era),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Some(-32000),
                );
            }
            let timeout_model_ergonomics = hard_timeout_model_ergonomics.take().map(|timer| {
                timer
                    .finish()
                    .record_for_pre_result_failure("dispatch_hard_timeout")
            });
            record_audit(
                false,
                StatusCode::INTERNAL_SERVER_ERROR,
                Some("mcp dispatch hard timeout".to_string()),
                timeout_model_ergonomics.as_ref(),
            );
            guard.capture_payload("final_response", &body);
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(500, estimated, Some(false), None, "dispatch_hard_timeout");
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(body));
            guard.handler_returned(500, estimated, Some(false), None, "dispatch_hard_timeout");
            return;
        }
    };

    if let Some(uri) = computer_app_resource_uri.as_deref() {
        log_mcp_computer_app_resource_outcome(
            uri,
            protocol_era,
            computer_app_ui_capability_present,
            &outcome,
        );
        let (status, mcp_error_code) = match &outcome {
            McpOutcome::Ok(_) | McpOutcome::ArtifactExportStream { .. } => (StatusCode::OK, None),
            McpOutcome::BadRequest(body) => {
                (StatusCode::BAD_REQUEST, body["error"]["code"].as_i64())
            }
            McpOutcome::NotFound(body) => (StatusCode::NOT_FOUND, body["error"]["code"].as_i64()),
            McpOutcome::Notification => (StatusCode::ACCEPTED, None),
            McpOutcome::Forbidden { .. } => (StatusCode::FORBIDDEN, None),
        };
        record_computer_app_resource_audit(
            mcp_protocol_era_label(protocol_era),
            status,
            mcp_error_code,
        );
    }

    if matches!(
        outcome,
        McpOutcome::Ok(_) | McpOutcome::ArtifactExportStream { .. }
    ) {
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
                model_ergonomics.as_ref(),
            );
            guard.capture_payload("final_response", &body);
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(200, estimated, Some(true), tool_success, "ok");
            res.render(Json(body));
            guard.handler_returned(200, estimated, Some(true), tool_success, "ok");
        }
        McpOutcome::ArtifactExportStream { id, plan } => {
            record_audit(true, StatusCode::OK, None, None);
            guard.response_serialized(200, None, Some(true), None, "artifact_export_stream");
            res.status_code(StatusCode::OK);
            let _ = res.add_header("content-type", "application/json", true);
            let receiver =
                resources::start_artifact_export_stream(runtime.clone(), id, auth.clone(), plan);
            let response_stream = stream::unfold(receiver, |mut receiver| async move {
                receiver.recv().await.map(|frame| (frame, receiver))
            });
            res.stream(response_stream);
            guard.handler_returned(200, None, Some(true), None, "artifact_export_stream");
        }
        McpOutcome::BadRequest(body) => {
            record_audit(
                false,
                StatusCode::BAD_REQUEST,
                body["error"]["message"].as_str().map(str::to_string),
                model_ergonomics.as_ref(),
            );
            guard.capture_payload("final_response", &body);
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
                model_ergonomics.as_ref(),
            );
            guard.capture_payload("final_response", &body);
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
                model_ergonomics.as_ref(),
            );
            guard.capture_payload("final_response", &body);
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(403, estimated, Some(false), None, "forbidden");
            res.status_code(StatusCode::FORBIDDEN);
            if auth.as_ref().is_some_and(AuthContext::is_oauth_token) {
                let challenge = crate::auth::oauth_insufficient_scope_challenge(required_scope);
                if let Ok(val) = salvo::http::HeaderValue::from_str(&challenge) {
                    res.headers_mut().insert("www-authenticate", val);
                }
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
    let outcome = handle_mcp_request_with_lifecycle(
        runtime,
        None,
        request,
        auth,
        protocol_era,
        HostFileImportTrust::Untrusted,
        None,
        None,
        None,
    )
    .await;
    match outcome {
        McpOutcome::ArtifactExportStream { id, plan } => {
            match resources::mcp_artifact_export_collect_stream_response(runtime, &id, auth, plan)
                .await
            {
                Ok(body) => McpOutcome::Ok(body),
                Err(error) => {
                    resources::mcp_artifact_export_read_error_outcome(Some(id), auth, error)
                }
            }
        }
        outcome => outcome,
    }
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
    mut model_ergonomics_out: Option<&mut Option<ModelErgonomicsRecord>>,
) -> McpOutcome {
    let stateless_2026 = protocol_era == McpProtocolEra::Stateless2026;
    let resource_read_bypasses_runtime_read = stateless_2026
        && request.method == "resources/read"
        && resources::resource_read_bypasses_runtime_read(&request.params);
    let mcp_app_enabled =
        resources::mcp_app_enabled(stateless_2026, runtime.model_surface(), &request.params);

    if auth.is_some()
        && (matches!(
            request.method.as_str(),
            "server/discover" | "tools/list" | "resources/list"
        ) || (request.method == "resources/read" && !resource_read_bypasses_runtime_read)
            || (!stateless_2026
                && matches!(
                    request.method.as_str(),
                    "initialize" | "ping" | "notifications/initialized"
                )))
    {
        if let Some(outcome) = require_mcp_scope(auth, crate::auth::SCOPE_RUNTIME_READ) {
            return outcome;
        }
    }

    if auth.is_some_and(|auth| !auth.is_bootstrap())
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
        return scope_forbidden(
            auth,
            None,
            "authenticated caller cannot call unknown MCP methods",
        );
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
        // modern clients while retaining the initialized 2025 tool-only
        // session lifecycle used by 2025-06-18 and ChatGPT 2025-11-25 clients.
        "server/discover" if stateless_2026 => rpc_result(
            id,
            json!({
                "resultType": "complete",
                "ttlMs": 0,
                "cacheScope": "private",
                "supportedVersions": [
                    MCP_STATELESS_PROTOCOL_VERSION,
                    MCP_CHATGPT_PROTOCOL_VERSION,
                    MCP_PROTOCOL_VERSION
                ],
                "capabilities": if resources::model_surface_supports_computer_app(runtime.model_surface()) {
                    resources::server_capabilities()
                } else if tasks::model_surface_supports_tasks(runtime.model_surface()) {
                    tasks::server_capabilities()
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
                "protocolVersion": legacy_initialize_protocol_version(&request.params),
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
            let oauth_scope_projection = auth.is_some_and(AuthContext::is_oauth_token);
            let mut result = if stateless_2026 {
                if oauth_scope_projection {
                    mcp_tools_list_payload_with_features_for_auth(
                        runtime.model_surface(),
                        crate::config::mcp_compact_schemas_enabled(),
                        resources::model_surface_supports_computer_app(runtime.model_surface()),
                        true,
                        auth,
                    )
                } else {
                    mcp_tools_list_payload_with_compact_and_app(
                        runtime.model_surface(),
                        crate::config::mcp_compact_schemas_enabled(),
                        resources::model_surface_supports_computer_app(runtime.model_surface()),
                    )
                }
            } else if oauth_scope_projection {
                mcp_tools_list_payload_with_features_for_auth(
                    runtime.model_surface(),
                    crate::config::mcp_compact_schemas_enabled(),
                    false,
                    false,
                    auth,
                )
            } else {
                mcp_tools_list_payload(runtime.model_surface())
            };
            if stateless_2026 {
                add_stateless_workflow_recorder_metadata(&mut result, runtime.model_surface());
            }
            if crate::mcp_gateway::authorized(auth) {
                if let Some(tools) = result.get_mut("tools").and_then(Value::as_array_mut) {
                    tools.push(crate::mcp_gateway::tool_spec());
                }
            }
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
            return resources::handle_list(id, mcp_app_enabled);
        }
        "resources/read" if stateless_2026 => {
            return resources::handle_read(
                runtime,
                request.params,
                id,
                auth,
                runtime.model_surface(),
            )
            .await;
        }
        method @ ("tasks/get" | "tasks/update" | "tasks/cancel")
            if stateless_2026 && tasks::model_surface_supports_tasks(runtime.model_surface()) =>
        {
            return tasks::handle_request(
                method,
                request.params,
                id,
                auth,
                connector.expect("validated canonical Connector state"),
            )
            .await;
        }
        "tools/call" => {
            let tasks_extension_declared =
                stateless_2026 && tasks::request_supports_tasks(&request.params);
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
            if let Some(lc) = lifecycle.as_deref() {
                lc.capture_payload("raw_arguments", &params.arguments);
            }
            // Emit dispatch_started only after params parse succeeds and before
            // ToolRuntime work begins.
            if let Some(lc) = lifecycle.as_deref_mut() {
                lc.set_tool_name(Some(params.name.clone()));
                lc.dispatch_started();
            }
            if params.name == crate::mcp_gateway::MCP_TOOL_NAME {
                if let Some(outcome) = require_mcp_scope(auth, crate::auth::SCOPE_MCP_LOCAL) {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("forbidden");
                        lc.dispatch_finished(false, Some(false), "forbidden");
                    }
                    return outcome;
                }
                if let Some(lc) = lifecycle.as_deref() {
                    lc.capture_payload("effective_arguments", &params.arguments);
                }
                let result = crate::mcp_gateway::call(runtime, params.arguments, auth).await;
                let ok = result.get("isError").and_then(Value::as_bool) != Some(true);
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_finished(true, Some(ok), if ok { "success" } else { "tool_error" });
                }
                return McpOutcome::Ok(rpc_result(
                    id,
                    if stateless_2026 {
                        mcp_stateless_result(result, false)
                    } else {
                        result
                    },
                ));
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
            // From here on, the MCP boundary has established a model-visible runtime
            // tool identity. A few MCP-only validations still happen before the
            // shared ToolRuntime kernel; preserve those failed attempts in generic
            // telemetry without creating a second record for normal kernel calls.
            let mut pre_kernel_model_ergonomics = ModelErgonomicsTimer::start(&params.name);
            let resource_tool_call = match resources::prepare_tool_call(
                &params.name,
                stateless_2026,
                runtime.model_surface(),
                auth,
            ) {
                Ok(context) => context,
                Err(error) => {
                    if error.records_model_ergonomics_failure() {
                        if let (Some(slot), Some(timer)) = (
                            model_ergonomics_out.as_deref_mut(),
                            pre_kernel_model_ergonomics.take(),
                        ) {
                            *slot = Some(
                                timer
                                    .finish()
                                    .record_for_pre_result_failure("invalid_arguments"),
                            );
                        }
                    }
                    return McpOutcome::BadRequest(rpc_error(id, -32602, error.message()));
                }
            };
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
                if let Some(lc) = lifecycle.as_deref() {
                    lc.capture_payload("effective_arguments", &params.arguments);
                }
                let task_polling = tasks_extension_declared
                    && matches!(params.name.as_str(), "commands_run" | "checks_run");
                let outcome = if task_polling {
                    connector
                        .call_for_window_with_task_polling(
                            &params.name,
                            params.arguments,
                            auth,
                            ConnectorTransport::Mcp,
                            window,
                        )
                        .await
                } else {
                    connector
                        .call_for_window(
                            &params.name,
                            params.arguments,
                            auth,
                            ConnectorTransport::Mcp,
                            window,
                        )
                        .await
                };
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
                    return scope_forbidden(auth, Some(required_scope), description);
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
                if task_polling && outcome.ok {
                    if let Some(task_outcome) =
                        tasks::promote_connector_tool_call(&id, &outcome, auth, connector).await
                    {
                        return task_outcome;
                    }
                }
                let result = connector_call_tool_result(outcome);
                return McpOutcome::Ok(rpc_result(
                    id,
                    if stateless_2026 {
                        mcp_stateless_result(result, false)
                    } else {
                        result
                    },
                ));
            }
            let session_id = match strip_reserved_session_id(&mut params.arguments) {
                Ok(session_id) => session_id,
                Err(message) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("invalid_arguments");
                        lc.dispatch_finished(false, Some(false), "invalid_arguments");
                    }
                    if let (Some(slot), Some(timer)) = (
                        model_ergonomics_out.as_deref_mut(),
                        pre_kernel_model_ergonomics.take(),
                    ) {
                        *slot = Some(
                            timer
                                .finish()
                                .record_for_pre_result_failure("invalid_arguments"),
                        );
                    }
                    return McpOutcome::BadRequest(rpc_error(id, -32602, message));
                }
            };
            let ack_session_message_ids = if stateless_2026 {
                match strip_stateless_ack_session_message_ids(&mut params.arguments) {
                    Ok(ids) => ids,
                    Err(message) => {
                        if let Some(lc) = lifecycle.as_deref() {
                            lc.dispatch_failed("invalid_arguments");
                            lc.dispatch_finished(false, Some(false), "invalid_arguments");
                        }
                        if let (Some(slot), Some(timer)) = (
                            model_ergonomics_out.as_deref_mut(),
                            pre_kernel_model_ergonomics.take(),
                        ) {
                            *slot = Some(
                                timer
                                    .finish()
                                    .record_for_pre_result_failure("invalid_arguments"),
                            );
                        }
                        return McpOutcome::BadRequest(rpc_error(id, -32602, message));
                    }
                }
            } else {
                Vec::new()
            };
            let session_message_resolution = if stateless_2026 {
                if let Some(arguments) = params.arguments.as_object_mut() {
                    arguments.remove(
                        crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_INTERNAL_FIELD,
                    );
                }
                match strip_stateless_session_message_resolution(&mut params.arguments) {
                    Ok(value) => value,
                    Err(message) => {
                        if let Some(lc) = lifecycle.as_deref() {
                            lc.dispatch_failed("invalid_arguments");
                            lc.dispatch_finished(false, Some(false), "invalid_arguments");
                        }
                        if let (Some(slot), Some(timer)) = (
                            model_ergonomics_out.as_deref_mut(),
                            pre_kernel_model_ergonomics.take(),
                        ) {
                            *slot = Some(
                                timer
                                    .finish()
                                    .record_for_pre_result_failure("invalid_arguments"),
                            );
                        }
                        return McpOutcome::BadRequest(rpc_error(id, -32602, message));
                    }
                }
            } else {
                None
            };
            if session_message_resolution.is_some() && session_id.is_none() {
                let message = format!(
                    "field '{}' requires '{}' for the exact target Workflow Session",
                    crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD,
                    crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD,
                );
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return McpOutcome::BadRequest(rpc_error(id, -32602, message));
            }
            if let (Some(arguments), Some(resolution)) =
                (params.arguments.as_object_mut(), session_message_resolution)
            {
                arguments.insert(
                    crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_INTERNAL_FIELD
                        .to_string(),
                    json!(resolution),
                );
            }
            if !ack_session_message_ids.is_empty() {
                if let Some(arguments) = params.arguments.as_object_mut() {
                    arguments.insert(
                        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_INTERNAL_FIELD
                            .to_string(),
                        json!(ack_session_message_ids),
                    );
                }
            }
            let context_continuity_capable = stateless_2026
                && matches!(runtime.model_surface(), ModelSurface::FullOperatorRuntime);
            if context_continuity_capable {
                if let Some(arguments) = params.arguments.as_object_mut() {
                    arguments.remove(
                        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD,
                    );
                }
                let context_revision =
                    strip_stateless_ack_session_context_revision(&mut params.arguments);
                if let (Some(arguments), Some(context_revision)) =
                    (params.arguments.as_object_mut(), context_revision)
                {
                    arguments.insert(
                        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD
                            .to_string(),
                        context_revision,
                    );
                }
            }
            if let Some(lc) = lifecycle.as_deref() {
                lc.capture_payload("effective_arguments", &params.arguments);
            }
            let as_image_requested = params.name == "read_project_artifact"
                && params.arguments.get("as_image").and_then(Value::as_bool) == Some(true);
            let outcome = runtime
                .call_tool_with_context_protocol_capability(
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
                    context_continuity_capable,
                )
                .await;
            let model_ergonomics_completion = outcome.model_ergonomics;
            let result = match outcome.error_status {
                Some(ToolCallErrorStatus::InsufficientScope {
                    required_scope,
                    description,
                }) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("forbidden");
                        lc.dispatch_finished(false, Some(false), "forbidden");
                    }
                    if let (Some(slot), Some(completion)) = (
                        model_ergonomics_out.as_deref_mut(),
                        model_ergonomics_completion.as_ref(),
                    ) {
                        *slot =
                            Some(completion.record_for_pre_result_failure("insufficient_scope"));
                    }
                    return scope_forbidden(auth, required_scope, description);
                }
                Some(ToolCallErrorStatus::InvalidArguments { message }) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("invalid_arguments");
                        lc.dispatch_finished(false, Some(false), "invalid_arguments");
                    }
                    if let (Some(slot), Some(completion)) = (
                        model_ergonomics_out.as_deref_mut(),
                        model_ergonomics_completion.as_ref(),
                    ) {
                        *slot = Some(completion.record_for_pre_result_failure("invalid_arguments"));
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
            let result = resources::adapt_tool_result(
                &params.name,
                as_image_requested,
                result,
                resource_tool_call,
            );
            let model_ergonomics = model_ergonomics_completion.as_ref().and_then(|completion| {
                result
                    .get("structuredContent")
                    .and_then(|structured| completion.record_for_structured_content(structured))
            });
            if let Some(slot) = model_ergonomics_out.as_deref_mut() {
                *slot = model_ergonomics;
            }
            return McpOutcome::Ok(rpc_result(
                id,
                if stateless_2026 {
                    mcp_stateless_result(result, false)
                } else {
                    result
                },
            ));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostFileImportTrustReason {
    Trusted,
    MissingConfig,
    MissingDatabase,
    MissingAuth,
    NotOAuthToken,
    MissingAllowedClientId,
    OAuthDisabled,
    ClientIdNotConfigured,
    ClientRegistrationMissingOrRevoked,
    ClientRegistrationLookupFailed,
}

impl HostFileImportTrustReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::MissingConfig => "missing_config",
            Self::MissingDatabase => "missing_database",
            Self::MissingAuth => "missing_auth",
            Self::NotOAuthToken => "not_oauth_token",
            Self::MissingAllowedClientId => "missing_allowed_client_id",
            Self::OAuthDisabled => "oauth_disabled",
            Self::ClientIdNotConfigured => "client_id_not_configured",
            Self::ClientRegistrationMissingOrRevoked => "client_registration_missing_or_revoked",
            Self::ClientRegistrationLookupFailed => "client_registration_lookup_failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HostFileImportTrustDecision {
    trust: HostFileImportTrust,
    reason: HostFileImportTrustReason,
    config_present: bool,
    database_present: bool,
    oauth_enabled: bool,
    configured_trusted_client_count: usize,
    client_id_configured: Option<bool>,
    active_client_registration_found: Option<bool>,
}

#[cfg(test)]
static LAST_MCP_HOST_FILE_IMPORT_TRUST_DECISION: std::sync::OnceLock<
    std::sync::Mutex<Option<HostFileImportTrustDecision>>,
> = std::sync::OnceLock::new();

#[cfg(test)]
fn take_last_mcp_host_file_import_trust_decision() -> Option<HostFileImportTrustDecision> {
    LAST_MCP_HOST_FILE_IMPORT_TRUST_DECISION
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap()
        .take()
}

impl HostFileImportTrustDecision {
    fn unavailable(reason: HostFileImportTrustReason) -> Self {
        Self {
            trust: HostFileImportTrust::Untrusted,
            reason,
            config_present: false,
            database_present: false,
            oauth_enabled: false,
            configured_trusted_client_count: 0,
            client_id_configured: None,
            active_client_registration_found: None,
        }
    }

    fn from_config(reason: HostFileImportTrustReason, config: &crate::Config) -> Self {
        Self {
            trust: HostFileImportTrust::Untrusted,
            reason,
            config_present: true,
            database_present: false,
            oauth_enabled: config.oauth2.enabled,
            configured_trusted_client_count: config.oauth2.trusted_mcp_file_client_ids.len(),
            client_id_configured: None,
            active_client_registration_found: None,
        }
    }
}

fn mcp_host_file_import_trust_decision_from_state(
    config: &crate::Config,
    db: &crate::Database,
    auth: Option<&AuthContext>,
) -> HostFileImportTrustDecision {
    let base = HostFileImportTrustDecision {
        trust: HostFileImportTrust::Untrusted,
        reason: HostFileImportTrustReason::MissingAuth,
        config_present: true,
        database_present: true,
        oauth_enabled: config.oauth2.enabled,
        configured_trusted_client_count: config.oauth2.trusted_mcp_file_client_ids.len(),
        client_id_configured: None,
        active_client_registration_found: None,
    };
    let Some(auth) = auth else {
        return base;
    };
    if !auth.is_oauth_token() {
        return HostFileImportTrustDecision {
            reason: HostFileImportTrustReason::NotOAuthToken,
            ..base
        };
    }
    let Some(client_id) = auth
        .allowed_client_id
        .as_deref()
        .map(str::trim)
        .filter(|client_id| !client_id.is_empty())
    else {
        return HostFileImportTrustDecision {
            reason: HostFileImportTrustReason::MissingAllowedClientId,
            ..base
        };
    };
    if !config.oauth2.enabled {
        return HostFileImportTrustDecision {
            reason: HostFileImportTrustReason::OAuthDisabled,
            ..base
        };
    }
    let client_id_configured = config
        .oauth2
        .trusted_mcp_file_client_ids
        .iter()
        .any(|trusted_client_id| trusted_client_id == client_id);
    if !client_id_configured {
        return HostFileImportTrustDecision {
            reason: HostFileImportTrustReason::ClientIdNotConfigured,
            client_id_configured: Some(false),
            ..base
        };
    }
    match db.get_oauth_client_by_client_id(client_id) {
        Ok(Some(client)) if client.client_id == client_id => HostFileImportTrustDecision {
            trust: HostFileImportTrust::TrustedOAuthClient,
            reason: HostFileImportTrustReason::Trusted,
            client_id_configured: Some(true),
            active_client_registration_found: Some(true),
            ..base
        },
        Ok(_) => HostFileImportTrustDecision {
            reason: HostFileImportTrustReason::ClientRegistrationMissingOrRevoked,
            client_id_configured: Some(true),
            active_client_registration_found: Some(false),
            ..base
        },
        Err(_) => HostFileImportTrustDecision {
            reason: HostFileImportTrustReason::ClientRegistrationLookupFailed,
            client_id_configured: Some(true),
            active_client_registration_found: None,
            ..base
        },
    }
}

#[cfg(test)]
fn mcp_host_file_import_trust_from_state(
    config: &crate::Config,
    db: &crate::Database,
    auth: Option<&AuthContext>,
) -> HostFileImportTrust {
    mcp_host_file_import_trust_decision_from_state(config, db, auth).trust
}

fn mcp_host_file_import_trust_decision(
    depot: &Depot,
    auth: Option<&AuthContext>,
) -> HostFileImportTrustDecision {
    let Some(config) = crate::auth::get_config(depot) else {
        return HostFileImportTrustDecision::unavailable(HostFileImportTrustReason::MissingConfig);
    };
    let Some(db) = crate::auth::get_db(depot) else {
        return HostFileImportTrustDecision::from_config(
            HostFileImportTrustReason::MissingDatabase,
            config.as_ref(),
        );
    };
    mcp_host_file_import_trust_decision_from_state(config.as_ref(), db.as_ref(), auth)
}

fn mcp_auth_kind_classification(auth: Option<&AuthContext>) -> &'static str {
    match auth.map(|auth| auth.kind) {
        None => "none",
        Some(crate::auth::AuthKind::OAuth2Token) => "oauth2",
        Some(crate::auth::AuthKind::ApiToken) => "api_token",
        Some(crate::auth::AuthKind::Bootstrap) => "bootstrap",
        Some(crate::auth::AuthKind::AgentToken) => "agent_token",
        Some(crate::auth::AuthKind::AccountCredential) => "account_credential",
        Some(crate::auth::AuthKind::SharedKey) => "shared_key",
        Some(crate::auth::AuthKind::ProjectCredential) => "project_credential",
        Some(crate::auth::AuthKind::OpenAnonymous) => "open_anonymous",
    }
}

fn mcp_token_kind_classification(auth: Option<&AuthContext>) -> &'static str {
    match auth.and_then(|auth| auth.token_kind.as_deref()) {
        None => "none",
        Some("oauth2") => "oauth2",
        Some("oauth2_shared_key") => "oauth2_shared_key",
        Some("oauth2_project") => "oauth2_project",
        Some("user") => "user",
        Some("agent") => "agent",
        Some(_) => "other",
    }
}

fn log_mcp_host_file_import_trust_decision(
    auth: Option<&AuthContext>,
    decision: &HostFileImportTrustDecision,
) {
    #[cfg(test)]
    {
        *LAST_MCP_HOST_FILE_IMPORT_TRUST_DECISION
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap() = Some(*decision);
    }
    let allowed_client_id_present = auth
        .and_then(|auth| auth.allowed_client_id.as_deref())
        .is_some_and(|client_id| !client_id.trim().is_empty());
    tracing::info!(
        target: "webcodex::mcp",
        trust = decision.trust.is_trusted(),
        reason = decision.reason.as_str(),
        auth_kind = mcp_auth_kind_classification(auth),
        token_kind = mcp_token_kind_classification(auth),
        allowed_client_id_present,
        config_present = decision.config_present,
        database_present = decision.database_present,
        oauth_enabled = decision.oauth_enabled,
        configured_trusted_client_count = decision.configured_trusted_client_count,
        client_id_configured = ?decision.client_id_configured,
        active_client_registration_found = ?decision.active_client_registration_found,
        "mcp_host_file_import_trust_decision"
    );
}

fn require_mcp_scope(auth: Option<&AuthContext>, scope: &'static str) -> Option<McpOutcome> {
    let auth = auth?;
    if auth.has_scope(scope) {
        return None;
    }
    Some(scope_forbidden(
        Some(auth),
        Some(scope),
        format!("missing required scope: {}", scope),
    ))
}

fn strip_reserved_session_id(arguments: &mut Value) -> Result<Option<String>, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Ok(None);
    };
    let canonical =
        object.remove(crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD);
    let legacy = object.remove(MCP_RESERVED_SESSION_ID_FIELD);

    let canonical = match canonical {
        None => None,
        Some(Value::String(value)) => {
            let value = value.trim();
            if value.is_empty() {
                return Err(format!(
                    "field '{}' must be a non-empty string",
                    crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD
                ));
            }
            Some(value.to_string())
        }
        Some(_) => {
            return Err(format!(
                "field '{}' must be a non-empty string",
                crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD
            ));
        }
    };
    let legacy = legacy
        .and_then(|value| value.as_str().map(str::to_string))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let (Some(canonical), Some(legacy)) = (&canonical, &legacy) {
        if canonical != legacy {
            return Err(format!(
                "fields '{}' and '{}' must identify the same Workflow Session when both are provided",
                crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD,
                MCP_RESERVED_SESSION_ID_FIELD
            ));
        }
    }
    Ok(canonical.or(legacy))
}

fn strip_stateless_ack_session_message_ids(arguments: &mut Value) -> Result<Vec<String>, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Ok(Vec::new());
    };
    let Some(value) =
        object.remove(crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD)
    else {
        return Ok(Vec::new());
    };
    let Value::Array(values) = value else {
        return Err(format!(
            "field '{}' must be an array of wc_msg_* ids",
            crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD
        ));
    };
    if values.len() > crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_MESSAGE_IDS {
        return Err(format!(
            "field '{}' accepts at most {} message ids",
            crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD,
            crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_MESSAGE_IDS
        ));
    }
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = std::collections::HashSet::new();
    for value in values {
        let Value::String(value) = value else {
            return Err(format!(
                "field '{}' must contain only wc_msg_* strings",
                crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD
            ));
        };
        let value = value.trim();
        let valid = value.strip_prefix("wc_msg_").is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix
                    .as_bytes()
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        });
        if !valid {
            return Err(format!(
                "field '{}' must contain only valid wc_msg_* ids",
                crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD
            ));
        }
        if seen.insert(value.to_string()) {
            normalized.push(value.to_string());
        }
    }
    Ok(normalized)
}

fn strip_stateless_session_message_resolution(
    arguments: &mut Value,
) -> Result<Option<crate::tool_runtime::sessions::ToolCallSessionMessageResolution>, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Ok(None);
    };
    let Some(value) =
        object.remove(crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD)
    else {
        return Ok(None);
    };
    let Value::Object(mut fields) = value else {
        return Err(format!(
            "field '{}' must be an object with message_id and resolution",
            crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD
        ));
    };
    if fields.len() != 2 || !fields.contains_key("message_id") || !fields.contains_key("resolution")
    {
        return Err(format!(
            "field '{}' accepts exactly message_id and resolution",
            crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD
        ));
    }
    let Some(Value::String(message_id)) = fields.remove("message_id") else {
        return Err("session_message_resolution.message_id must be a wc_msg_* string".to_string());
    };
    let message_id = message_id.trim().to_string();
    let valid_message_id = message_id.strip_prefix("wc_msg_").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix
                .as_bytes()
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    });
    if !valid_message_id {
        return Err(
            "session_message_resolution.message_id must be a valid wc_msg_* id".to_string(),
        );
    }
    let Some(Value::String(resolution)) = fields.remove("resolution") else {
        return Err("session_message_resolution.resolution must be a string".to_string());
    };
    let resolution = resolution.trim().to_string();
    if resolution.is_empty() {
        return Err("session_message_resolution.resolution must not be empty".to_string());
    }
    if resolution.chars().count() > crate::tool_runtime::sessions::MAX_MESSAGE_RESOLUTION_CHARS {
        return Err(format!(
            "session_message_resolution.resolution exceeds {} chars",
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

fn strip_stateless_ack_session_context_revision(arguments: &mut Value) -> Option<Value> {
    arguments
        .as_object_mut()?
        .remove(crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_FIELD)
}

fn scope_forbidden(
    auth: Option<&AuthContext>,
    required_scope: Option<&'static str>,
    description: impl Into<String>,
) -> McpOutcome {
    McpOutcome::Forbidden {
        body: crate::auth::scope_forbidden_body(auth, description),
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

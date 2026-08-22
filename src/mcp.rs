use crate::action_audit::{ActionAudit, ActionAuditRecord};
use crate::auth::AuthContext;
use crate::connector_runtime::{ConnectorRuntime, ConnectorRuntimeSlot, ConnectorTransport};
use crate::json_error;
use crate::model_surface::ModelSurface;
use crate::tool_request_trace::{
    estimate_json_bytes, jsonrpc_id_safe, new_trace_id, ToolRequestLifecycle,
};
use crate::tool_runtime::kernel::{
    check_runtime_tool_scope, HostFileImportTrust, ToolCallContext, ToolCallErrorStatus,
    ToolCallRequest as KernelToolCallRequest, ToolTransport,
};
use crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES;
#[cfg(test)]
use crate::tool_runtime::MAX_PROJECT_ARTIFACT_BYTES;
use crate::tool_runtime::{
    registered_tool_specs, validate_project_artifact_export_snapshot,
    ProjectArtifactExportSnapshot, ToolResult, ToolRuntime, ToolSpec,
    MAX_PROJECT_ARTIFACT_EXPORT_BYTES, MAX_READ_PROJECT_ARTIFACT_LENGTH,
};
use base64::{engine::general_purpose, Engine as _};
use futures_util::{future::join_all, stream};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_CHATGPT_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_STATELESS_PROTOCOL_VERSION: &str = "2026-07-28";
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const MCP_METHOD_HEADER: &str = "mcp-method";
const MCP_NAME_HEADER: &str = "mcp-name";
const MCP_ARTIFACT_EXPORT_URI_PREFIX: &str = "webcodex-artifact://export/";
const MCP_ARTIFACT_EXPORT_ID_PREFIX: &str = "wc_export_";
const MCP_SNAPSHOT_RESOURCE_URI_PREFIX: &str = "webcodex-snapshot://view/";
const MCP_SNAPSHOT_RESOURCE_ID_PREFIX: &str = "wc_snapshot_";
const MCP_SNAPSHOT_RESOURCE_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_MCP_SNAPSHOT_RESOURCES: usize = 32;
const MAX_MCP_SNAPSHOT_RESOURCES_PER_CALLER: usize = 8;
const MCP_ARTIFACT_EXPORT_TTL: Duration = Duration::from_secs(5 * 60);
const MCP_ARTIFACT_EXPORT_ADMISSION_TIMEOUT: Duration = Duration::from_secs(5);
const MCP_ARTIFACT_EXPORT_READ_TIMEOUT: Duration = Duration::from_secs(120);
const MCP_ARTIFACT_EXPORT_STREAM_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_MCP_ARTIFACT_EXPORT_READS: usize = 2;
const MAX_MCP_ARTIFACT_EXPORT_CHUNK_READS: usize = 4;
const MAX_MCP_ARTIFACT_EXPORTS: usize = 128;
const MAX_MCP_ARTIFACT_EXPORTS_PER_CALLER: usize = 16;
const MCP_ARTIFACT_EXPORT_BUSY_CODE: i64 = -32029;
const MCP_HEADER_MISMATCH: i64 = -32020;
const MCP_UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;
const MCP_SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[
    MCP_STATELESS_PROTOCOL_VERSION,
    MCP_CHATGPT_PROTOCOL_VERSION,
    MCP_PROTOCOL_VERSION,
];
const MCP_UI_EXTENSION: &str = "io.modelcontextprotocol/ui";
const MCP_COMPUTER_UI_RESOURCE_URI: &str = "ui://webcodex/computer/v11";
const MCP_COMPUTER_UI_RESOURCE_LEGACY_URIS: &[&str] = &[
    "ui://webcodex/computer/v1",
    "ui://webcodex/computer/v2",
    "ui://webcodex/computer/v3",
    "ui://webcodex/computer/v4",
    "ui://webcodex/computer/v5",
    "ui://webcodex/computer/v6",
    "ui://webcodex/computer/v7",
    "ui://webcodex/computer/v8",
    "ui://webcodex/computer/v9",
    "ui://webcodex/computer/v10",
];
// Temporary gray-card diagnostic: force the host to re-read the canonical App
// resource for every card so resource reuse/cache is not an unobserved variable.
const MCP_COMPUTER_UI_RESOURCE_TTL_MS: u64 = 0;
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

fn mcp_computer_app_resource_meta() -> Value {
    json!({
        "ui": {
            "prefersBorder": true,
            "domain": MCP_COMPUTER_UI_DOMAIN,
            "csp": {
                "connectDomains": [],
                "resourceDomains": []
            }
        }
    })
}

fn mcp_computer_app_resources_list() -> Value {
    json!({
        "resources": [{
            "uri": MCP_COMPUTER_UI_RESOURCE_URI,
            "name": "WebCodex Computer",
            "description": "Minimal read-only WebCodex Computer screenshot card that performs only the standard MCP Apps handshake and renders the native computer_snapshot image.",
            "mimeType": MCP_UI_RESOURCE_MIME_TYPE,
            "_meta": mcp_computer_app_resource_meta()
        }]
    })
}

fn is_mcp_computer_app_resource_uri(uri: &str) -> bool {
    uri == MCP_COMPUTER_UI_RESOURCE_URI || MCP_COMPUTER_UI_RESOURCE_LEGACY_URIS.contains(&uri)
}

fn mcp_computer_app_resource_read(uri: &str) -> Option<Value> {
    // ChatGPT can retain an older tool descriptor across connector refreshes.
    // Keep prior computer App URIs as hidden read aliases so an already-bound
    // card can fetch the current safe template. resources/list and tools/list
    // still advertise only the canonical URI above.
    let supported = is_mcp_computer_app_resource_uri(uri);
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum McpArtifactExportCallerBinding {
    Bootstrap,
    ApiToken {
        api_key_id: String,
    },
    AgentToken {
        api_key_id: String,
    },
    AccountCredential {
        user_id: String,
    },
    OAuthUser {
        user_id: String,
        client_id: String,
    },
    OAuthSharedKey {
        shared_key_hash: String,
        client_id: String,
    },
    SharedKey {
        shared_key_hash: String,
    },
    ProjectCredential {
        project_grant_id: String,
    },
}

fn mcp_artifact_export_caller_binding(
    auth: Option<&AuthContext>,
) -> Result<McpArtifactExportCallerBinding, &'static str> {
    let auth = auth.ok_or("authenticated caller identity is unavailable")?;
    match auth.kind {
        crate::auth::AuthKind::Bootstrap => Ok(McpArtifactExportCallerBinding::Bootstrap),
        crate::auth::AuthKind::ApiToken => auth
            .api_key_id
            .as_ref()
            .filter(|value| !value.is_empty())
            .cloned()
            .map(|api_key_id| McpArtifactExportCallerBinding::ApiToken { api_key_id })
            .ok_or("API token identity is unavailable"),
        crate::auth::AuthKind::AgentToken => auth
            .api_key_id
            .as_ref()
            .filter(|value| !value.is_empty())
            .cloned()
            .map(|api_key_id| McpArtifactExportCallerBinding::AgentToken { api_key_id })
            .ok_or("agent token identity is unavailable"),
        crate::auth::AuthKind::AccountCredential => auth
            .user_id
            .as_ref()
            .filter(|value| !value.is_empty())
            .cloned()
            .map(|user_id| McpArtifactExportCallerBinding::AccountCredential { user_id })
            .ok_or("account identity is unavailable"),
        crate::auth::AuthKind::OAuth2Token => {
            let client_id = auth
                .allowed_client_id
                .as_ref()
                .filter(|value| !value.is_empty())
                .cloned()
                .ok_or("OAuth client identity is unavailable")?;
            if auth.is_oauth_shared_key_subject() {
                let shared_key_hash = auth
                    .shared_key_hash
                    .as_ref()
                    .filter(|value| !value.is_empty())
                    .cloned()
                    .ok_or("OAuth shared-key subject identity is unavailable")?;
                Ok(McpArtifactExportCallerBinding::OAuthSharedKey {
                    shared_key_hash,
                    client_id,
                })
            } else if auth.token_kind.as_deref() == Some("oauth2") {
                let user_id = auth
                    .user_id
                    .as_ref()
                    .filter(|value| !value.is_empty())
                    .cloned()
                    .ok_or("OAuth user identity is unavailable")?;
                Ok(McpArtifactExportCallerBinding::OAuthUser { user_id, client_id })
            } else {
                Err("unsupported OAuth subject identity")
            }
        }
        crate::auth::AuthKind::SharedKey => auth
            .shared_key_hash
            .as_ref()
            .filter(|value| !value.is_empty())
            .cloned()
            .map(|shared_key_hash| McpArtifactExportCallerBinding::SharedKey { shared_key_hash })
            .ok_or("shared-key identity is unavailable"),
        crate::auth::AuthKind::ProjectCredential => auth
            .project_grant_id
            .as_ref()
            .filter(|value| !value.is_empty())
            .cloned()
            .map(
                |project_grant_id| McpArtifactExportCallerBinding::ProjectCredential {
                    project_grant_id,
                },
            )
            .ok_or("project credential identity is unavailable"),
        crate::auth::AuthKind::OpenAnonymous => {
            Err("anonymous MCP callers cannot create artifact export resources")
        }
    }
}

#[derive(Debug, Clone)]
struct McpArtifactExportRecord {
    caller: McpArtifactExportCallerBinding,
    project: String,
    snapshot: ProjectArtifactExportSnapshot,
    expires_at: Instant,
}

#[derive(Default)]
struct McpArtifactExportRegistry {
    entries: HashMap<String, McpArtifactExportRecord>,
    order: VecDeque<String>,
}

impl McpArtifactExportRegistry {
    fn cleanup(&mut self, now: Instant) {
        self.entries.retain(|_, record| record.expires_at > now);
        self.order.retain(|id| self.entries.contains_key(id));
    }

    fn insert(&mut self, record: McpArtifactExportRecord) -> String {
        self.cleanup(Instant::now());
        while self
            .entries
            .values()
            .filter(|existing| existing.caller == record.caller)
            .count()
            >= MAX_MCP_ARTIFACT_EXPORTS_PER_CALLER
        {
            let Some(position) = self.order.iter().position(|id| {
                self.entries
                    .get(id)
                    .is_some_and(|existing| existing.caller == record.caller)
            }) else {
                break;
            };
            if let Some(id) = self.order.remove(position) {
                self.entries.remove(&id);
            }
        }
        while self.entries.len() >= MAX_MCP_ARTIFACT_EXPORTS {
            if let Some(id) = self.order.pop_front() {
                self.entries.remove(&id);
            } else {
                break;
            }
        }
        let id = loop {
            let candidate = format!(
                "{MCP_ARTIFACT_EXPORT_ID_PREFIX}{}",
                uuid::Uuid::new_v4().simple()
            );
            if !self.entries.contains_key(&candidate) {
                break candidate;
            }
        };
        self.order.push_back(id.clone());
        self.entries.insert(id.clone(), record);
        format!("{MCP_ARTIFACT_EXPORT_URI_PREFIX}{id}")
    }

    fn get_for_caller(
        &mut self,
        uri: &str,
        caller: &McpArtifactExportCallerBinding,
    ) -> Option<McpArtifactExportRecord> {
        self.cleanup(Instant::now());
        let id = mcp_artifact_export_id_from_uri(uri)?;
        self.entries
            .get(id)
            .filter(|record| &record.caller == caller)
            .cloned()
    }
}

static MCP_ARTIFACT_EXPORT_REGISTRY: OnceLock<Mutex<McpArtifactExportRegistry>> = OnceLock::new();
static MCP_ARTIFACT_EXPORT_READ_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn mcp_artifact_export_registry() -> &'static Mutex<McpArtifactExportRegistry> {
    MCP_ARTIFACT_EXPORT_REGISTRY.get_or_init(|| Mutex::new(McpArtifactExportRegistry::default()))
}

fn mcp_artifact_export_read_semaphore() -> Arc<Semaphore> {
    MCP_ARTIFACT_EXPORT_READ_SEMAPHORE
        .get_or_init(|| Arc::new(Semaphore::new(MAX_MCP_ARTIFACT_EXPORT_READS)))
        .clone()
}

fn mcp_artifact_export_id_from_uri(uri: &str) -> Option<&str> {
    let id = uri.strip_prefix(MCP_ARTIFACT_EXPORT_URI_PREFIX)?;
    let hex = id.strip_prefix(MCP_ARTIFACT_EXPORT_ID_PREFIX)?;
    (hex.len() == 32
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
    .then_some(id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpSnapshotResourceKind {
    Window,
    Display,
}

impl McpSnapshotResourceKind {
    fn from_tool_name(tool_name: &str) -> Option<Self> {
        match tool_name {
            "computer_snapshot" => Some(Self::Window),
            "computer_snapshot_display" => Some(Self::Display),
            _ => None,
        }
    }

    fn name(self, client_id: &str, mime_type: &str) -> String {
        let extension = match mime_type {
            "image/png" => "png",
            "image/webp" => "webp",
            _ => "jpg",
        };
        let kind = match self {
            Self::Window => "window",
            Self::Display => "display",
        };
        let normalized_client_id: String = client_id
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        let client_id = if normalized_client_id.is_empty() {
            "computer"
        } else {
            normalized_client_id.as_str()
        };
        format!("{client_id}-{kind}-snapshot.{extension}")
    }
}

#[derive(Debug, Clone)]
struct McpSnapshotResourceRecord {
    caller: McpArtifactExportCallerBinding,
    kind: McpSnapshotResourceKind,
    bytes: Arc<[u8]>,
    mime_type: String,
    expires_at: Instant,
}

#[derive(Default)]
struct McpSnapshotResourceRegistry {
    entries: HashMap<String, McpSnapshotResourceRecord>,
    order: VecDeque<String>,
}

impl McpSnapshotResourceRegistry {
    fn cleanup(&mut self, now: Instant) {
        self.entries.retain(|_, record| record.expires_at > now);
        self.order.retain(|id| self.entries.contains_key(id));
    }

    fn insert(&mut self, record: McpSnapshotResourceRecord) -> String {
        self.cleanup(Instant::now());
        while self
            .entries
            .values()
            .filter(|existing| existing.caller == record.caller)
            .count()
            >= MAX_MCP_SNAPSHOT_RESOURCES_PER_CALLER
        {
            let Some(position) = self.order.iter().position(|id| {
                self.entries
                    .get(id)
                    .is_some_and(|existing| existing.caller == record.caller)
            }) else {
                break;
            };
            if let Some(id) = self.order.remove(position) {
                self.entries.remove(&id);
            }
        }
        while self.entries.len() >= MAX_MCP_SNAPSHOT_RESOURCES {
            if let Some(id) = self.order.pop_front() {
                self.entries.remove(&id);
            } else {
                break;
            }
        }
        let id = loop {
            let candidate = format!(
                "{MCP_SNAPSHOT_RESOURCE_ID_PREFIX}{}",
                uuid::Uuid::new_v4().simple()
            );
            if !self.entries.contains_key(&candidate) {
                break candidate;
            }
        };
        self.order.push_back(id.clone());
        self.entries.insert(id.clone(), record);
        format!("{MCP_SNAPSHOT_RESOURCE_URI_PREFIX}{id}")
    }

    fn get_for_caller(
        &mut self,
        uri: &str,
        caller: &McpArtifactExportCallerBinding,
    ) -> Option<McpSnapshotResourceRecord> {
        self.cleanup(Instant::now());
        let id = mcp_snapshot_resource_id_from_uri(uri)?;
        self.entries
            .get(id)
            .filter(|record| &record.caller == caller)
            .cloned()
    }
}

static MCP_SNAPSHOT_RESOURCE_REGISTRY: OnceLock<Mutex<McpSnapshotResourceRegistry>> =
    OnceLock::new();

fn mcp_snapshot_resource_registry() -> &'static Mutex<McpSnapshotResourceRegistry> {
    MCP_SNAPSHOT_RESOURCE_REGISTRY
        .get_or_init(|| Mutex::new(McpSnapshotResourceRegistry::default()))
}

fn mcp_snapshot_resource_id_from_uri(uri: &str) -> Option<&str> {
    let id = uri.strip_prefix(MCP_SNAPSHOT_RESOURCE_URI_PREFIX)?;
    let hex = id.strip_prefix(MCP_SNAPSHOT_RESOURCE_ID_PREFIX)?;
    (hex.len() == 32
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
    .then_some(id)
}

fn mcp_issue_artifact_export(
    caller: McpArtifactExportCallerBinding,
    result: &ToolResult,
) -> Result<(String, ProjectArtifactExportSnapshot), String> {
    let project = result
        .output
        .get("project")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "export result is missing canonical project identity".to_string())?
        .to_string();
    let path = result
        .output
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "export result is missing artifact path".to_string())?;
    let snapshot = validate_project_artifact_export_snapshot(path, &result.output)?;
    if result.output.get("name").and_then(Value::as_str) != Some(snapshot.name.as_str()) {
        return Err(
            "export result basename does not match validated artifact metadata".to_string(),
        );
    }
    let record = McpArtifactExportRecord {
        caller,
        project,
        snapshot: snapshot.clone(),
        expires_at: Instant::now() + MCP_ARTIFACT_EXPORT_TTL,
    };
    let uri = mcp_artifact_export_registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(record);
    Ok((uri, snapshot))
}

fn mcp_artifact_export_tool_result(
    result: ToolResult,
    caller: McpArtifactExportCallerBinding,
) -> Value {
    if !result.success {
        return mcp_runtime_tool_result("export_project_artifact", false, result);
    }
    let (uri, snapshot) = match mcp_issue_artifact_export(caller, &result) {
        Ok(value) => value,
        Err(error) => {
            return mcp_runtime_tool_result(
                "export_project_artifact",
                false,
                ToolResult::err(format!("cannot frame artifact export resource: {error}")),
            )
        }
    };
    json!({
        "content": [{
            "type": "resource_link",
            "uri": uri,
            "name": snapshot.name,
            "mimeType": snapshot.mime_type,
            "description": "Short-lived authenticated WebCodex project artifact export. Read this URI with MCP resources/read to retrieve the complete bounded binary."
        }],
        "structuredContent": {
            "success": true,
            "output": result.output,
            "error": Value::Null,
        },
        "isError": false
    })
}

#[cfg(test)]
fn mcp_expire_artifact_export_for_test(uri: &str) {
    if let Some(id) = mcp_artifact_export_id_from_uri(uri) {
        let mut registry = mcp_artifact_export_registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(record) = registry.entries.get_mut(id) {
            record.expires_at = Instant::now();
        }
    }
}

pub(crate) fn mcp_runtime_tool_result(
    tool_name: &str,
    as_image_requested: bool,
    result: ToolResult,
) -> Value {
    mcp_runtime_tool_result_with_snapshot_resource(tool_name, as_image_requested, result, None)
}

fn mcp_runtime_tool_result_with_snapshot_resource(
    tool_name: &str,
    as_image_requested: bool,
    mut result: ToolResult,
    snapshot_caller: Option<McpArtifactExportCallerBinding>,
) -> Value {
    let native_image_requested = (tool_name == "read_project_artifact" && as_image_requested)
        || matches!(tool_name, "computer_snapshot" | "computer_snapshot_display");
    if native_image_requested && result.success {
        match mcp_native_image_tool_result(tool_name, &mut result, snapshot_caller) {
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

fn mcp_native_image_tool_result(
    tool_name: &str,
    result: &mut ToolResult,
    snapshot_caller: Option<McpArtifactExportCallerBinding>,
) -> Result<Value, String> {
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
    } else if tool_name == "computer_snapshot_display" {
        result
            .output
            .get("display_id")
            .and_then(Value::as_str)
            .unwrap_or("full display")
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
    let metadata_text = if matches!(tool_name, "computer_snapshot" | "computer_snapshot_display") {
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

    let snapshot_link = snapshot_caller
        .zip(McpSnapshotResourceKind::from_tool_name(tool_name))
        .map(|(caller, kind)| {
            let client_id = result
                .output
                .get("client_id")
                .and_then(Value::as_str)
                .unwrap_or("computer");
            let name = kind.name(client_id, &mime_type);
            let record = McpSnapshotResourceRecord {
                caller,
                kind,
                bytes: Arc::from(decoded.into_boxed_slice()),
                mime_type: mime_type.clone(),
                expires_at: Instant::now() + MCP_SNAPSHOT_RESOURCE_TTL,
            };
            let uri = mcp_snapshot_resource_registry()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(record);
            tracing::info!(
                target: "webcodex::mcp",
                tool_name,
                file_bytes,
                "mcp_snapshot_resource_link_issued"
            );
            json!({
                "type": "resource_link",
                "uri": uri,
                "name": name,
                "mimeType": mime_type,
                "size": file_bytes,
                "description": "Short-lived authenticated WebCodex computer screenshot. No project artifact was created."
            })
        });
    let mut content = Vec::with_capacity(if snapshot_link.is_some() { 3 } else { 2 });
    if let Some(link) = snapshot_link {
        content.push(link);
    }
    content.push(json!({ "type": "text", "text": metadata_text }));
    content.push(json!({ "type": "image", "data": data, "mimeType": mime_type }));

    Ok(json!({
        "content": content,
        "structuredContent": {
            "success": true,
            "output": structured_output,
            "error": Value::Null,
        },
        "isError": false
    }))
}

#[derive(Debug)]
enum McpArtifactExportReadError {
    Unavailable,
    Forbidden {
        required_scope: Option<&'static str>,
        description: String,
    },
    SnapshotChanged,
    Unsafe,
    Busy,
    Timeout,
}

fn mcp_artifact_export_lookup(
    uri: &str,
    auth: Option<&AuthContext>,
) -> Result<McpArtifactExportRecord, McpArtifactExportReadError> {
    let caller = mcp_artifact_export_caller_binding(auth)
        .map_err(|_| McpArtifactExportReadError::Unavailable)?;
    mcp_artifact_export_registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get_for_caller(uri, &caller)
        .ok_or(McpArtifactExportReadError::Unavailable)
}

async fn mcp_artifact_export_metadata_recheck(
    runtime: &ToolRuntime,
    record: &McpArtifactExportRecord,
    auth: Option<&AuthContext>,
) -> Result<ProjectArtifactExportSnapshot, McpArtifactExportReadError> {
    let result = runtime
        .read_project_artifact_export_metadata_internal(
            &record.project,
            &record.snapshot.path,
            auth,
        )
        .await;
    if !result.success {
        return Err(McpArtifactExportReadError::Unavailable);
    }
    let snapshot = validate_project_artifact_export_snapshot(&record.snapshot.path, &result.output)
        .map_err(|_| McpArtifactExportReadError::Unsafe)?;
    if snapshot != record.snapshot {
        return Err(McpArtifactExportReadError::SnapshotChanged);
    }
    Ok(snapshot)
}

fn mcp_artifact_export_decode_chunk(
    record: &McpArtifactExportRecord,
    offset: usize,
    length: usize,
    output: &Value,
    require_complete_metadata: bool,
) -> Result<Vec<u8>, McpArtifactExportReadError> {
    if output.get("error_kind").and_then(Value::as_str) == Some("snapshot_changed") {
        return Err(McpArtifactExportReadError::SnapshotChanged);
    }
    if output.get("error").and_then(Value::as_str).is_some() {
        return Err(McpArtifactExportReadError::Unsafe);
    }
    if output.get("path").and_then(Value::as_str) != Some(record.snapshot.path.as_str())
        || output.get("file_bytes").and_then(Value::as_u64) != Some(record.snapshot.bytes as u64)
    {
        return Err(McpArtifactExportReadError::SnapshotChanged);
    }
    if require_complete_metadata
        && (output.get("mime_type").and_then(Value::as_str)
            != Some(record.snapshot.mime_type.as_str())
            || output.get("sha256").and_then(Value::as_str)
                != Some(record.snapshot.sha256.as_str()))
    {
        return Err(McpArtifactExportReadError::SnapshotChanged);
    }
    if output.get("offset").and_then(Value::as_u64) != Some(offset as u64) {
        return Err(McpArtifactExportReadError::Unsafe);
    }
    let encoded = output
        .get("content_base64")
        .and_then(Value::as_str)
        .ok_or(McpArtifactExportReadError::Unsafe)?;
    let decoded = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| McpArtifactExportReadError::Unsafe)?;
    let bytes_returned = output
        .get("bytes_returned")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(McpArtifactExportReadError::Unsafe)?;
    let next_offset = output
        .get("next_offset")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(McpArtifactExportReadError::Unsafe)?;
    let eof = output
        .get("eof")
        .and_then(Value::as_bool)
        .ok_or(McpArtifactExportReadError::Unsafe)?;
    let truncated = output
        .get("truncated")
        .and_then(Value::as_bool)
        .ok_or(McpArtifactExportReadError::Unsafe)?;
    let expected_next = offset
        .checked_add(decoded.len())
        .ok_or(McpArtifactExportReadError::Unsafe)?;
    if decoded.len() != bytes_returned
        || decoded.len() > length
        || expected_next != next_offset
        || next_offset > record.snapshot.bytes
        || (decoded.is_empty() && offset < record.snapshot.bytes)
        || eof != (next_offset == record.snapshot.bytes)
        || truncated == eof
    {
        return Err(McpArtifactExportReadError::Unsafe);
    }
    Ok(decoded)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpArtifactExportChunkRoute {
    Optimized,
    Legacy,
}

async fn mcp_artifact_export_read_optimized_chunk(
    runtime: &ToolRuntime,
    record: &McpArtifactExportRecord,
    auth: Option<&AuthContext>,
    offset: usize,
    length: usize,
) -> Result<Option<Vec<u8>>, McpArtifactExportReadError> {
    match runtime
        .read_project_artifact_export_chunk_internal(
            &record.project,
            &record.snapshot.path,
            record.snapshot.bytes,
            offset,
            length,
            auth,
        )
        .await
    {
        Ok(Some(output)) => {
            mcp_artifact_export_decode_chunk(record, offset, length, &output, false).map(Some)
        }
        Ok(None) => Ok(None),
        Err(_) => Err(McpArtifactExportReadError::Unavailable),
    }
}

async fn mcp_artifact_export_read_legacy_chunk(
    runtime: &ToolRuntime,
    record: &McpArtifactExportRecord,
    auth: Option<&AuthContext>,
    offset: usize,
    length: usize,
) -> Result<Vec<u8>, McpArtifactExportReadError> {
    let outcome = runtime
        .call_tool_with_context(
            KernelToolCallRequest {
                tool_name: "read_project_artifact".to_string(),
                arguments: json!({
                    "project": record.project,
                    "path": record.snapshot.path,
                    "encoding": "base64",
                    "offset": offset,
                    "length": length,
                    "max_bytes": length,
                }),
            },
            ToolCallContext {
                transport: ToolTransport::Mcp,
                session_id: None,
                auth,
                window: None,
                record_oauth_scope_denials: false,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    if let Some(error_status) = outcome.error_status {
        return match error_status {
            ToolCallErrorStatus::InsufficientScope {
                required_scope,
                description,
            } => Err(McpArtifactExportReadError::Forbidden {
                required_scope,
                description,
            }),
            ToolCallErrorStatus::InvalidArguments { .. } => Err(McpArtifactExportReadError::Unsafe),
        };
    }
    let result = outcome.result.ok_or(McpArtifactExportReadError::Unsafe)?;
    if !result.success {
        return Err(McpArtifactExportReadError::Unavailable);
    }
    mcp_artifact_export_decode_chunk(record, offset, length, &result.output, true)
}

async fn mcp_artifact_export_read_chunk(
    runtime: &ToolRuntime,
    record: &McpArtifactExportRecord,
    auth: Option<&AuthContext>,
    offset: usize,
    length: usize,
) -> Result<(Vec<u8>, McpArtifactExportChunkRoute), McpArtifactExportReadError> {
    if let Some(chunk) =
        mcp_artifact_export_read_optimized_chunk(runtime, record, auth, offset, length).await?
    {
        return Ok((chunk, McpArtifactExportChunkRoute::Optimized));
    }

    // Rolling-upgrade compatibility: an old Runner cannot receive the optimized
    // request kind because capability check + enqueue are atomic. Observe that
    // route once on the first chunk; the resource read then stays sequential on
    // this public compatibility path rather than amplifying legacy whole-file
    // work with Control-side concurrency.
    let chunk =
        mcp_artifact_export_read_legacy_chunk(runtime, record, auth, offset, length).await?;
    Ok((chunk, McpArtifactExportChunkRoute::Legacy))
}

#[derive(Debug)]
struct McpArtifactExportStreamPlan {
    uri: String,
    record: McpArtifactExportRecord,
    first_chunk: Vec<u8>,
    route: Option<McpArtifactExportChunkRoute>,
    offset: usize,
    chunks: usize,
    max_chunks: usize,
    read_budget: Duration,
    _permit: OwnedSemaphorePermit,
}

async fn mcp_artifact_export_with_read_budget<T, F>(
    runtime: &ToolRuntime,
    read_budget: &mut Duration,
    future: F,
) -> Result<T, McpArtifactExportReadError>
where
    F: Future<Output = Result<T, McpArtifactExportReadError>>,
{
    if read_budget.is_zero() {
        return Err(McpArtifactExportReadError::Timeout);
    }
    let started = Instant::now();
    let outcome = tokio::time::timeout(*read_budget, future).await;
    *read_budget = read_budget.saturating_sub(started.elapsed());
    match outcome {
        Ok(result) => result,
        Err(_) => {
            runtime.shell_clients.cancel_abandoned_sync_requests().await;
            Err(McpArtifactExportReadError::Timeout)
        }
    }
}

async fn mcp_artifact_export_stream_plan_with_gate_timeout(
    runtime: &ToolRuntime,
    uri: &str,
    auth: Option<&AuthContext>,
    gate: Arc<Semaphore>,
    admission_timeout: Duration,
    read_timeout: Duration,
) -> Result<McpArtifactExportStreamPlan, McpArtifactExportReadError> {
    let record = mcp_artifact_export_lookup(uri, auth)?;
    if auth.is_some_and(|auth| !auth.has_scope(crate::auth::SCOPE_PROJECT_READ)) {
        return Err(McpArtifactExportReadError::Forbidden {
            required_scope: Some(crate::auth::SCOPE_PROJECT_READ),
            description: format!(
                "missing required scope: {}",
                crate::auth::SCOPE_PROJECT_READ
            ),
        });
    }
    let permit = match tokio::time::timeout(admission_timeout, gate.acquire_owned()).await {
        Ok(Ok(permit)) => permit,
        Ok(Err(_)) | Err(_) => return Err(McpArtifactExportReadError::Busy),
    };
    let mut read_budget = read_timeout;
    let snapshot = mcp_artifact_export_with_read_budget(
        runtime,
        &mut read_budget,
        mcp_artifact_export_metadata_recheck(runtime, &record, auth),
    )
    .await?;
    let max_chunks = MAX_PROJECT_ARTIFACT_EXPORT_BYTES
        .div_ceil(MAX_READ_PROJECT_ARTIFACT_LENGTH)
        .saturating_add(1);
    let mut first_chunk = Vec::new();
    let mut route = None;
    let mut offset = 0usize;
    let mut chunks = 0usize;
    if snapshot.bytes > 0 {
        let length = snapshot.bytes.min(MAX_READ_PROJECT_ARTIFACT_LENGTH);
        let (chunk, first_route) = mcp_artifact_export_with_read_budget(
            runtime,
            &mut read_budget,
            mcp_artifact_export_read_chunk(runtime, &record, auth, 0, length),
        )
        .await?;
        offset = chunk.len();
        if offset == 0 || offset > snapshot.bytes {
            return Err(McpArtifactExportReadError::Unsafe);
        }
        chunks = 1;
        first_chunk = chunk;
        route = Some(first_route);
    }
    Ok(McpArtifactExportStreamPlan {
        uri: uri.to_string(),
        record,
        first_chunk,
        route,
        offset,
        chunks,
        max_chunks,
        read_budget,
        _permit: permit,
    })
}

async fn mcp_artifact_export_stream_plan(
    runtime: &ToolRuntime,
    uri: &str,
    auth: Option<&AuthContext>,
) -> Result<McpArtifactExportStreamPlan, McpArtifactExportReadError> {
    mcp_artifact_export_stream_plan_with_gate_timeout(
        runtime,
        uri,
        auth,
        mcp_artifact_export_read_semaphore(),
        MCP_ARTIFACT_EXPORT_ADMISSION_TIMEOUT,
        MCP_ARTIFACT_EXPORT_READ_TIMEOUT,
    )
    .await
}

#[derive(Default)]
struct McpArtifactExportBase64Encoder {
    carry: [u8; 3],
    carry_len: usize,
}

impl McpArtifactExportBase64Encoder {
    fn push(&mut self, bytes: &[u8]) -> String {
        let mut output = String::with_capacity(((bytes.len() + 2) / 3) * 4 + 4);
        let mut index = 0usize;
        if self.carry_len > 0 {
            let needed = 3 - self.carry_len;
            let take = needed.min(bytes.len());
            self.carry[self.carry_len..self.carry_len + take].copy_from_slice(&bytes[..take]);
            self.carry_len += take;
            index += take;
            if self.carry_len == 3 {
                general_purpose::STANDARD.encode_string(&self.carry, &mut output);
                self.carry_len = 0;
            }
        }
        let remaining = &bytes[index..];
        let aligned_len = (remaining.len() / 3) * 3;
        if aligned_len > 0 {
            general_purpose::STANDARD.encode_string(&remaining[..aligned_len], &mut output);
        }
        let tail = &remaining[aligned_len..];
        if !tail.is_empty() {
            self.carry[..tail.len()].copy_from_slice(tail);
            self.carry_len = tail.len();
        }
        output
    }

    fn finish(&mut self) -> String {
        if self.carry_len == 0 {
            return String::new();
        }
        let output = general_purpose::STANDARD.encode(&self.carry[..self.carry_len]);
        self.carry_len = 0;
        output
    }
}

type McpArtifactExportStreamFrame = Result<Vec<u8>, std::io::Error>;

fn mcp_artifact_export_stream_prefix(
    id: &Value,
    uri: &str,
    mime_type: &str,
) -> Result<Vec<u8>, McpArtifactExportReadError> {
    let mut output = Vec::with_capacity(256);
    output.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":");
    output.extend_from_slice(
        &serde_json::to_vec(id).map_err(|_| McpArtifactExportReadError::Unsafe)?,
    );
    output.extend_from_slice(b",\"result\":{\"contents\":[{\"uri\":");
    output.extend_from_slice(
        &serde_json::to_vec(uri).map_err(|_| McpArtifactExportReadError::Unsafe)?,
    );
    output.extend_from_slice(b",\"mimeType\":");
    output.extend_from_slice(
        &serde_json::to_vec(mime_type).map_err(|_| McpArtifactExportReadError::Unsafe)?,
    );
    output.extend_from_slice(b",\"blob\":\"");
    Ok(output)
}

fn mcp_artifact_export_stream_suffix() -> Result<Vec<u8>, McpArtifactExportReadError> {
    let mut output = b"\"}],\"resultType\":\"complete\",\"_meta\":{\"io.modelcontextprotocol/serverInfo\":{\"name\":\"webcodex\",\"version\":".to_vec();
    output.extend_from_slice(
        &serde_json::to_vec(env!("CARGO_PKG_VERSION"))
            .map_err(|_| McpArtifactExportReadError::Unsafe)?,
    );
    output.extend_from_slice(b"}}}}");
    Ok(output)
}

async fn mcp_artifact_export_send_frame(
    sender: &mpsc::Sender<McpArtifactExportStreamFrame>,
    frame: Vec<u8>,
) -> Result<(), McpArtifactExportReadError> {
    sender
        .send(Ok(frame))
        .await
        .map_err(|_| McpArtifactExportReadError::Unavailable)
}

async fn mcp_artifact_export_emit_chunk(
    sender: &mpsc::Sender<McpArtifactExportStreamFrame>,
    encoder: &mut McpArtifactExportBase64Encoder,
    sha256: &mut Sha256,
    emitted_bytes: &mut usize,
    expected_bytes: usize,
    chunk: &[u8],
) -> Result<(), McpArtifactExportReadError> {
    *emitted_bytes = emitted_bytes
        .checked_add(chunk.len())
        .ok_or(McpArtifactExportReadError::Unsafe)?;
    if *emitted_bytes > expected_bytes {
        return Err(McpArtifactExportReadError::Unsafe);
    }
    sha256.update(chunk);
    let encoded = encoder.push(chunk);
    if !encoded.is_empty() {
        mcp_artifact_export_send_frame(sender, encoded.into_bytes()).await?;
    }
    Ok(())
}

async fn mcp_artifact_export_stream_transfer(
    runtime: &ToolRuntime,
    id: &Value,
    auth: Option<&AuthContext>,
    mut plan: McpArtifactExportStreamPlan,
    sender: mpsc::Sender<McpArtifactExportStreamFrame>,
) -> Result<(), McpArtifactExportReadError> {
    let snapshot = plan.record.snapshot.clone();
    mcp_artifact_export_send_frame(
        &sender,
        mcp_artifact_export_stream_prefix(id, &plan.uri, &snapshot.mime_type)?,
    )
    .await?;

    let mut encoder = McpArtifactExportBase64Encoder::default();
    let mut sha256 = Sha256::new();
    let mut emitted_bytes = 0usize;
    if !plan.first_chunk.is_empty() {
        let first_chunk = std::mem::take(&mut plan.first_chunk);
        mcp_artifact_export_emit_chunk(
            &sender,
            &mut encoder,
            &mut sha256,
            &mut emitted_bytes,
            snapshot.bytes,
            &first_chunk,
        )
        .await?;
    }
    if emitted_bytes != plan.offset {
        return Err(McpArtifactExportReadError::Unsafe);
    }

    match plan.route {
        Some(McpArtifactExportChunkRoute::Legacy) => {
            while plan.offset < snapshot.bytes {
                if plan.chunks >= plan.max_chunks {
                    return Err(McpArtifactExportReadError::Unsafe);
                }
                plan.chunks = plan.chunks.saturating_add(1);
                let length = (snapshot.bytes - plan.offset).min(MAX_READ_PROJECT_ARTIFACT_LENGTH);
                let chunk = mcp_artifact_export_with_read_budget(
                    runtime,
                    &mut plan.read_budget,
                    mcp_artifact_export_read_legacy_chunk(
                        runtime,
                        &plan.record,
                        auth,
                        plan.offset,
                        length,
                    ),
                )
                .await?;
                plan.offset = plan
                    .offset
                    .checked_add(chunk.len())
                    .ok_or(McpArtifactExportReadError::Unsafe)?;
                mcp_artifact_export_emit_chunk(
                    &sender,
                    &mut encoder,
                    &mut sha256,
                    &mut emitted_bytes,
                    snapshot.bytes,
                    &chunk,
                )
                .await?;
            }
        }
        Some(McpArtifactExportChunkRoute::Optimized) => {
            while plan.offset < snapshot.bytes {
                let mut batch = Vec::with_capacity(MAX_MCP_ARTIFACT_EXPORT_CHUNK_READS);
                let mut batch_offset = plan.offset;
                while batch.len() < MAX_MCP_ARTIFACT_EXPORT_CHUNK_READS
                    && batch_offset < snapshot.bytes
                {
                    if plan.chunks >= plan.max_chunks {
                        return Err(McpArtifactExportReadError::Unsafe);
                    }
                    plan.chunks = plan.chunks.saturating_add(1);
                    let length =
                        (snapshot.bytes - batch_offset).min(MAX_READ_PROJECT_ARTIFACT_LENGTH);
                    batch.push((batch_offset, length));
                    batch_offset = batch_offset
                        .checked_add(length)
                        .ok_or(McpArtifactExportReadError::Unsafe)?;
                }
                let runtime_ref = runtime;
                let record = &plan.record;
                let results =
                    mcp_artifact_export_with_read_budget(runtime, &mut plan.read_budget, async {
                        Ok(
                            join_all(batch.iter().map(|&(batch_offset, length)| async move {
                                mcp_artifact_export_read_optimized_chunk(
                                    runtime_ref,
                                    record,
                                    auth,
                                    batch_offset,
                                    length,
                                )
                                .await
                            }))
                            .await,
                        )
                    })
                    .await?;

                // Drain the full bounded batch before surfacing an offset-ordered
                // error. This preserves the existing no-abandoned-request rule.
                for ((requested_offset, _), result) in batch.into_iter().zip(results) {
                    if requested_offset != plan.offset {
                        return Err(McpArtifactExportReadError::Unsafe);
                    }
                    let chunk = result?.ok_or(McpArtifactExportReadError::Unavailable)?;
                    plan.offset = plan
                        .offset
                        .checked_add(chunk.len())
                        .ok_or(McpArtifactExportReadError::Unsafe)?;
                    mcp_artifact_export_emit_chunk(
                        &sender,
                        &mut encoder,
                        &mut sha256,
                        &mut emitted_bytes,
                        snapshot.bytes,
                        &chunk,
                    )
                    .await?;
                }
            }
        }
        None => {}
    }

    if emitted_bytes != snapshot.bytes || plan.offset != snapshot.bytes {
        return Err(McpArtifactExportReadError::Unsafe);
    }
    let final_sha256 = format!("{:x}", sha256.finalize());
    if final_sha256 != snapshot.sha256 {
        // Bytes may already have reached the HTTP peer. Fail closed by never
        // emitting the base64 tail or closing JSON suffix, so a changed file
        // cannot become a syntactically valid successful MCP resource result.
        return Err(McpArtifactExportReadError::SnapshotChanged);
    }
    let tail = encoder.finish();
    if !tail.is_empty() {
        mcp_artifact_export_send_frame(&sender, tail.into_bytes()).await?;
    }
    mcp_artifact_export_send_frame(&sender, mcp_artifact_export_stream_suffix()?).await?;
    Ok(())
}

fn mcp_artifact_export_stream_io_error(error: &McpArtifactExportReadError) -> std::io::Error {
    let message = match error {
        McpArtifactExportReadError::SnapshotChanged => {
            "artifact export stream failed snapshot integrity validation"
        }
        McpArtifactExportReadError::Timeout => "artifact export stream timed out",
        _ => "artifact export stream failed",
    };
    std::io::Error::other(message)
}

#[cfg(test)]
async fn mcp_artifact_export_collect_stream_response(
    runtime: &ToolRuntime,
    id: &Value,
    auth: Option<&AuthContext>,
    plan: McpArtifactExportStreamPlan,
) -> Result<Value, McpArtifactExportReadError> {
    let (sender, mut receiver) = mpsc::channel::<McpArtifactExportStreamFrame>(1);
    let transfer = mcp_artifact_export_stream_transfer(runtime, id, auth, plan, sender);
    let collect = async {
        let mut body = Vec::new();
        while let Some(frame) = receiver.recv().await {
            let frame = frame.map_err(|_| McpArtifactExportReadError::Unsafe)?;
            body.extend_from_slice(&frame);
        }
        Ok::<Vec<u8>, McpArtifactExportReadError>(body)
    };
    let (transfer_result, body_result) = tokio::join!(transfer, collect);
    transfer_result?;
    serde_json::from_slice(&body_result?).map_err(|_| McpArtifactExportReadError::Unsafe)
}

#[cfg(test)]
async fn mcp_artifact_export_resource_read_with_gate_timeout(
    runtime: &ToolRuntime,
    uri: &str,
    auth: Option<&AuthContext>,
    gate: Arc<Semaphore>,
    admission_timeout: Duration,
    read_timeout: Duration,
) -> Result<Value, McpArtifactExportReadError> {
    let plan = mcp_artifact_export_stream_plan_with_gate_timeout(
        runtime,
        uri,
        auth,
        gate,
        admission_timeout,
        read_timeout,
    )
    .await?;
    let response =
        mcp_artifact_export_collect_stream_response(runtime, &Value::Null, auth, plan).await?;
    response
        .get("result")
        .cloned()
        .ok_or(McpArtifactExportReadError::Unsafe)
}

#[cfg(test)]
async fn mcp_artifact_export_resource_read_with_gate(
    runtime: &ToolRuntime,
    uri: &str,
    auth: Option<&AuthContext>,
    gate: Arc<Semaphore>,
    admission_timeout: Duration,
) -> Result<Value, McpArtifactExportReadError> {
    mcp_artifact_export_resource_read_with_gate_timeout(
        runtime,
        uri,
        auth,
        gate,
        admission_timeout,
        MCP_ARTIFACT_EXPORT_READ_TIMEOUT,
    )
    .await
}

fn mcp_artifact_export_read_error_outcome(
    id: Option<Value>,
    auth: Option<&AuthContext>,
    error: McpArtifactExportReadError,
) -> McpOutcome {
    match error {
        McpArtifactExportReadError::Forbidden {
            required_scope,
            description,
        } => scope_forbidden(auth, required_scope, description),
        McpArtifactExportReadError::Unavailable => McpOutcome::BadRequest(rpc_error(
            id,
            -32602,
            "Artifact export resource is unavailable",
        )),
        McpArtifactExportReadError::SnapshotChanged => McpOutcome::BadRequest(rpc_error(
            id,
            -32602,
            "Exported artifact no longer matches its snapshot",
        )),
        McpArtifactExportReadError::Unsafe => McpOutcome::BadRequest(rpc_error(
            id,
            -32603,
            "Artifact export resource failed bounded safety validation",
        )),
        McpArtifactExportReadError::Busy => McpOutcome::BadRequest(rpc_error(
            id,
            MCP_ARTIFACT_EXPORT_BUSY_CODE,
            "Artifact export is temporarily busy; retry later",
        )),
        McpArtifactExportReadError::Timeout => McpOutcome::BadRequest(rpc_error(
            id,
            -32603,
            "Artifact export resource read timed out",
        )),
    }
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
        plan: McpArtifactExportStreamPlan,
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
            .filter(|uri| is_mcp_computer_app_resource_uri(uri))
            .map(str::to_string)
    } else {
        None
    };
    let computer_app_ui_capability_present = request_supports_mcp_apps(&request.params);
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
            );
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(200, estimated, Some(true), tool_success, "ok");
            res.render(Json(body));
            guard.handler_returned(200, estimated, Some(true), tool_success, "ok");
        }
        McpOutcome::ArtifactExportStream { id, plan } => {
            record_audit(true, StatusCode::OK, None);
            guard.response_serialized(200, None, Some(true), None, "artifact_export_stream");
            res.status_code(StatusCode::OK);
            let _ = res.add_header("content-type", "application/json", true);
            let (sender, receiver) = mpsc::channel::<McpArtifactExportStreamFrame>(1);
            let error_sender = sender.clone();
            let stream_runtime = runtime.clone();
            let stream_auth = auth.clone();
            tokio::spawn(async move {
                let transfer = mcp_artifact_export_stream_transfer(
                    &stream_runtime,
                    &id,
                    stream_auth.as_ref(),
                    plan,
                    sender,
                );
                match tokio::time::timeout(MCP_ARTIFACT_EXPORT_STREAM_TIMEOUT, transfer).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        let _ = error_sender
                            .send(Err(mcp_artifact_export_stream_io_error(&error)))
                            .await;
                    }
                    Err(_) => {
                        stream_runtime
                            .shell_clients
                            .cancel_abandoned_sync_requests()
                            .await;
                        let _ = error_sender
                            .send(Err(std::io::Error::other(
                                "artifact export stream exceeded bounded transfer timeout",
                            )))
                            .await;
                    }
                }
            });
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
    )
    .await;
    match outcome {
        McpOutcome::ArtifactExportStream { id, plan } => {
            match mcp_artifact_export_collect_stream_response(runtime, &id, auth, plan).await {
                Ok(body) => McpOutcome::Ok(body),
                Err(error) => mcp_artifact_export_read_error_outcome(Some(id), auth, error),
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
) -> McpOutcome {
    let stateless_2026 = protocol_era == McpProtocolEra::Stateless2026;
    let artifact_export_resource_read = stateless_2026
        && request.method == "resources/read"
        && request
            .params
            .get("uri")
            .and_then(Value::as_str)
            .is_some_and(|uri| uri.starts_with(MCP_ARTIFACT_EXPORT_URI_PREFIX));
    let snapshot_resource_read = stateless_2026
        && request.method == "resources/read"
        && request
            .params
            .get("uri")
            .and_then(Value::as_str)
            .is_some_and(|uri| uri.starts_with(MCP_SNAPSHOT_RESOURCE_URI_PREFIX));
    let mcp_app_enabled = stateless_2026
        && model_surface_supports_computer_app(runtime.model_surface())
        && request_supports_mcp_apps(&request.params);

    if auth.is_some()
        && (matches!(
            request.method.as_str(),
            "server/discover" | "tools/list" | "resources/list"
        ) || (request.method == "resources/read"
            && !artifact_export_resource_read
            && !snapshot_resource_read)
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
                        model_surface_supports_computer_app(runtime.model_surface()),
                        true,
                        auth,
                    )
                } else {
                    mcp_tools_list_payload_with_compact_and_app(
                        runtime.model_surface(),
                        crate::config::mcp_compact_schemas_enabled(),
                        model_surface_supports_computer_app(runtime.model_surface()),
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
            let result = if mcp_app_enabled {
                mcp_computer_app_resources_list()
            } else {
                json!({ "resources": [] })
            };
            rpc_result(id, mcp_stateless_result(result, true))
        }
        "resources/read" if stateless_2026 => {
            let Some(uri) = request.params.get("uri").and_then(Value::as_str) else {
                return McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    "Invalid params: uri is required",
                ));
            };
            if uri.starts_with(MCP_ARTIFACT_EXPORT_URI_PREFIX) {
                let response_id = id.clone().unwrap_or(Value::Null);
                let plan = match mcp_artifact_export_stream_plan(runtime, uri, auth).await {
                    Ok(plan) => plan,
                    Err(error) => return mcp_artifact_export_read_error_outcome(id, auth, error),
                };
                return McpOutcome::ArtifactExportStream {
                    id: response_id,
                    plan,
                };
            }
            if uri.starts_with(MCP_SNAPSHOT_RESOURCE_URI_PREFIX) {
                let caller = match mcp_artifact_export_caller_binding(auth) {
                    Ok(caller) => caller,
                    Err(_) => {
                        return McpOutcome::BadRequest(rpc_error(
                            id,
                            -32602,
                            format!("Resource not found: {uri}"),
                        ))
                    }
                };
                let record = mcp_snapshot_resource_registry()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get_for_caller(uri, &caller);
                let Some(record) = record else {
                    return McpOutcome::BadRequest(rpc_error(
                        id,
                        -32602,
                        format!("Resource not found: {uri}"),
                    ));
                };
                for scope in match record.kind {
                    McpSnapshotResourceKind::Window => &[crate::auth::SCOPE_COMPUTER_READ][..],
                    McpSnapshotResourceKind::Display => &[
                        crate::auth::SCOPE_COMPUTER_READ,
                        crate::auth::SCOPE_COMPUTER_DISPLAY_READ,
                    ][..],
                } {
                    if let Some(outcome) = require_mcp_scope(auth, scope) {
                        return outcome;
                    }
                }
                tracing::info!(
                    target: "webcodex::mcp",
                    resource_kind = ?record.kind,
                    file_bytes = record.bytes.len(),
                    "mcp_snapshot_resource_read"
                );
                let result = json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": record.mime_type,
                        "blob": general_purpose::STANDARD.encode(record.bytes.as_ref())
                    }]
                });
                return McpOutcome::Ok(rpc_result(id, mcp_stateless_result(result, true)));
            }
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
            let Some(result) = mcp_computer_app_resource_read(uri) else {
                return McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    format!("Resource not found: {uri}"),
                ));
            };
            let mut result = mcp_stateless_result(result, true);
            // The canonical URI uses the current delivery TTL policy (temporarily
            // zero during gray-card diagnosis). Hidden legacy aliases always stay
            // zero-TTL because they intentionally serve the current HTML.
            if uri == MCP_COMPUTER_UI_RESOURCE_URI {
                result["ttlMs"] = Value::from(MCP_COMPUTER_UI_RESOURCE_TTL_MS);
            }
            rpc_result(id, result)
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
            if params.name == crate::mcp_gateway::MCP_TOOL_NAME {
                if let Some(outcome) = require_mcp_scope(auth, crate::auth::SCOPE_MCP_LOCAL) {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("forbidden");
                        lc.dispatch_finished(false, Some(false), "forbidden");
                    }
                    return outcome;
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
            let artifact_export_caller = if params.name == "export_project_artifact" {
                if !stateless_2026 || runtime.model_surface() != ModelSurface::FullOperatorRuntime {
                    return McpOutcome::BadRequest(rpc_error(
                        id,
                        -32602,
                        "export_project_artifact requires the stateless-2026 full-operator MCP surface",
                    ));
                }
                match mcp_artifact_export_caller_binding(auth) {
                    Ok(caller) => Some(caller),
                    Err(error) => {
                        return McpOutcome::BadRequest(rpc_error(
                            id,
                            -32602,
                            format!("export_project_artifact cannot bind this caller: {error}"),
                        ))
                    }
                }
            } else {
                None
            };
            let snapshot_resource_caller = if stateless_2026
                && runtime.model_surface() == ModelSurface::FullOperatorRuntime
                && matches!(
                    params.name.as_str(),
                    "computer_snapshot" | "computer_snapshot_display"
                ) {
                mcp_artifact_export_caller_binding(auth).ok()
            } else {
                None
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
            let session_id = match strip_reserved_session_id(&mut params.arguments) {
                Ok(session_id) => session_id,
                Err(message) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("invalid_arguments");
                        lc.dispatch_finished(false, Some(false), "invalid_arguments");
                    }
                    return McpOutcome::BadRequest(rpc_error(id, -32602, message));
                }
            };
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
                    return scope_forbidden(auth, required_scope, description);
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
            let result = if params.name == "export_project_artifact" {
                mcp_artifact_export_tool_result(
                    result,
                    artifact_export_caller.expect("validated artifact export caller binding"),
                )
            } else {
                mcp_runtime_tool_result_with_snapshot_resource(
                    &params.name,
                    as_image_requested,
                    result,
                    snapshot_resource_caller,
                )
            };
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

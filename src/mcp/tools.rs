use super::resources;
use super::response::{
    connector_call_tool_result, mcp_runtime_tool_result_fallback, mcp_stateless_result, rpc_error,
    rpc_result,
};
use super::tasks;
use super::{require_mcp_scope, scope_forbidden, McpOutcome};
use crate::auth::AuthContext;
use crate::connector_runtime::{ConnectorRuntime, ConnectorTransport};
use crate::model_surface::ModelSurface;
use crate::tool_request_trace::ToolRequestLifecycle;
use crate::tool_runtime::kernel::{
    check_runtime_tool_scope, HostFileImportTrust, ToolCallContext, ToolCallErrorStatus,
    ToolCallRequest as KernelToolCallRequest, ToolProtocolCapabilities, ToolTransport,
};
use crate::tool_runtime::model_ergonomics_telemetry::{
    ModelErgonomicsRecord, ModelErgonomicsTimer,
};
use crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES;
#[cfg(test)]
use crate::tool_runtime::ToolResult;
use crate::tool_runtime::{registered_tool_specs, ToolRuntime, ToolSpec};
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) const MCP_RESERVED_SESSION_ID_FIELD: &str = "_session_id";

#[derive(Debug, Deserialize)]
pub(super) struct McpToolCallParams {
    pub(super) name: String,
    #[serde(default)]
    pub(super) arguments: Value,
}

pub(super) fn tool_name_from_params(params: &Value) -> Option<String> {
    params
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub(super) fn project_from_tool_call_params(params: &Value) -> Option<String> {
    params["arguments"]["project"].as_str().map(str::to_string)
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
pub(super) fn mcp_tools_list_payload_with_compact(
    model_surface: ModelSurface,
    compact: bool,
) -> Value {
    mcp_tools_list_payload_with_features(model_surface, compact, false, false)
}

pub(super) fn mcp_tools_list_payload_with_compact_and_app(
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

fn mcp_context_projection_output_schema() -> Value {
    json!({
        "type": "object",
        "description": "Optional bounded post-tool context sidecar. It describes material projected after the main effect/observation and never grants authority or retroactively governs that effect.",
        "properties": {
            "timing": {"type": "string", "const": "post_tool"},
            "applies_to_current_effect": {"type": "boolean", "const": false},
            "materials": {
                "type": "array",
                "maxItems": crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_ITEMS,
                "items": {
                    "type": "object",
                    "properties": {
                        "key": {"type": "string", "maxLength": crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_KEY_CHARS},
                        "status": {"type": "string", "enum": ["available", "unavailable", "unsupported"]},
                        "reason_code": {"type": "string"},
                        "projection": {}
                    },
                    "required": ["key", "status"],
                    "additionalProperties": false
                }
            },
            "truncated": {"type": "boolean"}
        },
        "required": ["timing", "applies_to_current_effect", "materials", "truncated"],
        "additionalProperties": false
    })
}

fn add_context_projection_to_output_shape(schema: &mut Value, projection_schema: &Value) {
    if schema.get("type").and_then(Value::as_str) == Some("object") {
        if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
            properties.insert("context_projection".to_string(), projection_schema.clone());
        }
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema.get_mut(keyword).and_then(Value::as_array_mut) {
            for branch in branches {
                add_context_projection_to_output_shape(branch, projection_schema);
            }
        }
    }
}

fn add_stateless_context_projection_output_schema(tool: &mut Value) {
    let Some(output_schema) = tool.get_mut("outputSchema") else {
        return;
    };
    let projection_schema = mcp_context_projection_output_schema();
    if let Some(output) = output_schema.pointer_mut("/properties/output") {
        add_context_projection_to_output_shape(output, &projection_schema);
    }
    if let Some(conditions) = output_schema.get_mut("allOf").and_then(Value::as_array_mut) {
        for condition in conditions {
            for branch_name in ["then", "else"] {
                if let Some(output) =
                    condition.pointer_mut(&format!("/{branch_name}/properties/output"))
                {
                    add_context_projection_to_output_shape(output, &projection_schema);
                }
            }
        }
    }
}

pub(super) fn add_stateless_memory_tools(
    payload: &mut Value,
    model_surface: ModelSurface,
    compact: bool,
    app_enabled: bool,
    auth: Option<&AuthContext>,
) {
    if !matches!(model_surface, ModelSurface::FullOperatorRuntime) {
        return;
    }
    let Some(tools) = payload.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };
    for spec in crate::tool_runtime::memory_runtime_tool_specs()
        .into_iter()
        .chain(crate::tool_runtime::memory_management_tool_specs())
        .filter(|spec| check_runtime_tool_scope(auth, &spec.name).is_ok())
    {
        tools.push(mcp_tool_spec_json(spec, compact, app_enabled));
    }
}

pub(super) fn add_stateless_skill_tools(
    payload: &mut Value,
    model_surface: ModelSurface,
    compact: bool,
    app_enabled: bool,
    auth: Option<&AuthContext>,
) {
    if !matches!(model_surface, ModelSurface::FullOperatorRuntime) {
        return;
    }
    let Some(tools) = payload.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };
    let oauth_scope_projection = auth.is_some_and(AuthContext::is_oauth_token);
    for spec in crate::tool_runtime::skill_runtime_tool_specs()
        .into_iter()
        .filter(|spec| {
            !oauth_scope_projection || check_runtime_tool_scope(auth, &spec.name).is_ok()
        })
    {
        tools.push(mcp_tool_spec_json(spec, compact, app_enabled));
    }
    // Runner-global store management is intentionally narrower than Skill read.
    // Absence of authentication is never operator authority, and project:write
    // or direct shared-key access does not imply admin.
    if auth.is_some_and(|auth| auth.has_scope(crate::auth::SCOPE_ADMIN)) {
        for spec in crate::tool_runtime::skill_management_tool_specs() {
            tools.push(mcp_tool_spec_json(spec, compact, app_enabled));
        }
    }
}

pub(super) fn add_stateless_workflow_recorder_metadata(
    payload: &mut Value,
    model_surface: ModelSurface,
) {
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
                crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD
                    .to_string(),
                json!({
                    "type": "array",
                    "maxItems": crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_ITEMS,
                    "items": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_KEY_CHARS,
                        "description": "Bounded context material key. Keys are open-ended rather than schema-enumerated; unsupported keys are reported nonfatally in context_projection."
                    },
                    "description": format!("MCP wrapper metadata only. Request bounded context material to be appended as context_projection after this tool's main effect/observation is already complete. Keys remain open-ended; current materials include {}. This sidecar grants no authority and does not retroactively make requested guidance a precondition of the current effect. If project rules or durable Memory guidance were lost, first recover project.instructions and/or memory.bootstrap on an observation call, use memory_read when detail is needed, reason, and only then issue a later mutation that must follow that guidance.", crate::tool_runtime::context_projection::context_material_keys_csv())
                }),
            );
            properties.insert(
                crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_FIELD
                    .to_string(),
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Echo the latest Session context revision still present in the current model context. Omit it when unknown. A known behind revision may receive bounded continuous delta recovery; missing, invalid, future, retained-history-loss, or bounded-delta truncation recovery uses a compact current Session handoff. The tool effect remains nonblocking."
                }),
            );
            add_stateless_context_projection_output_schema(tool);
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

#[cfg(test)]
pub(super) fn mcp_runtime_tool_result(
    tool_name: &str,
    as_image_requested: bool,
    result: ToolResult,
) -> Value {
    if tool_name == "export_project_artifact" {
        return mcp_runtime_tool_result_fallback(result);
    }
    match resources::adapt_tool_result(
        tool_name,
        as_image_requested,
        result,
        resources::McpResourceToolCallContext::default(),
    ) {
        resources::McpResourceToolResultAdaptation::Framed(value) => value,
        resources::McpResourceToolResultAdaptation::Unhandled(result) => {
            mcp_runtime_tool_result_fallback(result)
        }
    }
}

pub(super) fn handle_list(
    runtime: &ToolRuntime,
    id: Option<Value>,
    auth: Option<&AuthContext>,
    stateless_2026: bool,
) -> McpOutcome {
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
        add_stateless_skill_tools(
            &mut result,
            runtime.model_surface(),
            crate::config::mcp_compact_schemas_enabled(),
            resources::model_surface_supports_computer_app(runtime.model_surface()),
            auth,
        );
        add_stateless_memory_tools(
            &mut result,
            runtime.model_surface(),
            crate::config::mcp_compact_schemas_enabled(),
            resources::model_surface_supports_computer_app(runtime.model_surface()),
            auth,
        );
        add_stateless_workflow_recorder_metadata(&mut result, runtime.model_surface());
    }
    if crate::mcp_gateway::authorized(auth) {
        if let Some(tools) = result.get_mut("tools").and_then(Value::as_array_mut) {
            tools.push(crate::mcp_gateway::tool_spec());
        }
    }
    McpOutcome::Ok(rpc_result(
        id,
        if stateless_2026 {
            mcp_stateless_result(result, true)
        } else {
            result
        },
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HostFileImportTrustReason {
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
pub(super) struct HostFileImportTrustDecision {
    pub(super) trust: HostFileImportTrust,
    pub(super) reason: HostFileImportTrustReason,
    config_present: bool,
    database_present: bool,
    oauth_enabled: bool,
    configured_trusted_client_count: usize,
    pub(super) client_id_configured: Option<bool>,
    pub(super) active_client_registration_found: Option<bool>,
}

#[cfg(test)]
static LAST_MCP_HOST_FILE_IMPORT_TRUST_DECISION: std::sync::OnceLock<
    std::sync::Mutex<Option<HostFileImportTrustDecision>>,
> = std::sync::OnceLock::new();

#[cfg(test)]
pub(super) fn take_last_mcp_host_file_import_trust_decision() -> Option<HostFileImportTrustDecision>
{
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

pub(super) fn mcp_host_file_import_trust_decision_from_state(
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
pub(super) fn mcp_host_file_import_trust_from_state(
    config: &crate::Config,
    db: &crate::Database,
    auth: Option<&AuthContext>,
) -> HostFileImportTrust {
    mcp_host_file_import_trust_decision_from_state(config, db, auth).trust
}

pub(super) fn host_file_import_trust_for_call(
    tool_name: Option<&str>,
    auth: Option<&AuthContext>,
    config: Option<&crate::Config>,
    db: Option<&crate::Database>,
) -> HostFileImportTrust {
    if tool_name != Some("import_conversation_files_to_project") {
        return HostFileImportTrust::Untrusted;
    }
    let decision = match config {
        None => HostFileImportTrustDecision::unavailable(HostFileImportTrustReason::MissingConfig),
        Some(config) => match db {
            None => HostFileImportTrustDecision::from_config(
                HostFileImportTrustReason::MissingDatabase,
                config,
            ),
            Some(db) => mcp_host_file_import_trust_decision_from_state(config, db, auth),
        },
    };
    log_mcp_host_file_import_trust_decision(auth, &decision);
    decision.trust
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

pub(super) fn strip_reserved_session_id(arguments: &mut Value) -> Result<Option<String>, String> {
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

pub(super) fn strip_stateless_ack_session_message_ids(
    arguments: &mut Value,
) -> Result<Vec<String>, String> {
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

pub(super) fn strip_stateless_session_message_resolution(
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

pub(super) fn strip_stateless_context_request(
    arguments: &mut Value,
) -> Result<Vec<String>, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Ok(Vec::new());
    };
    let Some(value) =
        object.remove(crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD)
    else {
        return Ok(Vec::new());
    };
    let Value::Array(values) = value else {
        return Err(format!(
            "field '{}' must be an array of bounded context material keys",
            crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD
        ));
    };
    if values.len() > crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_ITEMS {
        return Err(format!(
            "field '{}' accepts at most {} context material keys",
            crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD,
            crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_ITEMS
        ));
    }
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = std::collections::HashSet::new();
    for value in values {
        let Value::String(value) = value else {
            return Err(format!(
                "field '{}' must contain only strings",
                crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD
            ));
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
                "field '{}' keys must be non-empty, at most {} characters, and contain no whitespace or control characters",
                crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD,
                crate::tool_runtime::context_projection::MAX_CONTEXT_REQUEST_KEY_CHARS
            ));
        }
        if seen.insert(key.to_string()) {
            normalized.push(key.to_string());
        }
    }
    Ok(normalized)
}

pub(super) fn strip_stateless_ack_session_context_revision(arguments: &mut Value) -> Option<Value> {
    arguments
        .as_object_mut()?
        .remove(crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_FIELD)
}

pub(super) async fn handle_call(
    runtime: &ToolRuntime,
    connector: Option<&ConnectorRuntime>,
    request_params: Value,
    id: Option<Value>,
    auth: Option<&AuthContext>,
    stateless_2026: bool,
    host_file_import_trust: HostFileImportTrust,
    window: Option<&crate::client_window::ClientWindow>,
    mut lifecycle: Option<&mut ToolRequestLifecycle>,
    mut model_ergonomics_out: Option<&mut Option<ModelErgonomicsRecord>>,
) -> McpOutcome {
    let tasks_extension_declared = stateless_2026 && tasks::request_supports_tasks(&request_params);
    let mut params: McpToolCallParams = match serde_json::from_value(request_params) {
        Ok(params) => params,
        Err(e) => {
            return McpOutcome::BadRequest(rpc_error(id, -32602, format!("Invalid params: {}", e)));
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
    let context_continuity_capable =
        stateless_2026 && matches!(runtime.model_surface(), ModelSurface::FullOperatorRuntime);
    // The sidecar is surface-scoped like the ACK today, but it is a
    // separate caller-explicit protocol and must not participate in
    // revision continuity bookkeeping.
    let context_sidecar_capable =
        stateless_2026 && matches!(runtime.model_surface(), ModelSurface::FullOperatorRuntime);
    let skill_runtime_capable =
        stateless_2026 && matches!(runtime.model_surface(), ModelSurface::FullOperatorRuntime);
    let skill_management_capable =
        stateless_2026 && matches!(runtime.model_surface(), ModelSurface::FullOperatorRuntime);
    let memory_surface_capable =
        stateless_2026 && matches!(runtime.model_surface(), ModelSurface::FullOperatorRuntime);
    let context_request = if context_sidecar_capable {
        match strip_stateless_context_request(&mut params.arguments) {
            Ok(keys) => keys,
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
    if !context_request.is_empty() {
        if let Some(arguments) = params.arguments.as_object_mut() {
            arguments.insert(
                crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_INTERNAL_FIELD
                    .to_string(),
                json!(context_request),
            );
        }
    }
    if context_continuity_capable {
        if let Some(arguments) = params.arguments.as_object_mut() {
            arguments.remove(
                crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD,
            );
        }
        let context_revision = strip_stateless_ack_session_context_revision(&mut params.arguments);
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
        .call_tool_with_protocol_capabilities(
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
            ToolProtocolCapabilities {
                context_continuity: context_continuity_capable,
                context_sidecar: context_sidecar_capable,
                skill_runtime: skill_runtime_capable,
                skill_management: skill_management_capable,
                memory_surface: memory_surface_capable,
            },
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
                *slot = Some(completion.record_for_pre_result_failure("insufficient_scope"));
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
    let result = match resources::adapt_tool_result(
        &params.name,
        as_image_requested,
        result,
        resource_tool_call,
    ) {
        resources::McpResourceToolResultAdaptation::Framed(value) => value,
        resources::McpResourceToolResultAdaptation::Unhandled(result) => {
            mcp_runtime_tool_result_fallback(result)
        }
    };
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

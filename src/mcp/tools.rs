use super::presentation;
use super::resources;
use super::response::{
    mcp_runtime_tool_result_fallback, mcp_stateless_result, rpc_error, rpc_result,
    McpToolResultPresentation,
};
use super::{require_mcp_scope, scope_forbidden, McpOutcome};
use crate::auth::AuthContext;
pub(super) use crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME;
use crate::model_surface::{
    project_suggested_tool_call_schema, project_tool_result_suggested_calls, SuggestedToolCallRoute,
};
use crate::tool_request_trace::ToolRequestLifecycle;
use crate::tool_runtime::kernel::{
    check_runtime_tool_scope, HostFileImportTrust, ToolCallContext, ToolCallErrorStatus,
    ToolCallRequest as KernelToolCallRequest, ToolInvocationMetadata, ToolProtocolCapabilities,
    ToolTransport,
};
use crate::tool_runtime::model_ergonomics_telemetry::{
    ModelErgonomicsRecord, ModelErgonomicsTimer,
};
use crate::tool_runtime::specialized::SpecializedGovernanceDenial;
use crate::tool_runtime::tool_definition::{
    is_adaptive_runtime_direct_tool, is_model_visible_tool_name,
    runtime_tool_operator_extension_family, ToolOperatorExtensionFamily,
};
use crate::tool_runtime::{ToolCall, ToolResult, ToolRuntime, ToolSpec};
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) const WORK_RESULT_APP_RESULT_META_KEY: &str = "webcodex/workResult";

fn filter_specs_for_oauth(mut specs: Vec<ToolSpec>, auth: Option<&AuthContext>) -> Vec<ToolSpec> {
    let oauth_scope_projection = auth.is_some_and(AuthContext::is_oauth_token);
    specs.retain(|spec| {
        let authority = crate::tool_runtime::metadata::lookup_tool_metadata(&spec.name)
            .map(|metadata| metadata.authority);
        matches!(
            authority,
            Some(webcodex_core::authority::ToolAuthorityPolicy::RequireAny(_))
        )
        .then(|| check_runtime_tool_scope(auth, &spec.name).is_ok())
        .unwrap_or_else(|| {
            !oauth_scope_projection || check_runtime_tool_scope(auth, &spec.name).is_ok()
        })
    });
    specs
}

// Discovery projection only. Canonical operator-extension specs still own
// direct compatibility, capability-aware manifests, and gateway admission.
fn stateless_advertised_operator_extension_specs_for_auth(
    stateless_2026: bool,
    auth: Option<&AuthContext>,
) -> Vec<ToolSpec> {
    if !stateless_2026 {
        return Vec::new();
    }
    crate::tool_runtime::stateless_operator_extension_tool_specs()
        .into_iter()
        .filter(
            |spec| match runtime_tool_operator_extension_family(&spec.name) {
                Some(ToolOperatorExtensionFamily::SkillManagement) => {
                    auth.is_some_and(|auth| auth.has_scope(crate::auth::SCOPE_ADMIN))
                }
                Some(ToolOperatorExtensionFamily::TraceDiagnostics) => {
                    check_runtime_tool_scope(auth, &spec.name).is_ok()
                }
                Some(
                    ToolOperatorExtensionFamily::SkillRuntime
                    | ToolOperatorExtensionFamily::MemoryRuntime
                    | ToolOperatorExtensionFamily::MemoryManagement,
                )
                | None => false,
            },
        )
        .collect()
}

fn adaptive_runtime_gateway_tool_spec() -> ToolSpec {
    ToolSpec {
        name: ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME.to_string(),
        description: "Call one runtime tool admitted by Adaptive Runtime through the generic gateway. A tool with availability=direct should still be invoked through its direct callable when available; ordinary direct tools may fall back here when that callable is unavailable or not loaded. Explicit MCP App presentation tools are the exception: when MCP Apps are enabled they must use their direct callable because only that tool descriptor carries the Host App resource metadata. Directness otherwise changes preferred model exposure only: canonical runtime argument validation, OAuth scope, Project authority, permission gates, Runner capability, Session/context policy, host-file-import trust, protocol capability admission, and tool effects remain unchanged.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "tool": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 128,
                    "description": "Exact runtime tool name admitted by Adaptive Runtime. availability=direct is normally preferred with gateway fallback when unavailable; explicit MCP App presentation tools remain direct-only while MCP Apps are enabled."
                },
                "arguments": {
                    "type": "object",
                    "description": "Arguments for the selected runtime tool. Discover its contract before calling when it is not already known.",
                    "additionalProperties": true
                }
            },
            "required": ["tool", "arguments"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "description": "Normal MCP result for the selected runtime tool.",
            "additionalProperties": true
        }),
        annotations: json!({
            "readOnlyHint": false,
            "destructiveHint": true,
            "idempotentHint": false,
            "openWorldHint": false
        }),
    }
}

fn mcp_adaptive_runtime_gateway_target_route(
    target: &str,
    stateless_2026: bool,
) -> crate::model_surface::AdaptiveRuntimeGatewayTargetRoute {
    use crate::model_surface::AdaptiveRuntimeGatewayTargetRoute;

    if stateless_2026
        && crate::tool_runtime::stateless_operator_extension_tool_specs()
            .iter()
            .any(|spec| spec.name == target)
    {
        let (availability, gateway_tool) =
            crate::model_surface::adaptive_runtime_tool_invocation_route_with_operator_extension(
                target, true,
            );
        return match (availability, gateway_tool) {
            (crate::model_surface::TOOL_SURFACE_AVAILABILITY_DIRECT, None) => {
                AdaptiveRuntimeGatewayTargetRoute::Direct
            }
            (
                crate::model_surface::TOOL_SURFACE_AVAILABILITY_GATEWAY,
                Some(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME),
            ) => AdaptiveRuntimeGatewayTargetRoute::Gateway,
            _ => AdaptiveRuntimeGatewayTargetRoute::Unknown,
        };
    }
    let route = crate::model_surface::adaptive_runtime_gateway_target_route(target);
    if route != AdaptiveRuntimeGatewayTargetRoute::Unknown {
        return route;
    }
    // The MCP adapter's own gateway remains a specialized protocol route. It is
    // not part of the runtime ToolSpec extension universe.
    if target == crate::mcp_gateway::MCP_TOOL_NAME {
        return AdaptiveRuntimeGatewayTargetRoute::Gateway;
    }
    AdaptiveRuntimeGatewayTargetRoute::Unknown
}

#[cfg(test)]
pub(crate) fn adaptive_runtime_gateway_target_admitted_for_test(
    target: &str,
    stateless_2026: bool,
) -> bool {
    matches!(
        mcp_adaptive_runtime_gateway_target_route(target, stateless_2026),
        crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Gateway
            | crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Direct
    )
}

fn mcp_suggested_tool_call_route(target: &str, stateless_2026: bool) -> SuggestedToolCallRoute {
    if target == ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME {
        return SuggestedToolCallRoute::Unavailable;
    }
    if target == crate::mcp_gateway::MCP_TOOL_NAME {
        return SuggestedToolCallRoute::Gateway(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME);
    }
    let operator_extension_admitted = stateless_2026
        && crate::tool_runtime::stateless_operator_extension_tool_specs()
            .iter()
            .any(|spec| spec.name == target);
    crate::model_surface::suggested_tool_call_route(target, operator_extension_admitted)
}

fn project_mcp_tool_spec_output_schema(mut spec: ToolSpec, stateless_2026: bool) -> ToolSpec {
    project_suggested_tool_call_schema(&mut spec.output_schema, &|target| {
        mcp_suggested_tool_call_route(target, stateless_2026)
    });
    spec
}

fn unwrap_adaptive_runtime_gateway_arguments(
    arguments: Value,
    stateless_2026: bool,
) -> Result<(String, Value), String> {
    let mut outer = arguments
        .as_object()
        .cloned()
        .ok_or_else(|| "adaptive runtime gateway arguments must be an object".to_string())?;
    let target = outer
        .remove("tool")
        .and_then(|value| value.as_str().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "adaptive runtime gateway field 'tool' must be a non-empty string".to_string()
        })?;
    let mut target_arguments = outer.remove("arguments").unwrap_or_else(|| json!({}));
    if target_arguments.is_null() {
        target_arguments = json!({});
    }
    let target_object = target_arguments.as_object_mut().ok_or_else(|| {
        "adaptive runtime gateway field 'arguments' must be an object".to_string()
    })?;

    let mut allowed_wrapper_fields = Vec::new();
    if stateless_2026 {
        allowed_wrapper_fields.extend([
            crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD,
            crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD,
            crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD,
            crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD,
            crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD,
            crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD,
            crate::tool_runtime::control_sidecar::CONTROL_FIELD,
        ]);
    }
    for (key, value) in outer {
        if !allowed_wrapper_fields.contains(&key.as_str()) {
            return Err(format!(
                "unsupported adaptive runtime gateway field '{key}'"
            ));
        }
        if target_object.insert(key.clone(), value).is_some() {
            return Err(format!(
                "adaptive runtime gateway field '{key}' was supplied both outside and inside 'arguments'"
            ));
        }
    }
    Ok((target, target_arguments))
}

fn adaptive_runtime_gateway_unknown_target(target: &str) -> ToolResult {
    ToolResult::err_with_output(
        format!("unknown adaptive runtime tool '{target}'"),
        json!({
            "error_kind": "unknown_tool",
            "execution_state": "not_started",
            "state_changed": false,
            "target_tool": target,
            "recovery_kind": "fix_input"
        }),
    )
}

#[derive(Debug, Deserialize)]
pub(super) struct McpToolCallParams {
    pub(super) name: String,
    #[serde(default)]
    pub(super) arguments: Value,
}

pub(super) fn tool_name_from_params(params: &Value) -> Option<String> {
    let name = params.get("name").and_then(Value::as_str)?;
    if name == ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME {
        return params["arguments"]["tool"]
            .as_str()
            .map(str::to_string)
            .or_else(|| Some(name.to_string()));
    }
    Some(name.to_string())
}

pub(super) fn project_from_tool_call_params(params: &Value) -> Option<String> {
    if params.get("name").and_then(Value::as_str) == Some(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME) {
        return params["arguments"]["arguments"]["project"]
            .as_str()
            .map(str::to_string);
    }
    params["arguments"]["project"].as_str().map(str::to_string)
}

/// Test helper for legacy/non-stateless tools/list rendering. Production uses
/// the canonical auth/surface renderer below directly.
#[cfg(test)]
pub(super) fn mcp_tools_list_payload_with_compact(compact: bool) -> Value {
    mcp_tools_list_payload_with_features(compact, false)
}

#[cfg(test)]
pub(super) fn mcp_tools_list_payload_with_compact_and_app(
    compact: bool,
    app_enabled: bool,
) -> Value {
    mcp_tools_list_payload_with_features(compact, app_enabled)
}

#[cfg(test)]
fn mcp_tools_list_payload_with_features(compact: bool, app_enabled: bool) -> Value {
    mcp_tools_list_payload_with_features_for_auth(compact, app_enabled, false, None)
}

pub(super) fn mcp_tools_list_payload_with_features_for_auth(
    compact: bool,
    app_enabled: bool,
    stateless_2026: bool,
    auth: Option<&AuthContext>,
) -> Value {
    let mut specs = filter_specs_for_oauth(
        crate::model_surface::adaptive_runtime_direct_tool_specs(),
        auth,
    );
    specs.extend(stateless_advertised_operator_extension_specs_for_auth(
        stateless_2026,
        auth,
    ));

    let mut tools = specs
        .into_iter()
        .map(|spec| {
            mcp_tool_spec_json(
                project_mcp_tool_spec_output_schema(spec, stateless_2026),
                compact,
                app_enabled,
            )
        })
        .collect::<Vec<_>>();
    if app_enabled && stateless_2026 {
        let mut app_specs = filter_specs_for_oauth(
            crate::tool_runtime::goal_plan_app_tool_specs()
                .into_iter()
                .chain(crate::tool_runtime::work_result_app_tool_specs())
                .chain(crate::tool_runtime::agent_continuation_app_tool_specs())
                .chain(crate::tool_runtime::job_terminal_continuation_app_tool_specs())
                .collect(),
            auth,
        )
        .into_iter()
        .map(|spec| {
            let agent_continuation_tool = is_agent_continuation_app_tool_name(&spec.name);
            let job_terminal_continuation_tool =
                is_job_terminal_continuation_app_tool_name(&spec.name);
            let mut value = mcp_tool_spec_json(spec, compact, false);
            attach_app_visibility(&mut value);
            if agent_continuation_tool {
                // Keep the app-only tools associated with the same continuation
                // resource for compatibility with Hosts that use that hint. The
                // association is not authority; visibility remains app-only and
                // every call is re-authorized by the normal communication kernel.
                attach_app_metadata(
                    &mut value,
                    resources::MCP_AGENT_CONTINUATION_UI_RESOURCE_URI,
                );
                attach_agent_continuation_app_diagnostic_schema(&mut value);
            }
            if job_terminal_continuation_tool {
                attach_app_metadata(
                    &mut value,
                    resources::MCP_JOB_TERMINAL_CONTINUATION_UI_RESOURCE_URI,
                );
                attach_agent_continuation_app_diagnostic_schema(&mut value);
            }
            value
        })
        .collect::<Vec<_>>();
        tools.append(&mut app_specs);
    }
    json!({ "tools": tools })
}

fn adapt_native_image_output_schema_for_mcp(spec: &mut ToolSpec) {
    let properties = spec
        .output_schema
        .pointer_mut("/properties/output/properties")
        .and_then(Value::as_object_mut)
        .expect("computer_observe output schema properties");
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
        "required": ["materials", "truncated"],
        "additionalProperties": false
    })
}

fn add_wrapper_projection_to_output_shape(
    schema: &mut Value,
    field: &str,
    projection_schema: &Value,
) {
    if schema.get("type").and_then(Value::as_str) == Some("object") {
        if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
            properties.insert(field.to_string(), projection_schema.clone());
        }
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema.get_mut(keyword).and_then(Value::as_array_mut) {
            for branch in branches {
                add_wrapper_projection_to_output_shape(branch, field, projection_schema);
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
        add_wrapper_projection_to_output_shape(output, "context_projection", &projection_schema);
    }
    if let Some(conditions) = output_schema.get_mut("allOf").and_then(Value::as_array_mut) {
        for condition in conditions {
            for branch_name in ["then", "else"] {
                if let Some(output) =
                    condition.pointer_mut(&format!("/{branch_name}/properties/output"))
                {
                    add_wrapper_projection_to_output_shape(
                        output,
                        "context_projection",
                        &projection_schema,
                    );
                }
            }
        }
    }
}

fn stateless_collaboration_ack_schema() -> Value {
    json!({
        "type": "array",
        "maxItems": crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_MESSAGE_IDS,
        "items": {
            "type": "string",
            "pattern": "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"
        },
        "description": "Proves the current model context still retains the listed ACK-required collaboration messages. Session ACK uses the explicit recorder when present, otherwise an authorized same-Window active Session affinity for the resolved Project; Peer and Operator ACK target the current principal-bound ClientWindow. Repeat while retained. The historical field name is shared across all three channels. ACK neither resolves messages nor grants authority or gates execution."
    })
}

fn stateless_session_ack_ref_schema() -> Value {
    json!({
        "type": "string",
        "maxLength": crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_REF_CHARS,
        "description": "Compact request-scoped ACK evidence returned as session_attention.ack_ref for the exact retained Session ACK set represented by that exchange. It acknowledges only that Session set, grants no authority, never resolves messages, and does not apply to Peer ACK."
    })
}

fn stateless_session_attention_output_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Bounded open ACK-required messages from one exact authorized Workflow Session. Window affinity may select this delivery scope only when no explicit recorder was supplied; it never records the main call or supplies business authority.",
        "properties": {
            "session_id": {
                "type": "string",
                "pattern": "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"
            },
            "source": {
                "type": "string",
                "enum": ["recording_session", "business_session", "window_affinity"]
            },
            "requires_ack": {"type": "boolean"},
            "ack_ref": {
                "type": "string",
                "maxLength": crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_REF_CHARS,
                "description": "Exact compact Session ACK set retained across this exchange; echo on a later request while this context is still retained."
            },
            "messages": {
                "type": "array",
                "maxItems": crate::tool_runtime::SESSION_ATTENTION_MAX_MESSAGES,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "message_id": {"type": "string"},
                        "kind": {"type": "string"},
                        "priority": {"type": "string"},
                        "created_at": {"type": "integer"},
                        "message": {"type": "string"},
                        "message_truncated": {"type": "boolean"}
                    },
                    "required": ["message_id", "kind", "priority", "created_at", "message", "message_truncated"]
                }
            },
            "omitted_count": {"type": "integer", "minimum": 0},
            "truncated": {"type": "boolean"},
            "ack": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "accepted_count": {"type": "integer", "minimum": 0},
                    "ignored_count": {"type": "integer", "minimum": 0}
                },
                "required": ["accepted_count", "ignored_count"]
            }
        },
        "required": ["session_id", "source", "requires_ack", "messages", "omitted_count", "truncated", "ack"]
    })
}

fn add_stateless_session_attention_output_schema(tool: &mut Value) {
    let Some(output_schema) = tool.get_mut("outputSchema") else {
        return;
    };
    let projection = stateless_session_attention_output_schema();
    if let Some(output) = output_schema.pointer_mut("/properties/output") {
        add_wrapper_projection_to_output_shape(output, "session_attention", &projection);
    }
    if let Some(conditions) = output_schema.get_mut("allOf").and_then(Value::as_array_mut) {
        for condition in conditions {
            for branch_name in ["then", "else"] {
                if let Some(output) =
                    condition.pointer_mut(&format!("/{branch_name}/properties/output"))
                {
                    add_wrapper_projection_to_output_shape(
                        output,
                        "session_attention",
                        &projection,
                    );
                }
            }
        }
    }
}

fn stateless_window_reply_supported(tool_name: Option<&str>) -> bool {
    let Some(tool_name) = tool_name else {
        return false;
    };
    if matches!(
        tool_name,
        crate::mcp_gateway::MCP_TOOL_NAME
            | crate::plugin_gateway::PLUGIN_TOOL_NAME
            | crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME
    ) {
        return false;
    }
    tool_name == ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME || is_model_visible_tool_name(tool_name)
}

fn stateless_window_reply_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Reply to one Operator message retained in this exact Window; no Workflow Session required. The reply is persisted independently after the main tool result.",
        "properties": {
            "reply_to": {
                "type": "string",
                "pattern": "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"
            },
            "message": {
                "type": "string",
                "minLength": 1,
                "maxLength": crate::tool_runtime::window_collaboration::MAX_WINDOW_REPLY_CHARS
            }
        },
        "required": ["reply_to", "message"]
    })
}

fn stateless_window_reply_output_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": true,
        "properties": {
            "success": {"type": "boolean"},
            "message_id": {"type": "string"},
            "reply_to": {"type": "string"},
            "replayed": {"type": "boolean"},
            "state_changed": {}
        },
        "required": ["success", "state_changed"]
    })
}

fn add_stateless_window_reply_output_schema(tool: &mut Value) {
    let Some(output_schema) = tool.get_mut("outputSchema") else {
        return;
    };
    let projection = stateless_window_reply_output_schema();
    if let Some(output) = output_schema.pointer_mut("/properties/output") {
        add_wrapper_projection_to_output_shape(output, "window_reply", &projection);
    }
    if let Some(conditions) = output_schema.get_mut("allOf").and_then(Value::as_array_mut) {
        for condition in conditions {
            for branch_name in ["then", "else"] {
                if let Some(output) =
                    condition.pointer_mut(&format!("/{branch_name}/properties/output"))
                {
                    add_wrapper_projection_to_output_shape(output, "window_reply", &projection);
                }
            }
        }
    }
}

fn insert_stateless_collaboration_ack_property(properties: &mut serde_json::Map<String, Value>) {
    properties.insert(
        crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD.to_string(),
        stateless_collaboration_ack_schema(),
    );
    properties.insert(
        crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD.to_string(),
        stateless_session_ack_ref_schema(),
    );
}

pub(super) const RECORDING_SESSION_SELECTOR_SCHEMA_PATTERN: &str =
    "^(~s[1-9][0-9]{0,19}|wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32}))$";

pub(super) fn add_stateless_workflow_recorder_metadata(payload: &mut Value) {
    let Some(tools) = payload.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };
    for tool in tools {
        let tool_name_owned = tool.get("name").and_then(Value::as_str).map(str::to_string);
        let tool_name = tool_name_owned.as_deref();
        if matches!(
            tool_name,
            Some("goal_plan_sync" | "work_result_state" | "changes_file_diff")
        ) || tool_name.is_some_and(is_host_continuation_app_tool_name)
        {
            continue;
        }
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
                "pattern": RECORDING_SESSION_SELECTOR_SCHEMA_PATTERN,
                "description": "Optional explicit recorder provenance for one exact Workflow Session; accepts canonical wc_sess_* or issued principal-scoped ~sN. Never execution/business authority. Omission may still allow authorized same-Window attention without recording."
            }),
        );
        insert_stateless_collaboration_ack_property(properties);
        if stateless_window_reply_supported(tool_name) {
            properties.insert(
                crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD.to_string(),
                stateless_window_reply_input_schema(),
            );
        }
        properties.insert(
            crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD.to_string(),
            json!({
                "type": "object",
                "description": "After handling one non-todo message in the explicit recording Session, attach its id and bounded resolution text here to resolve it on the same WebCodex call. Any ACK-required Session message also needs request-scoped ACK. Applies only to that exact recording Session; removed before concrete parsing; does not apply to Peer messages and does not predict call success. Todos use the atomic completion path.",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "pattern": "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"
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
                        "description": "Bounded context material key; unsupported keys are reported nonfatally."
                    },
                    "description": format!("Request bounded context material after this tool's main effect/observation; keys are open-ended and currently include {}. This sidecar grants no authority and cannot make requested guidance a retroactive precondition of the current effect. Recover missing project or Memory guidance on a read/observation call before any later dependent mutation.", crate::tool_runtime::context_projection::context_material_keys_csv())
                }),
            );
        if let Some(name) = tool_name.filter(|name| {
            *name == "call_runtime_tool"
                || crate::tool_runtime::control_sidecar::supports_control_sidecars(name)
        }) {
            properties.insert(
                crate::tool_runtime::control_sidecar::CONTROL_FIELD.to_string(),
                crate::tool_runtime::control_sidecar::input_schema(name),
            );
            let projection = crate::tool_runtime::control_sidecar::output_schema();
            if let Some(output) = tool.pointer_mut("/outputSchema/properties/output") {
                add_wrapper_projection_to_output_shape(output, "control", &projection);
            }
            if let Some(conditions) = tool
                .pointer_mut("/outputSchema/allOf")
                .and_then(Value::as_array_mut)
            {
                for condition in conditions {
                    for branch in ["then", "else"] {
                        if let Some(output) =
                            condition.pointer_mut(&format!("/{branch}/properties/output"))
                        {
                            add_wrapper_projection_to_output_shape(output, "control", &projection);
                        }
                    }
                }
            }
        }
        add_stateless_context_projection_output_schema(tool);
        add_stateless_session_attention_output_schema(tool);
        if stateless_window_reply_supported(tool_name) {
            add_stateless_window_reply_output_schema(tool);
        }
    }
}

fn tool_meta_object(value: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
    let object = value.as_object_mut()?;
    let meta = object
        .entry("_meta".to_string())
        .or_insert_with(|| json!({}));
    meta.as_object_mut()
}

pub(super) fn attach_app_metadata(value: &mut Value, resource_uri: &str) {
    let Some(meta) = tool_meta_object(value) else {
        return;
    };
    let ui = meta.entry("ui".to_string()).or_insert_with(|| json!({}));
    let Some(ui) = ui.as_object_mut() else {
        return;
    };
    ui.insert(
        "resourceUri".to_string(),
        Value::String(resource_uri.to_string()),
    );
}

fn attach_app_visibility(value: &mut Value) {
    let Some(meta) = tool_meta_object(value) else {
        return;
    };
    let ui = meta.entry("ui".to_string()).or_insert_with(|| json!({}));
    let Some(ui) = ui.as_object_mut() else {
        return;
    };
    ui.insert("visibility".to_string(), json!(["app"]));
}

fn is_agent_continuation_app_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "agent_continuation_bind"
            | "agent_continuation_recover_endpoint"
            | "agent_continuation_state"
            | "agent_continuation_wake_acquire"
            | "agent_continuation_wake_prepare"
            | "agent_continuation_wake_finish"
            | "agent_continuation_unbind"
            | "agent_wait_state"
    )
}

fn is_job_terminal_continuation_app_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "job_terminal_continuation_bind"
            | "job_terminal_continuation_state"
            | "job_terminal_continuation_prepare"
            | "job_terminal_continuation_finish"
            | "job_terminal_continuation_unbind"
    )
}

fn is_host_continuation_app_tool_name(tool_name: &str) -> bool {
    is_agent_continuation_app_tool_name(tool_name)
        || is_job_terminal_continuation_app_tool_name(tool_name)
}

const AGENT_CONTINUATION_APP_CALL_ID_FIELD: &str = "app_call_id";
const AGENT_CONTINUATION_APP_CALL_ID_PATTERN: &str = "^wc_app_call_[0-9a-f]{16}_[1-9][0-9]{0,5}$";

fn valid_agent_continuation_app_call_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("wc_app_call_") else {
        return false;
    };
    let Some((view_prefix, sequence)) = rest.split_once('_') else {
        return false;
    };
    view_prefix.len() == 16
        && view_prefix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && !sequence.is_empty()
        && sequence.len() <= 6
        && !sequence.starts_with('0')
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
}

pub(super) fn agent_continuation_app_call_id_from_params(params: &Value) -> Option<String> {
    let name = params.get("name").and_then(Value::as_str)?;
    if !is_host_continuation_app_tool_name(name) {
        return None;
    }
    let value = params
        .get("arguments")?
        .get(AGENT_CONTINUATION_APP_CALL_ID_FIELD)?
        .as_str()?;
    valid_agent_continuation_app_call_id(value).then(|| value.to_string())
}

fn attach_agent_continuation_app_diagnostic_schema(value: &mut Value) {
    let Some(properties) = value
        .pointer_mut("/inputSchema/properties")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    properties.insert(
        AGENT_CONTINUATION_APP_CALL_ID_FIELD.to_string(),
        json!({
            "type": "string",
            "pattern": AGENT_CONTINUATION_APP_CALL_ID_PATTERN,
            "description": "Optional App-generated diagnostic correlation id. Grants no authority and is stripped by the MCP adapter before ToolRuntime parsing."
        }),
    );
}

fn strip_agent_continuation_app_call_id(arguments: &mut Value) -> Result<Option<String>, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Ok(None);
    };
    let Some(value) = object.remove(AGENT_CONTINUATION_APP_CALL_ID_FIELD) else {
        return Ok(None);
    };
    let Value::String(value) = value else {
        return Err(format!(
            "field '{AGENT_CONTINUATION_APP_CALL_ID_FIELD}' must be a canonical App diagnostic id"
        ));
    };
    if !valid_agent_continuation_app_call_id(&value) {
        return Err(format!(
            "field '{AGENT_CONTINUATION_APP_CALL_ID_FIELD}' must match {AGENT_CONTINUATION_APP_CALL_ID_PATTERN}"
        ));
    }
    Ok(Some(value))
}

fn attach_app_tool_content_fallback(result: &mut Value) {
    let Some(structured) = result.get("structuredContent") else {
        return;
    };
    let Ok(text) = serde_json::to_string(structured) else {
        return;
    };
    result["content"] = json!([{ "type": "text", "text": text }]);
}

fn attach_work_result_app_private_result(result: &mut Value) {
    let Some(structured) = result.get("structuredContent").cloned() else {
        return;
    };
    let meta = result.as_object_mut().and_then(|result| {
        result
            .entry("_meta")
            .or_insert_with(|| json!({}))
            .as_object_mut()
    });
    if let Some(meta) = meta {
        meta.insert(WORK_RESULT_APP_RESULT_META_KEY.to_string(), structured);
    }
}

fn safe_continuation_dispatch_observation(value: Option<&Value>) -> &'static str {
    match value.and_then(Value::as_str) {
        Some("dispatch_prepared") => "dispatch_prepared",
        Some("dispatch_accepted") => "dispatch_accepted",
        Some("dispatch_unknown") => "dispatch_unknown",
        Some("continuation_consumed") => "continuation_consumed",
        _ => "-",
    }
}

fn log_agent_continuation_app_result(
    lifecycle: Option<&ToolRequestLifecycle>,
    tool_name: &str,
    app_call_id: Option<&str>,
    result: &Value,
) {
    if !crate::tool_request_trace::tool_request_trace_enabled() {
        return;
    }
    let Some(lifecycle) = lifecycle else {
        return;
    };
    let structured = result.get("structuredContent").and_then(Value::as_object);
    let tool_success = structured
        .and_then(|structured| structured.get("success"))
        .and_then(Value::as_bool);
    let output = structured
        .and_then(|structured| structured.get("output"))
        .and_then(Value::as_object);
    let projection = output
        .and_then(|output| output.get("agent_continuation"))
        .and_then(Value::as_object)
        .or(output);
    let wake_present = projection
        .and_then(|projection| projection.get("wake"))
        .is_some_and(|wake| !wake.is_null())
        || output
            .and_then(|output| output.get("wake_id"))
            .is_some_and(Value::is_string);
    let queued_delivery_count = projection
        .and_then(|projection| projection.get("queued_delivery_count"))
        .and_then(Value::as_i64)
        .or_else(|| {
            projection
                .and_then(|projection| projection.get("wake"))
                .and_then(Value::as_object)
                .and_then(|wake| wake.get("queued_delivery_count"))
                .and_then(Value::as_i64)
        });
    let host_bound = projection
        .and_then(|projection| projection.get("host_binding"))
        .and_then(Value::as_object)
        .and_then(|host_binding| host_binding.get("bound"))
        .and_then(Value::as_bool);
    let dispatch_observation = safe_continuation_dispatch_observation(
        projection
            .and_then(|projection| projection.get("dispatch_observation"))
            .or_else(|| output.and_then(|output| output.get("dispatch_observation"))),
    );
    let content_blocks = result
        .get("content")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    tracing::info!(
        event = "mcp_agent_continuation_app_result",
        server_trace_id = %lifecycle.correlation_trace_id(),
        app_call_id = app_call_id.unwrap_or("-"),
        tool_name,
        tool_success = tool_success.map(|value| if value { 1_i32 } else { 0_i32 }).unwrap_or(-1),
        structured_content_present = if structured.is_some() { 1_i32 } else { 0_i32 },
        content_blocks = content_blocks as i64,
        host_bound = host_bound.map(|value| if value { 1_i32 } else { 0_i32 }).unwrap_or(-1),
        wake_present = if wake_present { 1_i32 } else { 0_i32 },
        queued_delivery_count = queued_delivery_count.unwrap_or(-1),
        dispatch_observation,
        "mcp_agent_continuation_app_result"
    );
}

fn attach_job_terminal_resume_suggested_call_schema(
    tool_name: &str,
    app_enabled: bool,
    value: &mut Value,
) {
    if !app_enabled || tool_name != "wait_for_job_terminal" {
        return;
    }
    let Some(properties) = value
        .pointer_mut("/outputSchema/properties/output/properties")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    properties.insert(
        "suggested_call".to_string(),
        json!({
            "type": "object",
            "description": "Host-specific parser-ready advisory call for the current MCP App continuation carrier. Present only for a still-waiting Job when this Host can create that carrier; it grants no authority and should be used only when no independent work remains and the current model turn can yield immediately after presentation.",
            "additionalProperties": false,
            "properties": {
                "tool": {"type": "string", "const": "present_job_terminal_continuation"},
                "arguments": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "wait_id": {
                            "type": "string",
                            "pattern": "^wc_job_wait_[A-Za-z0-9_-]{16}$"
                        }
                    },
                    "required": ["wait_id"]
                }
            },
            "required": ["tool", "arguments"]
        }),
    );
}

pub(super) fn project_job_terminal_resume_suggested_call(
    carrier_available: bool,
    result: &mut ToolResult,
) {
    if !carrier_available || !result.success {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    if output
        .get("automatic_resume_available")
        .and_then(Value::as_bool)
        != Some(false)
        || output.get("state").and_then(Value::as_str) != Some("waiting")
        || output.get("delivery_state").and_then(Value::as_str) != Some("not_ready")
    {
        return;
    }
    let Some(wait_id) = output
        .get("wait_id")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    output.insert(
        "suggested_call".to_string(),
        crate::tool_runtime::SuggestedToolCall::new(
            "present_job_terminal_continuation",
            json!({"wait_id": wait_id}),
        )
        .to_value(),
    );
}

fn mcp_tool_spec_json(mut spec: ToolSpec, compact: bool, app_enabled: bool) -> Value {
    let tool_name = spec.name.clone();
    if matches!(tool_name.as_str(), "computer_observe" | "browser_observe") {
        adapt_native_image_output_schema_for_mcp(&mut spec);
    }
    if tool_name == "read_project_artifact" {
        if let Some(properties) = spec.input_schema["properties"].as_object_mut() {
            properties.insert(
                "as_image".to_string(),
                json!({
                    "type": "boolean",
                    "description": "MCP-only. When true, read one complete PNG, JPEG, or WebP up to 1 MiB and return it as native image content. Cannot be combined with offset or length."
                }),
            );
        }
        spec.description.push_str(
            " Over MCP, set as_image=true to return one complete PNG, JPEG, or WebP as native image content; ordinary calls keep the existing chunked base64 response.",
        );
    }
    let ToolSpec {
        name,
        description,
        input_schema,
        output_schema,
        annotations,
    } = spec;
    let mut value = if compact {
        json!({
            "name": name,
            "description": description,
            "inputSchema": input_schema,
            "annotations": annotations,
        })
    } else {
        // Move the already-built schema DOMs directly into the MCP projection.
        // ToolSpec's serde shape is exactly these five camelCase fields; routing,
        // authorization, and capability admission have already run before here.
        json!({
            "name": name,
            "description": description,
            "inputSchema": input_schema,
            "outputSchema": output_schema,
            "annotations": annotations,
        })
    };
    if !compact && tool_name == crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME {
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "outputSchema".to_string(),
                crate::ssh_resource_gateway::mcp_output_schema(),
            );
        }
    }
    if tool_name == "import_conversation_files_to_project" {
        if let Some(required) =
            value.pointer_mut("/inputSchema/properties/openaiFileIdRefs/items/required")
        {
            *required = json!(["download_url", "file_id"]);
        }
        if let Some(meta) = tool_meta_object(&mut value) {
            meta.insert("openai/fileParams".to_string(), json!(["openaiFileIdRefs"]));
        }
    }
    if app_enabled && presentation::tool_supports_result_app(&tool_name) {
        attach_app_metadata(&mut value, resources::MCP_RESULT_UI_RESOURCE_URI);
    }
    if app_enabled && presentation::tool_supports_work_result_app(&tool_name) {
        attach_app_metadata(&mut value, resources::MCP_WORK_RESULT_UI_RESOURCE_URI);
    }
    if app_enabled && presentation::tool_supports_goal_plan_app(&tool_name) {
        attach_app_metadata(&mut value, resources::MCP_GOAL_PLAN_UI_RESOURCE_URI);
    }
    if app_enabled && presentation::tool_supports_agent_continuation_app(&tool_name) {
        attach_app_metadata(
            &mut value,
            resources::MCP_AGENT_CONTINUATION_UI_RESOURCE_URI,
        );
    }
    if app_enabled && presentation::tool_supports_job_terminal_continuation_app(&tool_name) {
        attach_app_metadata(
            &mut value,
            resources::MCP_JOB_TERMINAL_CONTINUATION_UI_RESOURCE_URI,
        );
    }
    attach_job_terminal_resume_suggested_call_schema(&tool_name, app_enabled, &mut value);
    if compact {
        super::discovery::compact_tool(&mut value);
    }
    value
}

#[cfg(test)]
pub(super) fn mcp_runtime_tool_result(
    tool_name: &str,
    as_image_requested: bool,
    result: ToolResult,
) -> Value {
    let result_presentation = McpToolResultPresentation::Standard;
    let artifact_presentation = if as_image_requested {
        resources::ProjectArtifactPresentationMode::Image
    } else {
        resources::ProjectArtifactPresentationMode::None
    };
    match resources::adapt_tool_result(
        tool_name,
        artifact_presentation,
        result,
        resources::McpResourceToolCallContext::default(),
        result_presentation,
    ) {
        resources::McpResourceToolResultAdaptation::Framed(value) => value,
        resources::McpResourceToolResultAdaptation::Unhandled(result) => {
            mcp_runtime_tool_result_fallback(result, result_presentation)
        }
    }
}

pub(super) async fn handle_list(
    id: Option<Value>,
    auth: Option<&AuthContext>,
    stateless_2026: bool,
    compact_schemas: bool,
    app_enabled: bool,
) -> McpOutcome {
    let mut result = mcp_tools_list_payload_with_features_for_auth(
        compact_schemas,
        app_enabled,
        stateless_2026,
        auth,
    );
    if let Some(tools) = result.get_mut("tools").and_then(Value::as_array_mut) {
        tools.push(mcp_tool_spec_json(
            adaptive_runtime_gateway_tool_spec(),
            compact_schemas,
            false,
        ));
    }
    if stateless_2026 {
        add_stateless_workflow_recorder_metadata(&mut result);
    }
    if crate::mcp_gateway::authorized(auth) {
        if let Some(tools) = result.get_mut("tools").and_then(Value::as_array_mut) {
            let mut spec = crate::mcp_gateway::tool_spec();
            if stateless_2026 {
                if let Some(properties) = spec
                    .pointer_mut("/inputSchema/properties")
                    .and_then(Value::as_object_mut)
                {
                    insert_stateless_collaboration_ack_property(properties);
                }
            }
            tools.push(spec);
        }
    }
    if compact_schemas {
        // Include adapter-added gateway and Session/context wrapper descriptions.
        // Apply after overlays so none of their repeated full copy leaks into L1.
        if let Some(tools) = result.get_mut("tools").and_then(Value::as_array_mut) {
            for tool in tools {
                super::discovery::compact_tool(tool);
            }
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
    TrustedLoopbackApiToken,
    TrustedLoopbackBootstrap,
    LoopbackApiTokenTrustRequiresLoopback,
    LoopbackBootstrapTrustRequiresLoopback,
    MissingConfig,
    MissingDatabase,
    MissingAuth,
    NotOAuthToken,
    MissingAllowedClientId,
    OAuthDisabled,
    AuthenticatedOAuthOpenAiHostOnly,
    ClientRegistrationMissingOrRevoked,
    ClientRegistrationLookupFailed,
}

impl HostFileImportTrustReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::TrustedLoopbackApiToken => "trusted_loopback_api_token",
            Self::TrustedLoopbackBootstrap => "trusted_loopback_bootstrap",
            Self::LoopbackApiTokenTrustRequiresLoopback => {
                "loopback_api_token_trust_requires_loopback"
            }
            Self::LoopbackBootstrapTrustRequiresLoopback => {
                "loopback_bootstrap_trust_requires_loopback"
            }
            Self::MissingConfig => "missing_config",
            Self::MissingDatabase => "missing_database",
            Self::MissingAuth => "missing_auth",
            Self::NotOAuthToken => "not_oauth_token",
            Self::MissingAllowedClientId => "missing_allowed_client_id",
            Self::OAuthDisabled => "oauth_disabled",
            Self::AuthenticatedOAuthOpenAiHostOnly => "authenticated_oauth_openai_host_only",
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
    if config.oauth2.trust_loopback_api_token_mcp_file_import {
        let loopback_reasons = if auth.kind == crate::auth::AuthKind::ApiToken
            && auth.token_kind.as_deref() == Some("user")
        {
            Some((
                HostFileImportTrustReason::TrustedLoopbackApiToken,
                HostFileImportTrustReason::LoopbackApiTokenTrustRequiresLoopback,
            ))
        } else if auth.kind == crate::auth::AuthKind::Bootstrap
            && auth.is_bootstrap()
            && config
                .token
                .as_deref()
                .is_some_and(|token| !token.trim().is_empty())
        {
            Some((
                HostFileImportTrustReason::TrustedLoopbackBootstrap,
                HostFileImportTrustReason::LoopbackBootstrapTrustRequiresLoopback,
            ))
        } else {
            None
        };
        if let Some((trusted_reason, non_loopback_reason)) = loopback_reasons {
            if config.is_loopback_bound() {
                return HostFileImportTrustDecision {
                    trust: HostFileImportTrust::TrustedMcpHostFile,
                    reason: trusted_reason,
                    ..base
                };
            }
            return HostFileImportTrustDecision {
                reason: non_loopback_reason,
                ..base
            };
        }
    }
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
    match db.get_oauth_client_by_client_id(client_id) {
        Ok(Some(client)) if client.client_id == client_id => {
            if client_id_configured {
                HostFileImportTrustDecision {
                    trust: HostFileImportTrust::TrustedMcpHostFile,
                    reason: HostFileImportTrustReason::Trusted,
                    client_id_configured: Some(true),
                    active_client_registration_found: Some(true),
                    ..base
                }
            } else {
                HostFileImportTrustDecision {
                    trust: HostFileImportTrust::AuthenticatedMcpOpenAiHostFile,
                    reason: HostFileImportTrustReason::AuthenticatedOAuthOpenAiHostOnly,
                    client_id_configured: Some(false),
                    active_client_registration_found: Some(true),
                    ..base
                }
            }
        }
        Ok(_) => HostFileImportTrustDecision {
            reason: HostFileImportTrustReason::ClientRegistrationMissingOrRevoked,
            client_id_configured: Some(client_id_configured),
            active_client_registration_found: Some(false),
            ..base
        },
        Err(_) => HostFileImportTrustDecision {
            reason: HostFileImportTrustReason::ClientRegistrationLookupFailed,
            client_id_configured: Some(client_id_configured),
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

pub(super) fn strip_recording_session_id(arguments: &mut Value) -> Result<Option<String>, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Ok(None);
    };
    if object.contains_key("_session_id") {
        return Err(
            "field '_session_id' is no longer supported; use 'recording_session_id'".to_string(),
        );
    }
    match object.remove(crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD) {
        None => Ok(None),
        Some(Value::String(value)) => {
            let value = value.trim();
            if value.is_empty() {
                return Err(format!(
                    "field '{}' must be a non-empty string",
                    crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD
                ));
            }
            Ok(Some(value.to_string()))
        }
        Some(_) => Err(format!(
            "field '{}' must be a non-empty string",
            crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD
        )),
    }
}

fn canonicalize_recording_session_id(
    runtime: &crate::tool_runtime::ToolRuntime,
    raw: Option<String>,
    auth: Option<&crate::auth::AuthContext>,
) -> Result<Option<String>, String> {
    raw.map(|raw| runtime.canonicalize_explicit_session_selector(&raw, auth))
        .transpose()
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
        if !webcodex_core::workflow_session_contract::is_valid_session_message_id(value) {
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

pub(super) fn strip_stateless_ack_ref(arguments: &mut Value) -> Result<Option<String>, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Ok(None);
    };
    match object.remove(crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD) {
        None => Ok(None),
        Some(Value::String(value)) => {
            let value = value.trim();
            if value.is_empty()
                || value.len() > crate::tool_runtime::sessions::MAX_TOOL_CALL_ACK_REF_CHARS
            {
                return Err(format!(
                    "field '{}' must be a non-empty bounded string",
                    crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD
                ));
            }
            Ok(Some(value.to_string()))
        }
        Some(_) => Err(format!(
            "field '{}' must be a non-empty bounded string",
            crate::tool_runtime::sessions::TOOL_CALL_ACK_REF_FIELD
        )),
    }
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
    if !webcodex_core::workflow_session_contract::is_valid_session_message_id(&message_id) {
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

pub(super) fn strip_stateless_window_reply(
    arguments: &mut Value,
) -> Result<Option<crate::tool_runtime::window_collaboration::ToolCallWindowReply>, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Ok(None);
    };
    let Some(value) =
        object.remove(crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD)
    else {
        return Ok(None);
    };
    let Value::Object(mut fields) = value else {
        return Err("field 'window_reply' must be an object with reply_to and message".to_string());
    };
    if fields.len() != 2 || !fields.contains_key("reply_to") || !fields.contains_key("message") {
        return Err("field 'window_reply' accepts exactly reply_to and message".to_string());
    }
    let Some(Value::String(reply_to)) = fields.remove("reply_to") else {
        return Err("window_reply.reply_to must be a wc_msg_* string".to_string());
    };
    let reply_to = reply_to.trim().to_string();
    if !webcodex_core::workflow_session_contract::is_valid_session_message_id(&reply_to) {
        return Err("window_reply.reply_to must be a valid wc_msg_* id".to_string());
    }
    let Some(Value::String(message)) = fields.remove("message") else {
        return Err("window_reply.message must be a string".to_string());
    };
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("window_reply.message must not be empty".to_string());
    }
    if message.chars().count() > crate::tool_runtime::window_collaboration::MAX_WINDOW_REPLY_CHARS {
        return Err(format!(
            "window_reply.message exceeds {} chars",
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

pub(super) async fn handle_call(
    runtime: &ToolRuntime,
    request_params: Value,
    id: Option<Value>,
    auth: Option<&AuthContext>,
    stateless_2026: bool,
    app_enabled: bool,
    server_mcp_apps_enabled: bool,
    host_file_import_trust: HostFileImportTrust,
    window: Option<&crate::client_window::ClientWindow>,
    mut lifecycle: Option<&mut ToolRequestLifecycle>,
    mut model_ergonomics_out: Option<&mut Option<ModelErgonomicsRecord>>,
    mut correlation_out: Option<&mut crate::tool_runtime::ToolCallCorrelation>,
) -> McpOutcome {
    let result_presentation = McpToolResultPresentation::from_request_params(&request_params);
    let mut params: McpToolCallParams = match serde_json::from_value(request_params) {
        Ok(params) => params,
        Err(e) => {
            return McpOutcome::BadRequest(rpc_error(id, -32602, format!("Invalid params: {}", e)));
        }
    };
    let app_call_id = if stateless_2026 && is_host_continuation_app_tool_name(&params.name) {
        match strip_agent_continuation_app_call_id(&mut params.arguments) {
            Ok(app_call_id) => app_call_id,
            Err(message) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return McpOutcome::BadRequest(rpc_error(id, -32602, message));
            }
        }
    } else {
        None
    };
    if let Some(lc) = lifecycle.as_deref_mut() {
        lc.set_app_call_id(app_call_id.clone());
    }
    let via_adaptive_runtime_gateway = params.name == ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME;
    if via_adaptive_runtime_gateway {
        let (target, arguments) =
            match unwrap_adaptive_runtime_gateway_arguments(params.arguments, stateless_2026) {
                Ok(target) => target,
                Err(message) => {
                    return McpOutcome::BadRequest(rpc_error(id, -32602, message));
                }
            };
        match mcp_adaptive_runtime_gateway_target_route(&target, stateless_2026) {
            crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Gateway
            | crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Direct => {
                if app_enabled && presentation::tool_requires_direct_app_presentation(&target) {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("direct_presentation_required");
                        lc.dispatch_finished(false, Some(false), "direct_presentation_required");
                    }
                    return McpOutcome::BadRequest(rpc_error(
                        id,
                        -32602,
                        format!(
                            "call_runtime_tool cannot invoke MCP App presentation tool '{target}' when MCP Apps are enabled; call '{target}' directly so the Host receives the required App resource metadata"
                        ),
                    ));
                }
                params.name = target;
                params.arguments = arguments;
            }
            crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Unknown => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("unknown_tool");
                    lc.dispatch_finished(false, Some(false), "unknown_tool");
                }
                let rendered = mcp_runtime_tool_result_fallback(
                    adaptive_runtime_gateway_unknown_target(&target),
                    result_presentation,
                );
                return McpOutcome::Ok(rpc_result(
                    id,
                    if stateless_2026 {
                        mcp_stateless_result(rendered, false)
                    } else {
                        rendered
                    },
                ));
            }
            crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Recursive => {
                return McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    "call_runtime_tool cannot target itself through the adaptive runtime gateway",
                ));
            }
        }
    }
    if let Some(lc) = lifecycle.as_deref_mut() {
        lc.set_tool_name(Some(params.name.clone()));
    }
    // Parse model-context message ACK metadata before specialized fast paths branch away from
    // the canonical ToolRuntime kernel. Adaptive gateway wrapper fields have already been folded
    // into the target arguments above, so every model-visible runtime route consumes one canonical
    // ACK representation. The wrapper is never forwarded to Plugin/MCP/SSH business parsers.
    let ack_session_message_ids = if stateless_2026 {
        match strip_stateless_ack_session_message_ids(&mut params.arguments) {
            Ok(ids) => ids,
            Err(message) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return McpOutcome::BadRequest(rpc_error(id, -32602, message));
            }
        }
    } else {
        Vec::new()
    };
    let ack_ref = if stateless_2026 {
        match strip_stateless_ack_ref(&mut params.arguments) {
            Ok(ack_ref) => ack_ref,
            Err(message) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return McpOutcome::BadRequest(rpc_error(id, -32602, message));
            }
        }
    } else {
        None
    };
    let window_reply = if stateless_2026 {
        match strip_stateless_window_reply(&mut params.arguments) {
            Ok(reply) => reply,
            Err(message) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return McpOutcome::BadRequest(rpc_error(id, -32602, message));
            }
        }
    } else {
        None
    };
    if window_reply.is_some()
        && matches!(
            params.name.as_str(),
            crate::mcp_gateway::MCP_TOOL_NAME
                | crate::plugin_gateway::PLUGIN_TOOL_NAME
                | crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME
        )
    {
        if let Some(lc) = lifecycle.as_deref() {
            lc.dispatch_failed("invalid_arguments");
            lc.dispatch_finished(false, Some(false), "invalid_arguments");
        }
        return McpOutcome::BadRequest(rpc_error(
            id,
            -32602,
            "window_reply is supported only on ordinary model-visible Runtime tools",
        ));
    }
    // Strip private control payloads before tracing, canonical argument parsing,
    // specialized dispatch, and audit. Legacy/hidden adapters reject explicitly.
    let control = match crate::tool_runtime::control_sidecar::strip_control_sidecars(
        &mut params.arguments,
        &params.name,
        stateless_2026,
    ) {
        Ok(value) => value,
        Err(message) => {
            if let Some(lc) = lifecycle.as_deref() {
                lc.dispatch_failed("invalid_arguments");
                lc.dispatch_finished(false, Some(false), "invalid_arguments");
            }
            return McpOutcome::BadRequest(rpc_error(id, -32602, message));
        }
    };
    if let Some(lc) = lifecycle.as_deref() {
        lc.capture_payload_lazy("raw_arguments", || {
            if params.name == crate::plugin_gateway::PLUGIN_TOOL_NAME {
                crate::plugin_gateway::audit_arguments(&params.arguments)
            } else if params.name == crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME {
                crate::ssh_resource_gateway::audit_arguments(&params.arguments)
            } else if via_adaptive_runtime_gateway {
                json!({"tool": params.name, "arguments_present": true})
            } else {
                params.arguments.clone()
            }
        });
    }
    // Emit dispatch_started only after params parse succeeds and before
    // ToolRuntime work begins.
    if let Some(lc) = lifecycle.as_deref() {
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
        let mut result = crate::mcp_gateway::call(runtime, params.arguments, auth).await;
        runtime.add_peer_collaboration_to_mcp_call_result(
            &mut result,
            auth,
            window,
            None,
            &ack_session_message_ids,
        );
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
    if params.name == crate::plugin_gateway::PLUGIN_TOOL_NAME {
        let recording_session_id = match strip_recording_session_id(&mut params.arguments) {
            Ok(session_id) => session_id,
            Err(message) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return McpOutcome::BadRequest(rpc_error(id, -32602, message));
            }
        };
        let call = match ToolCall::from_tool_name(&params.name, params.arguments.clone()) {
            Ok(call) => call,
            Err(message) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return McpOutcome::BadRequest(rpc_error(id, -32602, message));
            }
        };
        let ToolCall::PluginTool(plugin) = call else {
            unreachable!("plugin_tool parser must yield ToolCall::PluginTool");
        };
        let recording_session_id =
            match canonicalize_recording_session_id(runtime, recording_session_id, auth) {
                Ok(session_id) => session_id,
                Err(message) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("invalid_arguments");
                        lc.dispatch_finished(false, Some(false), "invalid_arguments");
                    }
                    return McpOutcome::BadRequest(rpc_error(id, -32602, message));
                }
            };
        if let Some(lc) = lifecycle.as_deref() {
            lc.capture_payload_lazy("effective_arguments", || {
                crate::plugin_gateway::audit_arguments(&params.arguments)
            });
        }
        let invocation = match crate::plugin_gateway::invoke(
            runtime,
            plugin,
            recording_session_id.as_deref(),
            auth,
            crate::tool_runtime::sessions::SessionTransport::Mcp,
        )
        .await
        {
            Ok(invocation) => invocation,
            Err(SpecializedGovernanceDenial::Scope {
                required_scope,
                description,
            }) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("forbidden");
                    lc.dispatch_finished(false, Some(false), "forbidden");
                }
                return scope_forbidden(auth, Some(required_scope), description);
            }
            Err(SpecializedGovernanceDenial::Tool(result)) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("specialized_governance_denied");
                    lc.dispatch_finished(true, Some(false), "tool_error");
                }
                let mut result = result;
                let project = recording_session_id
                    .as_deref()
                    .and_then(|session_id| runtime.sessions.session_project(session_id).flatten());
                runtime.add_peer_collaboration_projection(
                    &mut result,
                    auth,
                    window,
                    project.as_deref(),
                    &ack_session_message_ids,
                );

                let result = mcp_runtime_tool_result_fallback(result, result_presentation);
                return McpOutcome::Ok(rpc_result(
                    id,
                    if stateless_2026 {
                        mcp_stateless_result(result, false)
                    } else {
                        result
                    },
                ));
            }
        };
        let ok = invocation.success();
        if let (Some(slot), Some(session_id)) = (
            correlation_out.as_deref_mut(),
            recording_session_id.as_deref(),
        ) {
            let mut correlation = crate::tool_runtime::ToolCallCorrelation::default();
            let project = runtime.sessions.session_project(session_id).flatten();
            correlation.resolved_project = project.clone();
            correlation.add_workflow_session(crate::tool_runtime::WorkflowSessionCorrelation {
                session_id: session_id.to_string(),
                project,
                relation: crate::tool_runtime::WorkflowSessionCorrelationRelation::Recording,
            });
            *slot = correlation;
        }
        if let Some(lc) = lifecycle.as_deref() {
            lc.capture_payload_lazy("specialized_governance", || {
                invocation.policy().audit_projection()
            });
            lc.dispatch_finished(true, Some(ok), if ok { "success" } else { "tool_error" });
        }
        let project = recording_session_id
            .as_deref()
            .and_then(|session_id| runtime.sessions.session_project(session_id).flatten());
        let mut result = invocation.to_mcp_result();
        runtime.add_peer_collaboration_to_mcp_call_result(
            &mut result,
            auth,
            window,
            project.as_deref(),
            &ack_session_message_ids,
        );
        return McpOutcome::Ok(rpc_result(
            id,
            if stateless_2026 {
                mcp_stateless_result(result, false)
            } else {
                result
            },
        ));
    }
    if params.name == crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME
        && via_adaptive_runtime_gateway
    {
        let recording_session_id = match strip_recording_session_id(&mut params.arguments) {
            Ok(session_id) => session_id,
            Err(message) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return McpOutcome::BadRequest(rpc_error(id, -32602, message));
            }
        };
        // Resolve once so a valid short recorder can drive the same best-effort
        // fallback Project projection as its canonical id. Defer ref errors until
        // business parsing succeeds to preserve the existing fallback precedence.
        let canonical_recording_session_id =
            canonicalize_recording_session_id(runtime, recording_session_id.clone(), auth);
        let recorder_project = || {
            canonical_recording_session_id
                .as_ref()
                .ok()
                .and_then(|session_id| session_id.as_deref())
                .and_then(|session_id| runtime.sessions.session_project(session_id).flatten())
        };
        let policy = match crate::ssh_resource_gateway::operation_policy(&params.arguments) {
            Ok(policy) => policy,
            Err(_) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.capture_payload_lazy("effective_arguments", || {
                        crate::ssh_resource_gateway::audit_arguments(&params.arguments)
                    });
                }
                let mut result =
                    crate::ssh_resource_gateway::call(runtime, params.arguments, auth).await;
                let project = recorder_project();
                runtime.add_peer_collaboration_to_mcp_call_result(
                    &mut result,
                    auth,
                    window,
                    project.as_deref(),
                    &ack_session_message_ids,
                );
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
        };
        let request = match serde_json::from_value::<crate::tool_runtime::SshResourceToolCall>(
            params.arguments.clone(),
        ) {
            Ok(request) if request.validate().is_ok() => request,
            _ => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.capture_payload_lazy("effective_arguments", || {
                        crate::ssh_resource_gateway::audit_arguments(&params.arguments)
                    });
                }
                let mut result =
                    crate::ssh_resource_gateway::call(runtime, params.arguments, auth).await;
                let project = recorder_project();
                runtime.add_peer_collaboration_to_mcp_call_result(
                    &mut result,
                    auth,
                    window,
                    project.as_deref(),
                    &ack_session_message_ids,
                );
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
        };
        let recording_session_id = match canonical_recording_session_id {
            Ok(session_id) => session_id,
            Err(message) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return McpOutcome::BadRequest(rpc_error(id, -32602, message));
            }
        };
        let invocation = match crate::ssh_resource_gateway::invoke(
            runtime,
            request,
            recording_session_id.as_deref(),
            auth,
            crate::tool_runtime::sessions::SessionTransport::Mcp,
        )
        .await
        {
            Ok(invocation) => invocation,
            Err(SpecializedGovernanceDenial::Scope {
                required_scope,
                description,
            }) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.capture_payload_lazy("specialized_governance", || policy.audit_projection());
                    lc.dispatch_failed("forbidden");
                    lc.dispatch_finished(false, Some(false), "forbidden");
                }
                return scope_forbidden(auth, Some(required_scope), description);
            }
            Err(SpecializedGovernanceDenial::Tool(result)) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.capture_payload_lazy("specialized_governance", || policy.audit_projection());
                    lc.dispatch_failed("specialized_governance_denied");
                    lc.dispatch_finished(true, Some(false), "tool_error");
                }
                let mut result = result;
                let project = recording_session_id
                    .as_deref()
                    .and_then(|session_id| runtime.sessions.session_project(session_id).flatten());
                runtime.add_peer_collaboration_projection(
                    &mut result,
                    auth,
                    window,
                    project.as_deref(),
                    &ack_session_message_ids,
                );

                let result = mcp_runtime_tool_result_fallback(result, result_presentation);
                return McpOutcome::Ok(rpc_result(
                    id,
                    if stateless_2026 {
                        mcp_stateless_result(result, false)
                    } else {
                        result
                    },
                ));
            }
        };
        let ok = invocation.success();
        if let (Some(slot), Some(session_id)) = (
            correlation_out.as_deref_mut(),
            recording_session_id.as_deref(),
        ) {
            let mut correlation = crate::tool_runtime::ToolCallCorrelation::default();
            let project = runtime.sessions.session_project(session_id).flatten();
            correlation.resolved_project = project.clone();
            correlation.add_workflow_session(crate::tool_runtime::WorkflowSessionCorrelation {
                session_id: session_id.to_string(),
                project,
                relation: crate::tool_runtime::WorkflowSessionCorrelationRelation::Recording,
            });
            *slot = correlation;
        }
        if let Some(lc) = lifecycle.as_deref() {
            lc.capture_payload_lazy("effective_arguments", || {
                crate::ssh_resource_gateway::audit_arguments(&params.arguments)
            });
            lc.capture_payload_lazy("specialized_governance", || {
                invocation.policy().audit_projection()
            });
            lc.dispatch_finished(true, Some(ok), if ok { "success" } else { "tool_error" });
        }
        let project = recording_session_id
            .as_deref()
            .and_then(|session_id| runtime.sessions.session_project(session_id).flatten());
        let mut result = invocation.to_mcp_result();
        runtime.add_peer_collaboration_to_mcp_call_result(
            &mut result,
            auth,
            window,
            project.as_deref(),
            &ack_session_message_ids,
        );
        return McpOutcome::Ok(rpc_result(
            id,
            if stateless_2026 {
                mcp_stateless_result(result, false)
            } else {
                result
            },
        ));
    }
    // Adaptive Runtime accepts only its canonical direct tools directly. Gateway
    // calls are already reduced to an admitted target and continue through the
    // same canonical authority and capability checks. App-only operations keep
    // their independent server/protocol admission.
    let goal_plan_app_admitted = server_mcp_apps_enabled && stateless_2026;
    let work_result_app_admitted = server_mcp_apps_enabled && stateless_2026;
    let agent_continuation_app_admitted = server_mcp_apps_enabled && stateless_2026;
    let job_terminal_continuation_app_admitted = server_mcp_apps_enabled && stateless_2026;
    let app_only_goal_plan_sync = goal_plan_app_admitted && params.name == "goal_plan_sync";
    let app_only_work_result_state = work_result_app_admitted && params.name == "work_result_state";
    let app_only_work_result_send_message =
        work_result_app_admitted && params.name == "work_result_send_message";
    let app_only_changes_file_diff = work_result_app_admitted && params.name == "changes_file_diff";
    let app_only_agent_continuation =
        agent_continuation_app_admitted && is_agent_continuation_app_tool_name(&params.name);
    let app_only_job_terminal_continuation = job_terminal_continuation_app_admitted
        && is_job_terminal_continuation_app_tool_name(&params.name);
    let protocol_extension_admitted = stateless_2026
        && crate::tool_runtime::stateless_operator_extension_tool_specs()
            .iter()
            .any(|spec| spec.name == params.name);
    let direct_denied = !app_only_goal_plan_sync
        && !app_only_work_result_state
        && !app_only_work_result_send_message
        && !app_only_changes_file_diff
        && !app_only_agent_continuation
        && !app_only_job_terminal_continuation
        && !protocol_extension_admitted
        && !via_adaptive_runtime_gateway
        && !is_adaptive_runtime_direct_tool(&params.name);
    if direct_denied {
        if let Some(lc) = lifecycle.as_deref() {
            lc.dispatch_failed("direct_route_denied");
            lc.dispatch_finished(false, Some(false), "direct_route_denied");
        }
        return McpOutcome::BadRequest(rpc_error(
            id,
            -32602,
            format!(
                "tool '{}' is not directly callable on Adaptive Runtime; use call_runtime_tool for admitted model-visible long-tail tools",
                params.name
            ),
        ));
    }
    // From here on, the MCP boundary has established a model-visible runtime
    // tool identity. A few MCP-only validations still happen before the
    // shared ToolRuntime kernel; preserve those failed attempts in generic
    // telemetry without creating a second record for normal kernel calls.
    let mut pre_kernel_model_ergonomics =
        ModelErgonomicsTimer::start_with_arguments(&params.name, &params.arguments);
    let artifact_presentation =
        resources::project_artifact_presentation_mode(&params.name, &params.arguments);
    let resource_tool_call = match resources::prepare_tool_call(
        &params.name,
        artifact_presentation,
        stateless_2026,
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
    let mut session_id = match strip_recording_session_id(&mut params.arguments) {
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
    // App-only synchronization/presentation calls must never let the generic
    // Stateless recording wrapper manufacture Session authority or liveness.
    // Goal Plan sync accepts only goal_id; Work Result reads carry their exact
    // business Session separately. Discard a hand-crafted unadvertised
    // recording_session_id before the kernel sees any of these calls.
    if matches!(
        params.name.as_str(),
        "goal_plan_sync" | "work_result_state" | "work_result_send_message" | "changes_file_diff"
    ) {
        session_id = None;
    } else {
        session_id = match canonicalize_recording_session_id(runtime, session_id, auth) {
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
    }
    let session_message_resolution = if stateless_2026 {
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
    // context_request remains protocol-scoped and independent from ACK policy.
    let context_sidecar_capable = stateless_2026;
    let skill_runtime_capable = stateless_2026;
    let skill_management_capable = stateless_2026;
    let memory_surface_capable = stateless_2026;
    let trace_diagnostics_capable = stateless_2026;
    let goal_plan_app_capable = goal_plan_app_admitted;
    let work_result_app_capable = work_result_app_admitted;
    let agent_continuation_app_capable = agent_continuation_app_admitted;
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
    if let Some(lc) = lifecycle.as_deref() {
        lc.capture_payload("effective_arguments", &params.arguments);
    }
    let outcome = runtime
        .call_tool_with_invocation_metadata(
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
            ToolInvocationMetadata {
                control,
                ack_session_message_ids,
                ack_ref,
                session_message_resolution,
                window_reply,
                context_request,
            },
            ToolProtocolCapabilities {
                control_sidecars: stateless_2026,
                context_sidecar: context_sidecar_capable,
                skill_runtime: skill_runtime_capable,
                skill_management: skill_management_capable,
                memory_surface: memory_surface_capable,
                trace_diagnostics: trace_diagnostics_capable,
                goal_plan_app: goal_plan_app_capable,
                work_result_app: work_result_app_capable,
                agent_continuation_app: agent_continuation_app_capable,
            },
        )
        .await;
    let model_ergonomics_completion = outcome.model_ergonomics;
    if let Some(slot) = correlation_out.as_deref_mut() {
        *slot = outcome.correlation.clone();
    }
    let mut result = match outcome.error_status {
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
    project_job_terminal_resume_suggested_call(
        app_enabled
            && job_terminal_continuation_app_admitted
            && params.name == "wait_for_job_terminal",
        &mut result,
    );
    project_tool_result_suggested_calls(&params.name, &mut result, &|target| {
        mcp_suggested_tool_call_route(target, stateless_2026)
    });
    if let Some(lc) = lifecycle.as_deref() {
        // Protocol layer produced a JSON-RPC result (not -32xxx).
        // Canonical tool success is independent of the MCP presentation signal.
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
    let mut result = match resources::adapt_tool_result(
        &params.name,
        artifact_presentation,
        result,
        resource_tool_call,
        result_presentation,
    ) {
        resources::McpResourceToolResultAdaptation::Framed(value) => value,
        resources::McpResourceToolResultAdaptation::Unhandled(result) => {
            // App-only tools use the standard CallToolResult channel too. Their
            // visibility/admission boundary, not custom result metadata, keeps
            // continuation protocol data out of ordinary model tool results.
            mcp_runtime_tool_result_fallback(result, result_presentation)
        }
    };
    if app_only_work_result_state
        || app_only_work_result_send_message
        || app_only_changes_file_diff
        || app_only_agent_continuation
        || app_only_job_terminal_continuation
    {
        // ChatGPT production has been observed to complete View-originated
        // tools/call server-side while not forwarding structuredContent back to
        // the View. Keep structuredContent canonical, but duplicate this bounded
        // app-only envelope into standard text content as a compatibility path.
        // These tools are ModelHidden/app-visible only, so ordinary model tool
        // results retain the compact text fallback.
        attach_app_tool_content_fallback(&mut result);
    }
    if app_enabled && params.name == "present_work_result" {
        // Initial model-originated presentation keeps normal model content compact.
        // The private MCP App result channel lets the mounted View recover the exact
        // bounded Work Result when a Host omits structuredContent from tool-result.
        attach_work_result_app_private_result(&mut result);
    }
    if app_only_agent_continuation {
        log_agent_continuation_app_result(
            lifecycle.as_deref(),
            &params.name,
            app_call_id.as_deref(),
            &result,
        );
    }
    if app_only_job_terminal_continuation {
        log_agent_continuation_app_result(
            lifecycle.as_deref(),
            &params.name,
            app_call_id.as_deref(),
            &result,
        );
    }
    if app_enabled {
        presentation::attach_result_app_presentation(&params.name, &mut result);
    }
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

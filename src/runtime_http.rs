use crate::action_audit::{ActionAudit, ActionAuditRecord};
use crate::json_error;
use crate::tool_request_trace::{
    estimate_json_bytes, new_trace_id, scope_active_trace, ToolRequestLifecycle,
};
use crate::tool_runtime::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallErrorStatus,
    ToolCallRequest as KernelToolCallRequest, ToolTransport,
};
use crate::tool_runtime::model_ergonomics_telemetry::ModelErgonomicsCompletion;
use crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD;
use crate::tool_runtime::{
    ListToolsOptions, ToolCall, ToolRuntime, TOOL_CALL_PARAMS_FIELD, TOOL_CALL_TOOL_FIELD,
};
use salvo::prelude::*;
use serde_json::{json, Value};
use std::sync::Arc;

mod import_http;
mod projects;

pub use import_http::import_conversation_files_to_project;
pub use projects::projects_resolve_or_register;

fn runtime(depot: &Depot) -> Option<Arc<ToolRuntime>> {
    depot.obtain::<Arc<ToolRuntime>>().ok().cloned()
}

/// Pull the [`ToolRuntime`] out of the depot, or render a 500 "Tool runtime
/// not configured" error and return `None` so the handler can bail early.
///
/// Every GPT-Actions / MCP handler opens with the same guard; this collapses
/// the seven-line `let Some(runtime) = runtime(depot) else { render; return }`
/// block into `let Some(runtime) = require_runtime(depot, res) else { return };`.
pub(crate) fn require_runtime(depot: &Depot, res: &mut Response) -> Option<Arc<ToolRuntime>> {
    match runtime(depot) {
        Some(runtime) => Some(runtime),
        None => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Tool runtime not configured",
            ));
            None
        }
    }
}

/// Parse a JSON request body as `T`, or render a 400 "Invalid JSON" error and
/// return `None` so the handler can bail early.
///
/// Mirrors the inline `match req.parse_json().await { Ok(b) => b, Err(e) => {
/// render; return } }` block repeated across every handler. The error message
/// format is byte-identical to the previous inline form.
pub(crate) async fn parse_json_body<T>(req: &mut Request, res: &mut Response) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    match req.parse_json().await {
        Ok(body) => Some(body),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            None
        }
    }
}

/// Parse an optional JSON request body while distinguishing a truly absent body
/// from malformed non-empty JSON. Empty/whitespace bodies and explicit `null`
/// preserve legacy no-argument behavior; malformed bodies fail closed with 400
/// instead of silently widening a targeted inventory request.
pub(crate) async fn parse_optional_json_body(
    req: &mut Request,
    res: &mut Response,
) -> Option<Value> {
    let payload = match req.payload().await {
        Ok(payload) => payload,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Failed to read request body: {}", e),
            ));
            return None;
        }
    };
    if payload.is_empty() || payload.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Some(Value::Object(Default::default()));
    }
    match serde_json::from_slice::<Value>(payload) {
        Ok(Value::Null) => Some(Value::Object(Default::default())),
        Ok(body) => Some(body),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            None
        }
    }
}

fn render_result(
    res: &mut Response,
    audit: &ActionAudit,
    operation: &str,
    project: Option<String>,
    result: crate::tool_runtime::ToolResult,
) {
    let status = if result.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    res.status_code(status);
    let mut event = ActionAuditRecord::new(operation.to_string(), result.success, status)
        .error(result.error.clone())
        .summary(json!({
            "output": result.output.clone(),
        }));
    event.project = project;
    audit.record(event);
    res.render(Json(result));
}

/// Return the durable ActionAudit projection for one tool result.
///
/// Privacy-sensitive computer observation tools use the same bounded
/// metadata-only projection as the Workflow Session ledger, so screenshots and
/// complete window lists never enter ActionAudit.
fn action_audit_output_for_tool(tool: &str, output: &Value) -> Value {
    crate::tool_runtime::audit_safe_result_for_tool(tool, output)
}

/// Audit and return the canonical tool result.
fn prepare_action_tools_call_response(
    audit: &ActionAudit,
    tool: &str,
    project: Option<String>,
    result: crate::tool_runtime::ToolResult,
    model_ergonomics: Option<&ModelErgonomicsCompletion>,
    correlation: &crate::tool_runtime::ToolCallCorrelation,
) -> (StatusCode, crate::tool_runtime::ToolResult) {
    let status = if result.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    let audit_output = action_audit_output_for_tool(tool, &result.output);
    let response = result;
    let mut summary = json!({"output": audit_output});
    if let Some(telemetry) = model_ergonomics
        .and_then(|completion| completion.record_for_tool_result(&response))
        .and_then(|record| serde_json::to_value(record).ok())
    {
        summary["model_ergonomics"] = telemetry;
    }
    if let Some(composition) = correlation.code_mode_composition_audit_summary() {
        summary["code_mode_composition"] = composition;
    }
    let mut event = ActionAuditRecord::new(tool.to_string(), response.success, status)
        .error(response.error.clone())
        .summary(summary);
    event.project = project;
    audit.record(event);
    (status, response)
}

fn record_action_tools_call_pre_result_failure(
    audit: &ActionAudit,
    tool: &str,
    status: StatusCode,
    model_ergonomics: Option<&ModelErgonomicsCompletion>,
    error_kind: &'static str,
) {
    let mut summary = json!({});
    if let Some(telemetry) = model_ergonomics
        .map(|completion| completion.record_for_pre_result_failure(error_kind))
        .and_then(|record| serde_json::to_value(record).ok())
    {
        summary["model_ergonomics"] = telemetry;
    }
    audit.record(ActionAuditRecord::new(tool.to_string(), false, status).summary(summary));
}

#[handler]
pub async fn tools_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(runtime) = require_runtime(depot, res) else {
        return;
    };
    let body = match req.payload().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Failed to read request body: {}", e),
            ));
            return;
        }
    };
    let options = if body.is_empty() || body.iter().all(|b| b.is_ascii_whitespace()) {
        ListToolsOptions::default()
    } else {
        match serde_json::from_slice::<ListToolsOptions>(body) {
            Ok(options) => options,
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(
                    StatusCode::BAD_REQUEST,
                    format!("Invalid listRuntimeTools request: {}", e),
                ));
                return;
            }
        }
    };
    let mut payload = runtime.list_tools_payload(options);
    payload["success"] = json!(true);
    res.render(Json(payload));
}

#[handler]
pub async fn tools_call(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let mut guard =
        ToolRequestLifecycle::new("api", new_trace_id(), "-", "POST /api/tools/call", None);
    guard.received();

    let audit = ActionAudit::start(req, depot, "/api/tools/call", "callTool");
    let Some(runtime) = runtime(depot) else {
        guard.parsed("error_runtime_missing");
        let body = serde_json::json!({
            "status": StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            "error": "Tool runtime not configured",
        });
        guard.capture_payload("final_response", &body);
        let estimated = estimate_json_bytes(&body);
        guard.response_serialized(500, estimated, Some(false), None, "error_runtime_missing");
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(body));
        guard.handler_returned(500, estimated, Some(false), None, "error_runtime_missing");
        return;
    };
    // Parse the body as a raw JSON value so we can enforce the explicit
    // tool/params envelope and emit field-aware errors that include the tool
    // name. We never echo the raw body back, so tokens/headers/env never leak
    // through error messages.
    let body: Value = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            guard.parsed("parse_error");
            let body = serde_json::json!({
                "status": StatusCode::BAD_REQUEST.as_u16(),
                "error": format!("Invalid JSON: {}", e),
            });
            guard.capture_payload("final_response", &body);
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(400, estimated, Some(false), None, "parse_error");
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(body));
            guard.handler_returned(400, estimated, Some(false), None, "parse_error");
            return;
        }
    };
    let (tool, params) = match extract_tool_call(&body) {
        Ok(pair) => pair,
        Err(msg) => {
            guard.capture_payload_lazy("raw_request_body", || tool_call_trace_raw_body(&body));
            // Params-level failure: not yet in ToolRuntime.
            guard.parsed("invalid_tool_call");
            let body = serde_json::json!({
                "status": StatusCode::BAD_REQUEST.as_u16(),
                "error": msg,
            });
            guard.capture_payload("final_response", &body);
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(400, estimated, Some(false), None, "invalid_tool_call");
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(body));
            guard.handler_returned(400, estimated, Some(false), None, "invalid_tool_call");
            return;
        }
    };
    guard.set_tool_name(Some(tool.clone()));
    guard.capture_payload_lazy("raw_request_body", || tool_call_trace_raw_body(&body));
    let window = crate::client_window::api_window(req, res);
    guard.set_client_window(Some(&window));
    guard.parsed("ok");
    guard.capture_payload_lazy("effective_arguments", || {
        tool_call_trace_effective_arguments(&tool, &params)
    });
    // dispatch_started only after argument extraction succeeds and immediately
    // before ToolRuntime dispatch.
    guard.dispatch_started();

    let session_id = extract_recording_session_id(&body);
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let active_trace_id = guard.active_trace_id();
    let outcome = scope_active_trace(
        active_trace_id,
        runtime.call_tool_with_context(
            KernelToolCallRequest {
                tool_name: tool.clone(),
                arguments: params,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: session_id.as_deref(),
                auth: auth.as_ref(),
                window: Some(&window),
                record_oauth_scope_denials: true,
                host_file_import_trust: crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
            },
        ),
    )
    .await;
    let model_ergonomics = outcome.model_ergonomics;
    match outcome.error_status {
        Some(ToolCallErrorStatus::InsufficientScope {
            required_scope,
            description,
        }) => {
            guard.dispatch_failed("insufficient_scope");
            record_action_tools_call_pre_result_failure(
                &audit,
                &tool,
                StatusCode::FORBIDDEN,
                model_ergonomics.as_ref(),
                "insufficient_scope",
            );
            guard.dispatch_finished(false, Some(false), "insufficient_scope");
            let response_body =
                crate::auth::scope_forbidden_body(auth.as_ref(), description.clone());
            guard.capture_payload("final_response", &response_body);
            let estimated = estimate_json_bytes(&response_body);
            guard.response_serialized(
                403,
                estimated,
                Some(false),
                Some(false),
                "insufficient_scope",
            );
            crate::auth::render_scope_forbidden(res, auth.as_ref(), required_scope, description);
            guard.handler_returned(
                403,
                estimated,
                Some(false),
                Some(false),
                "insufficient_scope",
            );
        }
        Some(ToolCallErrorStatus::InvalidArguments { message }) => {
            guard.dispatch_failed("invalid_arguments");
            record_action_tools_call_pre_result_failure(
                &audit,
                &tool,
                StatusCode::BAD_REQUEST,
                model_ergonomics.as_ref(),
                "invalid_arguments",
            );
            guard.dispatch_finished(false, Some(false), "invalid_arguments");
            let body = serde_json::json!({
                "status": StatusCode::BAD_REQUEST.as_u16(),
                "error": message,
            });
            guard.capture_payload("final_response", &body);
            let estimated = estimate_json_bytes(&body);
            guard.response_serialized(
                400,
                estimated,
                Some(false),
                Some(false),
                "invalid_arguments",
            );
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(body));
            guard.handler_returned(
                400,
                estimated,
                Some(false),
                Some(false),
                "invalid_arguments",
            );
        }
        None => {
            let result = outcome
                .result
                .expect("tool kernel outcome without error must include result");
            debug_assert_eq!(outcome.success, result.success);
            let tool_success = result.success;
            // HTTP/API protocol success tracks the rendered status (200 vs 400).
            let protocol_success = tool_success;
            if tool_success {
                guard.dispatch_finished(true, Some(true), "success");
            } else {
                guard.dispatch_finished(true, Some(false), "tool_error");
            }
            // Audit the tool-specific durable projection; optionally compact only the HTTP response body.
            // Trace size reflects what ChatGPT receives (post-compact when on).
            let (status, response) = prepare_action_tools_call_response(
                &audit,
                &tool,
                outcome.project,
                result,
                model_ergonomics.as_ref(),
                &outcome.correlation,
            );
            let response_value = guard
                .enabled()
                .then(|| serde_json::to_value(&response).ok())
                .flatten();
            if let Some(value) = response_value.as_ref() {
                guard.capture_payload("final_response", value);
            }
            let estimated = response_value.as_ref().and_then(estimate_json_bytes);
            let category = if tool_success { "ok" } else { "tool_error" };
            guard.response_serialized(
                status.as_u16(),
                estimated,
                Some(protocol_success),
                Some(tool_success),
                category,
            );
            res.status_code(status);
            res.render(Json(response));
            guard.handler_returned(
                status.as_u16(),
                estimated,
                Some(protocol_success),
                Some(tool_success),
                category,
            );
        }
    }
}

fn tool_call_trace_raw_body(body: &Value) -> Value {
    let Some(object) = body.as_object() else {
        return body.clone();
    };
    if object.get(TOOL_CALL_TOOL_FIELD).and_then(Value::as_str)
        != Some(crate::plugin_gateway::PLUGIN_TOOL_NAME)
    {
        return body.clone();
    }
    let plugin_arguments = object
        .get(TOOL_CALL_PARAMS_FIELD)
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| {
            let mut flattened = serde_json::Map::new();
            for (key, value) in object {
                if key == TOOL_CALL_TOOL_FIELD
                    || key == TOOL_CALL_PARAMS_FIELD
                    || key == TOOL_CALL_RECORDING_SESSION_ID_FIELD
                {
                    continue;
                }
                flattened.insert(key.clone(), value.clone());
            }
            Value::Object(flattened)
        });
    json!({
        "tool": crate::plugin_gateway::PLUGIN_TOOL_NAME,
        "arguments": crate::plugin_gateway::audit_arguments(&plugin_arguments),
        "recording_session_id_present": object
            .get(TOOL_CALL_RECORDING_SESSION_ID_FIELD)
            .is_some(),
    })
}

fn tool_call_trace_effective_arguments(tool: &str, params: &Value) -> Value {
    if tool == crate::plugin_gateway::PLUGIN_TOOL_NAME {
        crate::plugin_gateway::audit_arguments(params)
    } else {
        params.clone()
    }
}

/// Extract `(tool, params)` from the generic REST `/api/tools/call` body.
///
/// Accepted shapes:
/// - `{"tool":"list_tools"}`
/// - `{"tool":"list_tools","params":null}`
/// - `{"tool":"show_changes","params":{"project":"agent:c:p"}}`
/// - `{"tool":"git_status","params":{"project":"agent:c:p"},"recording_session_id":"wc_sess_..."}`
///
/// `params` is the only tool-argument container. Top-level
/// `recording_session_id` remains request metadata and is not injected into
/// tool arguments. Unknown top-level fields fail with migration guidance. The
/// retired `arguments` wrapper is rejected explicitly. Returns a human-readable
/// error string (never including the raw body) when the body is invalid.
fn extract_tool_call(body: &Value) -> Result<(String, Value), String> {
    let obj = body
        .as_object()
        .ok_or_else(|| "request body must be a JSON object".to_string())?;
    let tool = match obj.get(TOOL_CALL_TOOL_FIELD) {
        Some(v) => match v.as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                return Err(format!(
                    "field '{TOOL_CALL_TOOL_FIELD}' must be a non-empty string"
                ));
            }
        },
        None => {
            return Err(format!("missing required field '{TOOL_CALL_TOOL_FIELD}'"));
        }
    };
    if obj.contains_key("arguments") {
        return Err(
            "field 'arguments' is no longer supported; move tool arguments under 'params'"
                .to_string(),
        );
    }
    let mut unexpected = obj
        .keys()
        .filter(|key| {
            key.as_str() != TOOL_CALL_TOOL_FIELD
                && key.as_str() != TOOL_CALL_PARAMS_FIELD
                && key.as_str() != TOOL_CALL_RECORDING_SESSION_ID_FIELD
        })
        .cloned()
        .collect::<Vec<_>>();
    unexpected.sort();
    if !unexpected.is_empty() {
        let fields = unexpected
            .iter()
            .map(|field| format!("'{field}'"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "unexpected top-level field(s) {fields}; move tool arguments under 'params'"
        ));
    }
    let params = obj
        .get(TOOL_CALL_PARAMS_FIELD)
        .cloned()
        .unwrap_or(Value::Null);
    Ok((tool, params))
}

fn extract_recording_session_id(body: &Value) -> Option<String> {
    body.as_object()
        .and_then(|obj| obj.get(TOOL_CALL_RECORDING_SESSION_ID_FIELD))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_gpt_action_gateway(body: Value) -> Result<(String, Value), String> {
    let mut object = body
        .as_object()
        .cloned()
        .ok_or_else(|| "call_runtime_tool body must be a JSON object".to_string())?;
    if object.len() != 2 || !object.contains_key("tool") || !object.contains_key("arguments") {
        return Err("call_runtime_tool accepts exactly {tool, arguments}".to_string());
    }
    let tool = object
        .remove("tool")
        .and_then(|value| value.as_str().map(str::trim).map(str::to_string))
        .filter(|tool| !tool.is_empty())
        .ok_or_else(|| "call_runtime_tool field 'tool' must be a non-empty string".to_string())?;
    let arguments = object
        .remove("arguments")
        .filter(Value::is_object)
        .ok_or_else(|| "call_runtime_tool field 'arguments' must be an object".to_string())?;
    Ok((tool, arguments))
}

fn rewrite_gpt_action_file_params(arguments: &mut Value) -> Result<(), String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| "GPT Action request body must be a JSON object".to_string())?;
    let Some(refs) = object.get_mut("openaiFileIdRefs") else {
        return Ok(());
    };
    let refs = refs
        .as_array_mut()
        .ok_or_else(|| "openaiFileIdRefs must be an array".to_string())?;
    for file_ref in refs {
        let source = file_ref.as_object().ok_or_else(|| {
            "openaiFileIdRefs entries must be host file-reference objects".to_string()
        })?;
        if source
            .keys()
            .any(|key| !matches!(key.as_str(), "name" | "id" | "mime_type" | "download_link"))
        {
            return Err("GPT Action file references contain unsupported fields".to_string());
        }
        let download_url = source
            .get("download_link")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "GPT Action file reference requires download_link".to_string())?
            .to_string();
        let mut canonical = serde_json::Map::new();
        canonical.insert("download_url".to_string(), Value::String(download_url));
        if let Some(value) = source.get("id").and_then(Value::as_str) {
            canonical.insert("file_id".to_string(), Value::String(value.to_string()));
        }
        if let Some(value) = source.get("mime_type").and_then(Value::as_str) {
            canonical.insert("mime_type".to_string(), Value::String(value.to_string()));
        }
        if let Some(value) = source.get("name").and_then(Value::as_str) {
            canonical.insert("file_name".to_string(), Value::String(value.to_string()));
        }
        *file_ref = Value::Object(canonical);
    }
    Ok(())
}

fn gpt_action_admit_target(path_tool: &str, target: &str) -> Result<(), String> {
    use crate::model_surface::AdaptiveRuntimeGatewayTargetRoute;

    if !webcodex_tool_contracts::gpt_action_tool_supported(target) {
        return Err(format!(
            "runtime tool '{target}' is not available through GPT Actions"
        ));
    }
    let route = crate::model_surface::gpt_action_gateway_target_route(target);
    if path_tool == crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME {
        return match route {
            AdaptiveRuntimeGatewayTargetRoute::Gateway => Ok(()),
            AdaptiveRuntimeGatewayTargetRoute::Direct => Err(format!(
                "runtime tool '{target}' is a direct GPT Action; use /api/actions/{target}"
            )),
            AdaptiveRuntimeGatewayTargetRoute::Recursive => {
                Err("call_runtime_tool cannot target itself".to_string())
            }
            AdaptiveRuntimeGatewayTargetRoute::Unknown => Err(format!(
                "runtime tool '{target}' is not admitted on the adaptive runtime surface"
            )),
        };
    }
    if path_tool != target {
        return Err("GPT Action direct path/tool mismatch".to_string());
    }
    match route {
        AdaptiveRuntimeGatewayTargetRoute::Direct => Ok(()),
        AdaptiveRuntimeGatewayTargetRoute::Gateway => Err(format!(
            "runtime tool '{target}' is long-tail; use /api/actions/call_runtime_tool"
        )),
        AdaptiveRuntimeGatewayTargetRoute::Recursive
        | AdaptiveRuntimeGatewayTargetRoute::Unknown => Err(format!(
            "runtime tool '{target}' is not a direct GPT Action"
        )),
    }
}

fn gpt_action_suggested_tool_call_route(
    target: &str,
) -> crate::model_surface::SuggestedToolCallRoute {
    use crate::model_surface::{AdaptiveRuntimeGatewayTargetRoute, SuggestedToolCallRoute};

    if !webcodex_tool_contracts::gpt_action_tool_supported(target) {
        return SuggestedToolCallRoute::Unavailable;
    }
    match crate::model_surface::gpt_action_gateway_target_route(target) {
        AdaptiveRuntimeGatewayTargetRoute::Direct => SuggestedToolCallRoute::Direct,
        AdaptiveRuntimeGatewayTargetRoute::Gateway => SuggestedToolCallRoute::Gateway(
            crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
        ),
        AdaptiveRuntimeGatewayTargetRoute::Recursive
        | AdaptiveRuntimeGatewayTargetRoute::Unknown => SuggestedToolCallRoute::Unavailable,
    }
}

#[handler]
pub async fn gpt_action_invoke(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(path_tool) = req.param::<String>("tool_name") else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "missing GPT Action tool name",
        ));
        return;
    };
    let Some(runtime) = require_runtime(depot, res) else {
        return;
    };
    let body: Value = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {error}"),
            ));
            return;
        }
    };
    let (tool, arguments) = if path_tool == crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
    {
        match parse_gpt_action_gateway(body) {
            Ok(parsed) => parsed,
            Err(message) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(StatusCode::BAD_REQUEST, message));
                return;
            }
        }
    } else {
        if !body.is_object() {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                "GPT Action request body must be a JSON object",
            ));
            return;
        }
        (path_tool.clone(), body)
    };
    if let Err(message) = gpt_action_admit_target(&path_tool, &tool) {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(StatusCode::BAD_REQUEST, message));
        return;
    }
    let mut arguments = arguments;
    if tool == "import_conversation_files_to_project" {
        if let Err(message) = rewrite_gpt_action_file_params(&mut arguments) {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, message));
            return;
        }
    }

    let recording_session_id = arguments
        .as_object()
        .and_then(|object| object.get(TOOL_CALL_RECORDING_SESSION_ID_FIELD))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let window = crate::client_window::api_window(req, res);
    let import_provenance = if tool == "import_conversation_files_to_project" {
        HostFileImportTrust::GptActionOpenAiHost
    } else {
        HostFileImportTrust::Untrusted
    };
    let audit = ActionAudit::start(req, depot, "/api/actions/{tool_name}", "gpt_action");
    let outcome = runtime
        .call_tool_with_context(
            KernelToolCallRequest {
                tool_name: tool.clone(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: recording_session_id.as_deref(),
                auth: auth.as_ref(),
                window: Some(&window),
                record_oauth_scope_denials: true,
                host_file_import_trust: import_provenance,
            },
        )
        .await;

    match outcome.error_status {
        Some(ToolCallErrorStatus::InsufficientScope {
            required_scope,
            description,
        }) => {
            record_action_tools_call_pre_result_failure(
                &audit,
                &tool,
                StatusCode::FORBIDDEN,
                outcome.model_ergonomics.as_ref(),
                "insufficient_scope",
            );
            crate::auth::render_scope_forbidden(res, auth.as_ref(), required_scope, description);
        }
        Some(ToolCallErrorStatus::InvalidArguments { message }) => {
            record_action_tools_call_pre_result_failure(
                &audit,
                &tool,
                StatusCode::BAD_REQUEST,
                outcome.model_ergonomics.as_ref(),
                "invalid_arguments",
            );
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, message));
        }
        None => {
            let result = outcome
                .result
                .expect("tool kernel outcome without error must include result");
            let (status, mut response) = prepare_action_tools_call_response(
                &audit,
                &tool,
                outcome.project,
                result,
                outcome.model_ergonomics.as_ref(),
                &outcome.correlation,
            );
            // ActionAudit above records canonical ToolRuntime truth. Only the
            // response copy is projected to the callable Adaptive Action route.
            crate::model_surface::project_tool_result_suggested_calls(
                &tool,
                &mut response,
                &gpt_action_suggested_tool_call_route,
            );
            res.status_code(status);
            res.render(Json(response));
        }
    }
}

#[handler]
pub async fn runtime_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/runtime/status", "getRuntimeStatus");
    let Some(runtime) = require_runtime(depot, res) else {
        return;
    };
    // Body remains optional for compatibility. Non-empty malformed JSON must
    // fail closed instead of silently widening a focused request to fleet-wide.
    let Some(arguments) = parse_optional_json_body(req, res).await else {
        return;
    };
    let call = match ToolCall::from_tool_name("runtime_status", arguments) {
        Ok(call) => call,
        Err(error) => {
            render_result(
                res,
                &audit,
                "runtime_status",
                None,
                crate::tool_runtime::ToolResult::err(error),
            );
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime.dispatch_with_auth(call, auth.as_ref()).await;
    render_result(res, &audit, "runtime_status", None, result);
}

#[cfg(test)]
mod job_action_routing_tests {
    use super::*;

    #[test]
    fn stop_job_actions_admission_and_followup_use_definition_owned_gateway_policy() {
        let gateway = crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME;
        assert!(gpt_action_admit_target(gateway, "stop_job").is_ok());
        assert!(gpt_action_admit_target("stop_job", "stop_job").is_err());
        assert_eq!(
            gpt_action_suggested_tool_call_route("stop_job"),
            crate::model_surface::SuggestedToolCallRoute::Gateway(gateway)
        );
        for definition in webcodex_tool_contracts::model_visible_tool_definitions() {
            if definition.gpt_action_exposure()
                == webcodex_tool_contracts::ToolGptActionExposure::GatewayOnly
            {
                assert!(gpt_action_admit_target(gateway, definition.name).is_ok());
                assert!(gpt_action_admit_target(definition.name, definition.name).is_err());
            }
        }
        assert!(gpt_action_admit_target(gateway, "cancel_job").is_err());
        assert!(gpt_action_admit_target(gateway, gateway).is_err());
    }
}

#[cfg(test)]
#[path = "runtime_http_tests.rs"]
mod tests;

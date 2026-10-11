//! Ordered MCP call pipeline: resolve the adapter envelope, route specialized
//! gateways or admit a runtime call, execute once, then project its response.
use super::*;

mod response;
mod specialized;
mod surface;
use response::RuntimeResponse;
use surface::RuntimeSurface;

struct CallContext<'a> {
    runtime: &'a ToolRuntime,
    id: Option<Value>,
    auth: Option<&'a AuthContext>,
    stateless_2026: bool,
    app_enabled: bool,
    server_mcp_apps_enabled: bool,
    host_file_import_trust: HostFileImportTrust,
    window: Option<&'a crate::client_window::ClientWindow>,
}

struct CallDiagnostics<'a> {
    lifecycle: Option<&'a mut ToolRequestLifecycle>,
    model_ergonomics_out: Option<&'a mut Option<ModelErgonomicsRecord>>,
    correlation_out: Option<&'a mut crate::tool_runtime::ToolCallCorrelation>,
}

struct ParsedCall {
    params: McpToolCallParams,
    invocation: McpInvocationEnvelope,
    raw_mcp_arguments: Value,
    via_adaptive_runtime_gateway: bool,
    app_call_id: Option<String>,
    result_presentation: McpToolResultPresentation,
}

pub(in crate::mcp) async fn handle_call(
    runtime: &ToolRuntime,
    request_params: Value,
    id: Option<Value>,
    auth: Option<&AuthContext>,
    stateless_2026: bool,
    app_enabled: bool,
    server_mcp_apps_enabled: bool,
    host_file_import_trust: HostFileImportTrust,
    window: Option<&crate::client_window::ClientWindow>,
    lifecycle: Option<&mut ToolRequestLifecycle>,
    model_ergonomics_out: Option<&mut Option<ModelErgonomicsRecord>>,
    correlation_out: Option<&mut crate::tool_runtime::ToolCallCorrelation>,
) -> McpOutcome {
    let context = CallContext {
        runtime,
        id,
        auth,
        stateless_2026,
        app_enabled,
        server_mcp_apps_enabled,
        host_file_import_trust,
        window,
    };
    let mut diagnostics = CallDiagnostics {
        lifecycle,
        model_ergonomics_out,
        correlation_out,
    };
    let call = match ParsedCall::prepare(&context, &mut diagnostics, request_params).await {
        Ok(call) => call,
        Err(outcome) => return outcome,
    };
    match call.params.name.as_str() {
        crate::mcp_gateway::MCP_TOOL_NAME => {
            return specialized::local_mcp(&context, &mut diagnostics, call).await
        }
        crate::plugin_gateway::PLUGIN_TOOL_NAME => {
            return specialized::plugin(&context, &mut diagnostics, call).await
        }
        crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME
            if call.via_adaptive_runtime_gateway =>
        {
            return specialized::ssh_resource(&context, &mut diagnostics, call).await
        }
        _ => {}
    }
    let prepared = match PreparedRuntimeCall::prepare(&context, &mut diagnostics, call) {
        Ok(prepared) => prepared,
        Err(outcome) => return outcome,
    };
    prepared.run(&context, &mut diagnostics).await
}

impl ParsedCall {
    async fn prepare(
        context: &CallContext<'_>,
        diagnostics: &mut CallDiagnostics<'_>,
        request_params: Value,
    ) -> Result<Self, McpOutcome> {
        let runtime = context.runtime;
        let stateless_2026 = context.stateless_2026;
        let app_enabled = context.app_enabled;
        let id = context.id.clone();
        let lifecycle = &mut diagnostics.lifecycle;
        let result_presentation = McpToolResultPresentation::from_request_params(&request_params);
        if let (Some(lc), Some(name), Some(arguments)) = (
            lifecycle.as_deref(),
            request_params.get("name").and_then(Value::as_str),
            request_params.get("arguments"),
        ) {
            lc.capture_request_diagnostic(name, arguments);
        }
        let mut params: McpToolCallParams = match serde_json::from_value(request_params) {
            Ok(params) => params,
            Err(e) => {
                return Err(McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    format!("Invalid params: {}", e),
                )));
            }
        };
        // OpenAI's search contract is a host-only adapter. It composes the ordinary
        // authorized resource service and never becomes a generic runtime gateway.
        if params.name == "search_mentions" {
            return Err(search_mentions(context, params.arguments).await);
        }
        let app_call_id = if stateless_2026 && is_host_continuation_app_tool_name(&params.name) {
            match strip_agent_continuation_app_call_id(&mut params.arguments) {
                Ok(app_call_id) => app_call_id,
                Err(message) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("invalid_arguments");
                        lc.dispatch_finished(false, Some(false), "invalid_arguments");
                    }
                    return Err(McpOutcome::BadRequest(rpc_error(id, -32602, message)));
                }
            }
        } else {
            None
        };
        if let Some(lc) = lifecycle.as_deref_mut() {
            lc.set_app_call_id(app_call_id.clone());
        }
        // Preserve the model-supplied direct MCP arguments for full tracing before
        // adapter-owned invocation metadata is removed. Specialized gateways keep
        // their existing bounded audit projections below.
        let raw_mcp_arguments = params.arguments.clone();
        let via_adaptive_runtime_gateway = params.name == ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME;
        let invocation = match parse_mcp_invocation_envelope(
            &params.name,
            &mut params.arguments,
            stateless_2026,
        ) {
            Ok(invocation) => invocation,
            Err(message) => {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return Err(McpOutcome::BadRequest(rpc_error(id, -32602, message)));
            }
        };
        if via_adaptive_runtime_gateway {
            let (target, arguments) =
                match unwrap_adaptive_runtime_gateway_arguments(params.arguments) {
                    Ok(target) => target,
                    Err(message) => {
                        return Err(McpOutcome::BadRequest(rpc_error(id, -32602, message)));
                    }
                };
            match mcp_adaptive_runtime_gateway_target_route(&target, stateless_2026) {
                crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Gateway
                | crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Direct => {
                    if app_enabled && presentation::tool_requires_direct_app_presentation(&target) {
                        if let Some(lc) = lifecycle.as_deref() {
                            lc.dispatch_failed("direct_presentation_required");
                            lc.dispatch_finished(
                                false,
                                Some(false),
                                "direct_presentation_required",
                            );
                        }
                        return Err(McpOutcome::BadRequest(rpc_error(
                        id,
                        -32602,
                        format!(
                            "call_runtime_tool cannot invoke MCP App presentation tool '{target}' when MCP Apps are enabled; call '{target}' directly so the Host receives the required App resource metadata"
                        ),
                    )));
                    }
                    if let Err(message) =
                        validate_mcp_invocation_envelope_for_target(&invocation, &target)
                    {
                        if let Some(lc) = lifecycle.as_deref() {
                            lc.dispatch_failed("invalid_arguments");
                            lc.dispatch_finished(false, Some(false), "invalid_arguments");
                        }
                        return Err(McpOutcome::BadRequest(rpc_error(id, -32602, message)));
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
                        runtime.runtime_info.mcp_text_json_compat_enabled,
                        result_presentation,
                    );
                    return Err(McpOutcome::Ok(rpc_result(
                        id,
                        if stateless_2026 {
                            mcp_stateless_result(rendered, false)
                        } else {
                            rendered
                        },
                    )));
                }
                crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Recursive => {
                    return Err(McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    "call_runtime_tool cannot target itself through the adaptive runtime gateway",
                )));
                }
            }
        }
        if let Some(lc) = lifecycle.as_deref_mut() {
            lc.set_tool_name(Some(params.name.clone()));
        }
        if let Some(lc) = lifecycle.as_deref() {
            lc.capture_payload_lazy("raw_arguments", || {
                if params.name == crate::plugin_gateway::PLUGIN_TOOL_NAME {
                    crate::plugin_gateway::audit_arguments(&params.arguments)
                } else if params.name == crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME {
                    crate::ssh_resource_gateway::audit_arguments(&params.arguments)
                } else if via_adaptive_runtime_gateway {
                    json!({"tool": params.name, "arguments_present": true})
                } else {
                    raw_mcp_arguments.clone()
                }
            });
        }
        // Emit dispatch_started only after params parse succeeds and before
        // ToolRuntime work begins.
        if let Some(lc) = lifecycle.as_deref() {
            lc.dispatch_started();
        }
        Ok(Self {
            params,
            invocation,
            raw_mcp_arguments,
            via_adaptive_runtime_gateway,
            app_call_id,
            result_presentation,
        })
    }
}

struct PreparedRuntimeCall {
    params: McpToolCallParams,
    metadata: ToolInvocationMetadata,
    session_id: Option<String>,
    response: RuntimeResponse,
}

impl PreparedRuntimeCall {
    fn prepare(
        context: &CallContext<'_>,
        diagnostics: &mut CallDiagnostics<'_>,
        call: ParsedCall,
    ) -> Result<Self, McpOutcome> {
        let ParsedCall {
            mut params,
            invocation,
            raw_mcp_arguments,
            via_adaptive_runtime_gateway,
            app_call_id,
            result_presentation,
        } = call;
        let surface = RuntimeSurface::admit(
            context,
            diagnostics,
            &mut params,
            via_adaptive_runtime_gateway,
            result_presentation,
        )?;
        let McpInvocationEnvelope {
            recording_session_selector,
            metadata,
        } = invocation;
        let runtime = context.runtime;
        let auth = context.auth;
        let stateless_2026 = context.stateless_2026;
        let id = context.id.clone();
        let lifecycle = &mut diagnostics.lifecycle;
        let model_ergonomics_out = &mut diagnostics.model_ergonomics_out;
        // From here on, the MCP boundary has established a model-visible runtime
        // tool identity. A few MCP-only validations still happen before the
        // shared ToolRuntime kernel; preserve those failed attempts in generic
        // telemetry without creating a second record for normal kernel calls.
        // Preserve omission versus explicit false before canonical bool parsing.
        project_mcp_model_argument_defaults(&params.name, &mut params.arguments);
        let invocation_facts = crate::tool_runtime::model_ergonomics_telemetry::invocation::InvocationFacts::from_arguments(&raw_mcp_arguments);
        let mut pre_kernel_model_ergonomics =
            ModelErgonomicsTimer::start_with_arguments(&params.name, &params.arguments);
        if stateless_2026 {
            if let Some(timer) = pre_kernel_model_ergonomics.as_mut() {
                timer.invocation = invocation_facts.clone();
            }
        }
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
                return Err(McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    error.message(),
                )));
            }
        };
        let mut session_id = recording_session_selector;
        // App-only synchronization/presentation calls must never let the generic
        // Stateless recording wrapper manufacture Session authority or liveness.
        // Goal Plan sync accepts only goal_id; Work Result reads carry their exact
        // business Session separately. Discard a hand-crafted unadvertised
        // recorder provenance before the kernel sees any of these calls.
        if surface.discard_recorder {
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
                    return Err(McpOutcome::BadRequest(rpc_error(id, -32602, message)));
                }
            };
        }
        if metadata.session_message_resolution.is_some() && session_id.is_none() {
            let message =
                "field '_wc.resolve' requires '_wc.record' for the exact target Workflow Session"
                    .to_string();
            if let Some(lc) = lifecycle.as_deref() {
                lc.dispatch_failed("invalid_arguments");
                lc.dispatch_finished(false, Some(false), "invalid_arguments");
            }
            return Err(McpOutcome::BadRequest(rpc_error(id, -32602, message)));
        }

        Ok(Self {
            params,
            metadata,
            session_id,
            response: RuntimeResponse {
                surface,
                artifact_presentation,
                resource_tool_call,
                result_presentation,
                invocation_facts,
                app_call_id,
            },
        })
    }

    async fn run(
        self,
        context: &CallContext<'_>,
        diagnostics: &mut CallDiagnostics<'_>,
    ) -> McpOutcome {
        let Self {
            params,
            metadata,
            session_id,
            response,
        } = self;
        let compact_execution = metadata.compact_execution;
        if let Some(lc) = diagnostics.lifecycle.as_deref() {
            lc.capture_payload("effective_arguments", &params.arguments);
        }
        let outcome = context
            .runtime
            .call_tool_with_invocation_metadata(
                KernelToolCallRequest {
                    tool_name: params.name.clone(),
                    arguments: params.arguments,
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: session_id.as_deref(),
                    auth: context.auth,
                    window: context.window,
                    record_oauth_scope_denials: false,
                    host_file_import_trust: context.host_file_import_trust,
                },
                metadata,
                response.surface.capabilities,
            )
            .await;
        response.finish(
            context,
            diagnostics,
            &params.name,
            compact_execution,
            outcome,
        )
    }
}

async fn search_mentions(context: &CallContext<'_>, arguments: Value) -> McpOutcome {
    let runtime = context.runtime;
    let auth = context.auth;
    let stateless_2026 = context.stateless_2026;
    let server_mcp_apps_enabled = context.server_mcp_apps_enabled;
    let id = context.id.clone();
    if !server_mcp_apps_enabled || !stateless_2026 {
        return McpOutcome::BadRequest(rpc_error(
            id,
            -32602,
            "Resource mentions are unavailable on this protocol surface",
        ));
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct MentionQuery {
        query: String,
    }
    let query = match serde_json::from_value::<MentionQuery>(arguments) {
        Ok(value) if value.query.chars().count() <= 200 && !value.query.contains('\0') => {
            value.query
        }
        _ => return McpOutcome::BadRequest(rpc_error(id, -32602, "Invalid mention query")),
    };
    let mut items = Vec::new();
    let mut incomplete = Vec::new();
    for (kind, tool, label) in [
        (
            webcodex_tool_contracts::tool_call::WebcodexResourceKind::Project,
            "list_projects",
            "project",
        ),
        (
            webcodex_tool_contracts::tool_call::WebcodexResourceKind::Goal,
            "list_goals",
            "goal",
        ),
    ] {
        if crate::tool_runtime::kernel::check_runtime_tool_scope(auth, tool).is_err() {
            continue;
        }
        let found = runtime
            .search_webcodex_resources(
                kind,
                Some(query.clone()),
                None,
                None,
                None,
                Some(10),
                None,
                auth,
            )
            .await;
        if found.success {
            if let Some(rows) = found.output["items"].as_array() {
                items.extend(rows.iter().cloned());
            }
            if found.output["list_truncated"] == true {
                incomplete.push(label);
            }
        } else {
            incomplete.push(label);
        }
    }
    let mut result = json!({"content":[],"structuredContent":{"items":items}});
    if !incomplete.is_empty() {
        result["_meta"] = json!({"webcodex/incompleteResourceKinds":incomplete});
        if result["structuredContent"]["items"]
            .as_array()
            .is_some_and(|items| items.is_empty())
        {
            result["isError"] = json!(true);
            result["content"] = json!([{"type":"text","text":"Resource search is incomplete; use search_webcodex_resources for domain details."}]);
        }
    }
    return McpOutcome::Ok(rpc_result(id, mcp_stateless_result(result, false)));
}

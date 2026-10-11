//! Existing heterogeneous gateway paths, each retaining its own governance and audit.
use super::*;

pub(super) async fn local_mcp(
    context: &CallContext<'_>,
    diagnostics: &mut CallDiagnostics<'_>,
    call: ParsedCall,
) -> McpOutcome {
    let runtime = context.runtime;
    let auth = context.auth;
    let stateless_2026 = context.stateless_2026;
    let window = context.window;
    let id = context.id.clone();
    let lifecycle = &mut diagnostics.lifecycle;
    let ParsedCall {
        params, invocation, ..
    } = call;
    let ack_session_message_ids = invocation.metadata.ack_session_message_ids;
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

pub(super) async fn plugin(
    context: &CallContext<'_>,
    diagnostics: &mut CallDiagnostics<'_>,
    call: ParsedCall,
) -> McpOutcome {
    let runtime = context.runtime;
    let auth = context.auth;
    let stateless_2026 = context.stateless_2026;
    let window = context.window;
    let id = context.id.clone();
    let lifecycle = &mut diagnostics.lifecycle;
    let correlation_out = &mut diagnostics.correlation_out;
    let ParsedCall {
        params,
        result_presentation,
        invocation,
        ..
    } = call;
    let ack_session_message_ids = invocation.metadata.ack_session_message_ids;
    let recording_session_selector = invocation.recording_session_selector;
    let recording_session_id = recording_session_selector.clone();
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

            let result = mcp_runtime_tool_result_fallback(
                result,
                runtime.runtime_info.mcp_text_json_compat_enabled,
                result_presentation,
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

pub(super) async fn ssh_resource(
    context: &CallContext<'_>,
    diagnostics: &mut CallDiagnostics<'_>,
    call: ParsedCall,
) -> McpOutcome {
    let runtime = context.runtime;
    let auth = context.auth;
    let stateless_2026 = context.stateless_2026;
    let window = context.window;
    let id = context.id.clone();
    let lifecycle = &mut diagnostics.lifecycle;
    let correlation_out = &mut diagnostics.correlation_out;
    let ParsedCall {
        params,
        result_presentation,
        invocation,
        ..
    } = call;
    let ack_session_message_ids = invocation.metadata.ack_session_message_ids;
    let recording_session_selector = invocation.recording_session_selector;
    let recording_session_id = recording_session_selector.clone();
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

            let result = mcp_runtime_tool_result_fallback(
                result,
                runtime.runtime_info.mcp_text_json_compat_enabled,
                result_presentation,
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

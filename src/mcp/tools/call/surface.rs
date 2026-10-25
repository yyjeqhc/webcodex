//! MCP surface admission and exact Work Result Window selection.
//! This supplies capabilities only; canonical runtime authorization still follows.
use super::*;

pub(super) struct RuntimeSurface {
    pub(super) capabilities: ToolProtocolCapabilities,
    pub(super) work_result_thread_context: Option<Value>,
    pub(super) discard_recorder: bool,
    pub(super) echo_app_content: bool,
    pub(super) host_continuation: bool,
}

impl RuntimeSurface {
    pub(super) fn admit(
        context: &CallContext<'_>,
        diagnostics: &mut CallDiagnostics<'_>,
        params: &mut McpToolCallParams,
        via_adaptive_runtime_gateway: bool,
        result_presentation: McpToolResultPresentation,
    ) -> Result<Self, McpOutcome> {
        let runtime = context.runtime;
        let auth = context.auth;
        let stateless_2026 = context.stateless_2026;
        let server_mcp_apps_enabled = context.server_mcp_apps_enabled;
        let window = context.window;
        let id = context.id.clone();
        let lifecycle = &mut diagnostics.lifecycle;
        // Adaptive Runtime accepts only its canonical direct tools directly. Gateway
        // calls are already reduced to an admitted target and continue through the
        // same canonical authority and capability checks. App-only operations keep
        // their independent server/protocol admission.
        let goal_plan_app_admitted = server_mcp_apps_enabled && stateless_2026;
        let work_result_app_admitted = server_mcp_apps_enabled && stateless_2026;
        let agent_continuation_app_admitted = server_mcp_apps_enabled && stateless_2026;
        let job_terminal_continuation_app_admitted = server_mcp_apps_enabled && stateless_2026;
        let app_only_goal_plan_sync = goal_plan_app_admitted && params.name == "sync_goal_plan";
        let work_result_thread_panel =
            work_result_app_admitted && params.name == WORK_RESULT_THREAD_ENTRYPOINT_TOOL_NAME;
        let mut work_result_thread_context = None;
        if work_result_thread_panel {
            let empty_arguments = params
                .arguments
                .as_object()
                .is_some_and(serde_json::Map::is_empty);
            if !empty_arguments {
                if let Some(lc) = lifecycle.as_deref() {
                    lc.dispatch_failed("invalid_arguments");
                    lc.dispatch_finished(false, Some(false), "invalid_arguments");
                }
                return Err(McpOutcome::BadRequest(rpc_error(
                    id,
                    -32602,
                    "Work Result thread panel entrypoint accepts only an empty argument object",
                )));
            }
            let binding = match work_result_thread_binding(runtime, auth, window) {
                Ok(Some(binding)) => binding,
                Ok(None) => {
                    // There is no domain target to read, but absence must not bypass
                    // the same canonical scope required by a bound state read.
                    match check_runtime_tool_scope(auth, "get_work_result_state") {
                        Ok(()) => {}
                        Err(ToolCallErrorStatus::InsufficientScope {
                            required_scope,
                            description,
                        }) => {
                            return Err(tool_scope_denied(
                                context,
                                lifecycle.as_deref(),
                                required_scope,
                                description,
                            ));
                        }
                        Err(ToolCallErrorStatus::InvalidArguments { message }) => {
                            if let Some(lc) = lifecycle.as_deref() {
                                lc.dispatch_failed("invalid_arguments");
                                lc.dispatch_finished(false, Some(false), "invalid_arguments");
                            }
                            return Err(McpOutcome::BadRequest(rpc_error(id, -32602, message)));
                        }
                    }
                    let mut result = mcp_runtime_tool_result_fallback(
                        ToolResult::ok(json!({"work_result": null})),
                        runtime.runtime_info.mcp_text_json_compat_enabled,
                        result_presentation,
                    );
                    attach_work_result_app_private_result(&mut result, None);
                    result["_meta"][WORK_RESULT_THREAD_CONTEXT_META_KEY] =
                        json!({"empty": true, "session_id": null});
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_finished(true, Some(true), "success");
                    }
                    return Err(McpOutcome::Ok(rpc_result(
                        id,
                        mcp_stateless_result(result, false),
                    )));
                }
                Err(message) => {
                    if let Some(lc) = lifecycle.as_deref() {
                        lc.dispatch_failed("invalid_context");
                        lc.dispatch_finished(false, Some(false), "invalid_context");
                    }
                    return Err(McpOutcome::BadRequest(rpc_error(id, -32602, message)));
                }
            };
            work_result_thread_context = Some(json!({"session_id": binding.session_id.clone()}));
            params.name = "get_work_result_state".to_string();
            params.arguments = match binding.session_id {
                Some(session_id) => json!({"project": binding.project, "session_id": session_id}),
                None => json!({"project": binding.project}),
            };
        }
        let app_only_artifact_read =
            server_mcp_apps_enabled && stateless_2026 && params.name == "read_app_artifact_chunk";
        let app_only_work_result_state =
            work_result_app_admitted && params.name == "get_work_result_state";
        let app_only_work_result_activity_detail =
            work_result_app_admitted && params.name == "read_work_result_activity_detail";
        let app_only_work_result_send_message =
            work_result_app_admitted && params.name == "send_work_result_message";
        let app_only_changes_file_diff =
            work_result_app_admitted && params.name == "read_changed_file_diff";
        let app_only_agent_continuation =
            agent_continuation_app_admitted && is_agent_continuation_app_tool_name(&params.name);
        let app_only_job_terminal_continuation = job_terminal_continuation_app_admitted
            && is_job_terminal_continuation_app_tool_name(&params.name);
        let protocol_extension_admitted =
            stateless_2026 && runtime_tool_operator_extension_family(&params.name).is_some();
        let workbench_view_call = server_mcp_apps_enabled
            && stateless_2026
            && (super::super::super::workbench::CANONICAL_TOOLS.contains(&params.name.as_str())
                || matches!(
                    params.name.as_str(),
                    "search_webcodex_resources"
                        | "read_webcodex_resource"
                        | "list_sessions"
                        | "open_webcodex_workbench"
                ));
        // The Workbench retains independently admitted App-only Goal controls.
        // This exception never admits a gateway target or advertises a model route.
        if !context.exposure.allows(&params.name)
            && !(workbench_view_call && !via_adaptive_runtime_gateway)
        {
            return Err(McpOutcome::BadRequest(rpc_error(
                id,
                -32602,
                "Tool is unavailable on the configured MCP tool surface; refresh tools/list",
            )));
        }
        let direct_denied = !workbench_view_call
            && !app_only_artifact_read
            && !app_only_goal_plan_sync
            && !app_only_work_result_state
            && !app_only_work_result_activity_detail
            && !app_only_work_result_send_message
            && !app_only_changes_file_diff
            && !app_only_agent_continuation
            && !app_only_job_terminal_continuation
            && !protocol_extension_admitted
            && !via_adaptive_runtime_gateway
            && !context.exposure.is_direct(context.surface, &params.name);
        if direct_denied {
            if let Some(lc) = lifecycle.as_deref() {
                lc.dispatch_failed("direct_route_denied");
                lc.dispatch_finished(false, Some(false), "direct_route_denied");
            }
            // A recovery hint must name a real, currently admitted route. Retired
            // names and unavailable App tools cannot become valid through a gateway.
            let message = if mcp_adaptive_runtime_gateway_target_route(&params.name, stateless_2026)
                != crate::model_surface::AdaptiveRuntimeGatewayTargetRoute::Gateway
            {
                format!(
                    "tool '{}' is unavailable on the current MCP surface; refresh tools/list and select a current callable",
                    params.name
                )
            } else if !crate::tool_runtime::kernel::runtime_tool_scope_allows_discovery(
                auth,
                &params.name,
            ) {
                format!(
                    "tool '{}' is unavailable under the current authorization; check granted scopes and current tools/list",
                    params.name
                )
            } else {
                format!(
                    "tool '{}' is not directly callable on Adaptive Runtime; use call_runtime_tool with its current canonical arguments and normal authorization",
                    params.name
                )
            };
            return Err(McpOutcome::BadRequest(rpc_error(id, -32602, message)));
        }

        Ok(Self {
            capabilities: ToolProtocolCapabilities {
                control_sidecars: stateless_2026,
                context_sidecar: stateless_2026
                    || context.surface == crate::mcp::request_policy::McpSurface::Cli,
                skill_runtime: stateless_2026,
                skill_management: stateless_2026,
                memory_surface: stateless_2026,
                trace_diagnostics: stateless_2026,
                goal_plan_app: goal_plan_app_admitted && context.exposure.goals,
                work_result_app: work_result_app_admitted,
                artifact_app: server_mcp_apps_enabled && stateless_2026,
                agent_continuation_app: agent_continuation_app_admitted
                    && (context.exposure.agent_continuation || context.exposure.job_continuation),
            },
            work_result_thread_context,
            discard_recorder: (workbench_view_call
                && (!super::super::super::workbench::CANONICAL_TOOLS
                    .contains(&params.name.as_str())
                    || (!via_adaptive_runtime_gateway
                        && !protocol_extension_admitted
                        && !is_adaptive_runtime_direct_tool(&params.name))))
                || matches!(
                    params.name.as_str(),
                    "sync_goal_plan"
                        | "get_work_result_state"
                        | "send_work_result_message"
                        | "read_changed_file_diff"
                ),
            echo_app_content: app_only_artifact_read
                || workbench_view_call
                || (app_only_work_result_state && !work_result_thread_panel)
                || app_only_work_result_activity_detail
                || app_only_work_result_send_message
                || app_only_changes_file_diff
                || app_only_agent_continuation
                || app_only_job_terminal_continuation,
            host_continuation: app_only_agent_continuation || app_only_job_terminal_continuation,
        })
    }
}

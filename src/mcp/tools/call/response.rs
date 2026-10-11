//! Post-kernel MCP projection. This stage never dispatches or reinterprets arguments.
use super::*;
use crate::tool_runtime::kernel::ToolCallOutcome;
use crate::tool_runtime::model_ergonomics_telemetry::invocation::InvocationFacts;

pub(super) struct RuntimeResponse {
    pub(super) surface: RuntimeSurface,
    pub(super) artifact_presentation: resources::ProjectArtifactPresentationMode,
    pub(super) resource_tool_call: resources::McpResourceToolCallContext,
    pub(super) result_presentation: McpToolResultPresentation,
    pub(super) invocation_facts: InvocationFacts,
    pub(super) app_call_id: Option<String>,
}

impl RuntimeResponse {
    pub(super) fn finish(
        self,
        context: &CallContext<'_>,
        diagnostics: &mut CallDiagnostics<'_>,
        tool_name: &str,
        compact_execution: bool,
        outcome: ToolCallOutcome,
    ) -> McpOutcome {
        let Self {
            surface,
            artifact_presentation,
            resource_tool_call,
            result_presentation,
            invocation_facts,
            app_call_id,
        } = self;
        let work_result_thread_panel = surface.work_result_thread_context.is_some();
        let work_result_thread_context = surface.work_result_thread_context;
        let runtime = context.runtime;
        let auth = context.auth;
        let stateless_2026 = context.stateless_2026;
        let app_enabled = context.app_enabled;
        let server_mcp_apps_enabled = context.server_mcp_apps_enabled;
        let id = context.id.clone();
        let lifecycle = &mut diagnostics.lifecycle;
        let model_ergonomics_out = &mut diagnostics.model_ergonomics_out;
        let correlation_out = &mut diagnostics.correlation_out;
        let mut model_ergonomics_completion = outcome.model_ergonomics;
        if stateless_2026 {
            if let Some(completion) = model_ergonomics_completion.as_mut() {
                completion.invocation = invocation_facts;
            }
        }
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
        if tool_name == "read_tool_manifest" {
            project_mcp_model_manifest_defaults(&mut result);
        }
        project_job_terminal_resume_suggested_call(
            app_enabled
                && surface.capabilities.agent_continuation_app
                && tool_name == "wait_for_job_terminal",
            &mut result,
        );
        project_tool_result_suggested_calls(tool_name, &mut result, &|target| {
            mcp_suggested_tool_call_route(target, stateless_2026)
        });
        // App display consumes the existing observation shape before the Host view
        // separates details. Keep only its bounded projection, not another log copy.
        let compact_app_presentation = (compact_execution && app_enabled)
            .then(|| presentation::result_app_presentation(tool_name, &result.output))
            .flatten();
        if compact_execution {
            crate::mcp::execution_control::project_result(&mut result);
        }
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
            tool_name,
            artifact_presentation,
            result,
            resource_tool_call,
            runtime.runtime_info.mcp_text_json_compat_enabled,
            result_presentation,
        ) {
            resources::McpResourceToolResultAdaptation::Framed(value) => value,
            resources::McpResourceToolResultAdaptation::Unhandled(result) => {
                // App-only tools use the standard CallToolResult channel too. Their
                // visibility/admission boundary, not custom result metadata, keeps
                // continuation protocol data out of ordinary model tool results.
                mcp_runtime_tool_result_fallback(
                    result,
                    runtime.runtime_info.mcp_text_json_compat_enabled,
                    result_presentation,
                )
            }
        };
        // Direct calls may omit discovery UI capabilities. Deliver the canonical identity
        // privately as well, matching the generic App artifact transport's Host envelopes.
        if server_mcp_apps_enabled && tool_name == "present_spreadsheet" {
            if let Some(structured) = result.get("structuredContent").cloned() {
                result["_meta"]["webcodex/spreadsheetSource"] = structured;
            }
        }
        if surface.echo_app_content {
            // ChatGPT production has been observed to complete View-originated
            // tools/call server-side while not forwarding structuredContent back to
            // the View. Keep structuredContent canonical, but duplicate this bounded
            // app-only envelope into standard text content as a compatibility path.
            // These tools are ModelHidden/app-visible only, so ordinary model tool
            // results retain the compact text fallback.
            attach_app_tool_content_fallback(&mut result);
        }
        if app_enabled && tool_name == "present_docx" {
            if let Some(structured) = result.get("structuredContent").cloned() {
                let meta = result
                    .as_object_mut()
                    .unwrap()
                    .entry("_meta")
                    .or_insert_with(|| json!({}));
                meta["webcodex/docxDocument"] = structured;
            }
        }
        if app_enabled && tool_name == "present_pdf" {
            if let Some(structured) = result.get("structuredContent").cloned() {
                result["_meta"]["webcodex/pdfDocument"] = structured;
            }
        }
        if (app_enabled && tool_name == "present_work_result") || work_result_thread_panel {
            // Initial model-originated presentation keeps normal model content compact.
            // The private MCP App result channel lets the mounted View recover the exact
            // bounded Work Result when a Host omits structuredContent from tool-result.
            attach_work_result_app_private_result(&mut result);
            if let Some(context) = work_result_thread_context {
                // The admitted native entrypoint can omit discovery's UI capability
                // on this call. Its exact authorized binding must still reach the View.
                // Only the exact explicit presentation selection becomes refresh
                // context. A Session merely linked to Window activity is not authority.
                result["_meta"][WORK_RESULT_THREAD_CONTEXT_META_KEY] = context;
            }
        }
        if surface.host_continuation {
            log_agent_continuation_app_result(
                lifecycle.as_deref(),
                tool_name,
                app_call_id.as_deref(),
                &result,
            );
        }
        if app_enabled {
            if compact_execution {
                if let Some(projection) = compact_app_presentation {
                    presentation::attach_presentation(&mut result, projection);
                }
            } else {
                presentation::attach_result_app_presentation(tool_name, &mut result);
            }
            if let (Some(expectation), Some(presentation)) = (
                outcome.correlation.failure_expectation_result.as_ref(),
                result
                    .pointer_mut("/_meta/webcodex~1presentation")
                    .and_then(Value::as_object_mut),
            ) {
                presentation.insert("failure_expectation_result".to_string(), json!(expectation));
            }
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
}

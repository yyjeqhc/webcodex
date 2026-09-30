//! Session ledger records, activity attribution, and requested context projection.

use super::*;

pub(super) fn add_run_process_expectation_projection(
    tool_name: &str,
    expectation: &sessions::ToolCallExpectation,
    result: &mut ToolResult,
) {
    if tool_name != "run_process" {
        return;
    }
    if result.output.get("expectation_satisfied").is_some() {
        return;
    }
    let Some(expectation_satisfied) = sessions::public_result_expectation_satisfied(
        result.success,
        expectation,
        &result.output,
        result.error.as_deref(),
        None,
    ) else {
        return;
    };
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    output.insert(
        "expectation_satisfied".to_string(),
        Value::Bool(expectation_satisfied),
    );
}

/// Snapshot of the activity-relevant request facts, captured before the
/// `ToolCall` is moved into execution.
pub(super) struct WorkspaceActivityContext {
    pub(super) tool: &'static str,
    pub(super) project: Option<String>,
    pub(super) client: Option<String>,
    pub(super) command: Option<String>,
    pub(super) paths: Vec<String>,
}
impl ToolRuntime {
    /// Everything the activity ledger needs from a call, captured before the
    /// call value is moved into execution. `None` for non-mutating tools.
    pub(super) fn capture_workspace_activity_context(
        call: &ToolCall,
        resolved_project: Option<&str>,
    ) -> Option<WorkspaceActivityContext> {
        let tool = call.tool_name();
        let mutating = crate::tool_runtime::tool_definition::runtime_tool_is_write_like(tool)
            || crate::tool_runtime::tool_definition::runtime_tool_is_shell_like(tool);
        if !mutating {
            return None;
        }
        let sanitized = call.session_log_arguments();
        let project = resolved_project.or_else(|| call.project());
        Some(WorkspaceActivityContext {
            tool,
            project: project.map(str::to_string),
            client: project
                .and_then(crate::tool_runtime::activity::runner_client_from_project)
                .map(str::to_string),
            command: match call {
                ToolCall::RunProcess {
                    executable, args, ..
                } => Some(crate::runner_http::process_preview(
                    executable,
                    args.iter().map(String::as_str),
                )),
                ToolCall::RunSkillResource {
                    skill_id,
                    path,
                    args,
                    ..
                } => Some(format!(
                    "trusted skill resource {skill_id}:{path} ({} args)",
                    args.len()
                )),
                ToolCall::RunDetachedProcess { args, .. } => {
                    Some(format!("detached process ({} args)", args.len()))
                }
                ToolCall::RunScript {
                    language,
                    script,
                    args,
                    ..
                } => Some(crate::runner_http::script_preview(
                    language.as_str(),
                    script.len(),
                    args.len(),
                )),
                _ => call.command_text().map(str::to_string),
            },
            paths: crate::tool_runtime::activity::paths_from_sanitized_arguments(&sanitized, 16),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn record_dispatch_session_result(
        &self,
        result: &mut ToolResult,
        session_id: &str,
        start: Option<sessions::ToolCallStart>,
        tool_name: &str,
        error_kind: Option<&str>,
        model_facing: bool,
        ack_observation: Option<&sessions::SessionAckObservation>,
        ack_requested: bool,
    ) {
        let success = result.success;
        let error = result.error.clone();
        if model_facing {
            let session_output = crate::tool_runtime::tool_audit::session_log_result_for_tool(
                tool_name,
                &result.output,
            );
            self.sessions.record_tool_call_finished(
                start,
                success,
                &session_output,
                error.as_deref(),
                error_kind,
            );
            add_session_hint(result, &self.sessions, session_id);
            if let Some(ack) = ack_observation {
                session_context::add_session_attention_projection(
                    result,
                    &self.sessions,
                    session_id,
                    "business_session",
                    ack,
                    ack_requested,
                );
            }
        } else {
            self.sessions.record_tool_call_finished(
                start,
                success,
                &result.output,
                error.as_deref(),
                error_kind,
            );
            add_session_hint(result, &self.sessions, session_id);
        }
    }
}

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn record_dispatch_activity_and_context(
        &self,
        result: &mut ToolResult,
        tool_name: &'static str,
        activity_context: Option<WorkspaceActivityContext>,
        activity_project: &Option<String>,
        session_id: &Option<String>,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        context_request: &[String],
        material_capabilities: crate::tool_runtime::context_projection::ContextMaterialCapabilities,
        context_guidance_profile: CodingGuidanceProfile,
        window: Option<&crate::client_window::ClientWindow>,
        bootstrap_context: &Option<crate::tool_runtime::coding_task::BootstrapContext>,
        context_projection_project: &Option<ResolvedProject>,
    ) {
        if let Some(context) = activity_context {
            self.activity
                .record(crate::tool_runtime::activity::ActivityRecord {
                    tool: context.tool,
                    project: context.project.as_deref(),
                    surface: transport.as_str(),
                    client: context.client.as_deref(),
                    success: result.success,
                    session_id: session_id.as_deref(),
                    command: context.command.as_deref(),
                    paths: context.paths,
                    error_summary: result.error.as_deref(),
                    // Derived from the verified caller here, not looked up later
                    // from whoever holds this client id at read time.
                    scope: crate::tool_runtime::activity::activity_scope_from_auth(auth),
                });
        }
        if result.success
            && webcodex_tool_contracts::runtime_tool_activity_interaction(tool_name).is_meaningful()
        {
            if let Ok((principal_kind, principal_id)) =
                crate::tool_runtime::session_context::runtime_observation_principal(auth)
            {
                self.observations.record_successful_tool_call(
                    crate::tool_runtime::observations::ToolCallObservation {
                        principal_kind,
                        principal_id,
                        project: activity_project.clone(),
                        surface: transport.as_str().to_string(),
                        session_id: session_id.clone(),
                        tool: tool_name.to_string(),
                        observed_at: chrono::Utc::now().timestamp(),
                    },
                );
            }
        }
        self.add_requested_context_projection_with_guidance(
            result,
            context_request,
            bootstrap_context
                .as_ref()
                .map(|context| &context.project)
                .or(context_projection_project.as_ref()),
            auth,
            material_capabilities,
            context_guidance_profile,
            window,
            bootstrap_context
                .as_ref()
                .map(|context| &context.instructions),
        )
        .await;
    }
}

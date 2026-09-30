//! Ordered Session/scope/authority guards and canonical execution provenance.

use super::*;

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn dispatch_with_auth_transport_options_and_metadata_inner(
        &self,
        mut call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        mut recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
        inner_model_facing_recording: bool,
        context_request: Vec<String>,
        material_capabilities: crate::tool_runtime::context_projection::ContextMaterialCapabilities,
        protocol_capabilities: crate::tool_runtime::kernel::ToolProtocolCapabilities,
        return_timing: crate::tool_runtime::return_timing::ToolReturnTimingPolicy,
        correlation: &mut crate::tool_runtime::window_activity::ToolCallCorrelation,
        result_projection: &mut ModelFacingProjectionPlan,
    ) -> ToolResult {
        call = call
            .with_coding_agent_recording_session_id(recorder_metadata.recording_session_id.clone());
        let effective_return_timing = return_timing.intersect(
            crate::tool_runtime::mcp_timing::return_timing_policy(transport, self.mcp_host_policy),
        );
        crate::tool_runtime::mcp_timing::normalize_observation_call_timing(
            &mut call,
            transport,
            self.mcp_host_policy,
        );
        if let ToolCall::PluginTool(plugin) = call {
            return self
                .dispatch_plugin_gateway(
                    plugin,
                    recorder_metadata.recording_session_id.as_deref(),
                    auth,
                    transport,
                )
                .await;
        }
        if let ToolCall::SshResource(ssh_resource) = call {
            return self
                .dispatch_ssh_resource_gateway(
                    ssh_resource,
                    recorder_metadata.recording_session_id.as_deref(),
                    auth,
                    transport,
                )
                .await;
        }
        // Kernel requests arrive with the same trusted logical identity already
        // used by the outer recorder. Mark only this concrete ledger path as the
        // authoritative business role; direct/internal dispatch without a kernel
        // identity remains uncorrelated legacy-style evidence.
        recorder_metadata.mark_business_execution();
        let project_resolution = match call.project() {
            Some(project) => Some(self.resolve_project_input_for_auth(project, auth).await),
            None => None,
        };
        let resolved_project = project_resolution
            .as_ref()
            .and_then(|resolution| resolution.as_ref().ok());
        result_projection.bind_resolved_project(resolved_project);
        // Preserve the canonical project for activity attribution before the
        // session recorder consumes the resolved value below. Short aliases
        // must not turn a real Runner execution into a client-less row.
        let activity_project = resolved_project
            .as_ref()
            .map(|resolved| resolved.resolved_id.clone());
        correlation.resolved_project = activity_project.clone();
        if let (Some(project), Some(trace_id)) = (
            activity_project.as_deref(),
            crate::tool_request_trace::current_active_trace_id(),
        ) {
            self.window_activity.update(&trace_id, None, Some(project));
        }
        let context_projection_project = if context_request.is_empty() {
            None
        } else {
            resolved_project.cloned()
        };
        let context_guidance_profile = self.context_guidance_profile(&call, transport);
        // work_on_project.session_id is explicit coding-resume business input,
        // never a generic tool recorder. Its implementation delegates exact
        // Session/project/lifecycle/authority handling to the coding workflow
        // engine.
        let defer_work_session = matches!(&call, ToolCall::WorkOnProject { .. });
        // session_handoff_summary.session_id remains business input for direct
        // internal dispatch. When the kernel already has an explicit outer
        // recording Session, suppress this inner recorder so worker W reading
        // coordinator C records the tool execution only in W.
        let suppress_handoff_business_recorder = !inner_model_facing_recording
            && matches!(&call, ToolCall::SessionHandoffSummary { .. });
        let session_id = if defer_work_session || suppress_handoff_business_recorder {
            None
        } else {
            call.session_id().map(str::to_string)
        };
        if let Some(session_id) = session_id.as_deref() {
            // Direct/internal dispatch may derive a recorder from explicit
            // business session_id (notably the handoff compatibility path).
            // Fence that exact Session before lifecycle/guard inheritance,
            // project mismatch logic, or any ledger mutation.
            if let Err(mut result) = self
                .authorize_session_target(session_id, call.tool_name(), auth)
                .await
            {
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "session_authority_denied",
                );
                return result;
            }
        }
        let inner_ack_requested =
            inner_model_facing_recording && !recorder_metadata.ack_session_message_ids.is_empty();
        let inner_ack_observation = if inner_model_facing_recording {
            session_id.as_deref().map(|session_id| {
                session_context::observe_session_attention_acks(
                    &self.sessions,
                    session_id,
                    &recorder_metadata.ack_session_message_ids,
                )
            })
        } else {
            None
        };
        let mut session_contract =
            crate::tool_runtime::sessions::session_tool_contract(call.tool_name());
        let session_project_mismatch = session_id.as_deref().and_then(|session_id| {
            match (
                self.sessions.session_project(session_id),
                resolved_project.as_ref(),
            ) {
                (Some(Some(session_project)), Some(resolved))
                    if session_project != resolved.resolved_id =>
                {
                    Some(SessionProjectMismatch {
                        session_project,
                        request_project: resolved.resolved_id.clone(),
                    })
                }
                _ => None,
            }
        });
        if let (Some(session_id), Some(mismatch)) =
            (session_id.as_deref(), session_project_mismatch.as_ref())
        {
            let session_start = self.sessions.record_tool_call_started_with_metadata(
                Some(session_id),
                transport,
                call.tool_name(),
                &call.session_log_arguments(),
                Some(mismatch.request_project.clone()),
                recorder_metadata.clone(),
                session_contract,
            );
            let mut result =
                session_project_mismatch_result(session_id, call.tool_name(), mismatch);
            decorate_structured_execution_prestart_denial(
                call.tool_name(),
                &mut result,
                session_context::SESSION_PROJECT_MISMATCH_KIND,
            );
            self.record_dispatch_session_result(
                &mut result,
                session_id,
                session_start,
                call.tool_name(),
                Some(session_context::SESSION_PROJECT_MISMATCH_KIND),
                inner_model_facing_recording,
                inner_ack_observation.as_ref(),
                inner_ack_requested,
            )
            .await;
            return result;
        }
        // Inherit execution defaults only after exact project matching has
        // been established. Explicit per-call cwd/shell fields remain authoritative.
        let mut ssh_resource = None;
        if session_project_mismatch.is_none() {
            if let (Some(session_id), Some(resolved)) =
                (session_id.as_deref(), resolved_project.as_ref())
            {
                if let Some(execution_context) = self
                    .sessions
                    .execution_context_for_project(session_id, &resolved.resolved_id)
                {
                    if matches!(
                        &call,
                        ToolCall::RunProcess { .. }
                            | ToolCall::RunSkillResource { .. }
                            | ToolCall::RunDetachedProcess { .. }
                            | ToolCall::RunScript { .. }
                            | ToolCall::RunShell { .. }
                            | ToolCall::RunJob { .. }
                            | ToolCall::OpenSessionShell { .. }
                            | ToolCall::CargoFmt { .. }
                            | ToolCall::CargoCheck { .. }
                            | ToolCall::CargoTest { .. }
                            | ToolCall::ProjectBuild { .. }
                            | ToolCall::ProjectValidate { .. }
                            | ToolCall::GoTest { .. }
                    ) {
                        ssh_resource = execution_context.resource.clone();
                    }
                    call = call.with_session_execution_context(&execution_context);
                }
            }
        }
        // Recovery is admitted only after the existing helper proves an exact
        // canonical shell call. Re-enter all later guards with RunShell as the
        // authoritative tool identity; RunProcess never dispatches shell text.
        let mut shell_normalization = None;
        if let Some(recovery) = self
            .process_shell_recovery_call(
                &call,
                &recorder_metadata.expectation,
                ssh_resource.as_deref(),
                resolved_project,
            )
            .await
        {
            let arguments = recovery["arguments"].clone();
            let login = arguments["login"] == true;
            if let Err(error) =
                crate::tool_runtime::kernel::check_runtime_tool_scope(auth, "run_shell")
            {
                let detail = match error {
                    crate::tool_runtime::kernel::ToolCallErrorStatus::InsufficientScope {
                        description,
                        ..
                    } => description,
                    crate::tool_runtime::kernel::ToolCallErrorStatus::InvalidArguments {
                        message,
                    } => message,
                };
                return ToolResult::err_with_output(
                    detail,
                    serde_json::json!({
                        "failure_kind": "insufficient_scope", "execution_state": "not_started",
                        "command_started": false, "requested_surface": "run_process",
                        "execution_source": "run_shell"
                    }),
                );
            }
            call = ToolCall::from_tool_name("run_shell", arguments)
                .expect("recovery helper validated canonical run_shell");
            // The proven normalization changes the authoritative execution form.
            // Use shell projection so runtime-selected cwd/shell stay visible.
            *result_projection = ModelFacingProjectionPlan::capture(&call);
            if let Some(session_id) = session_id.as_deref() {
                if let Err(mut denial) = self
                    .authorize_session_target(session_id, "run_shell", auth)
                    .await
                {
                    decorate_structured_execution_prestart_denial(
                        "run_shell",
                        &mut denial,
                        "session_authority_denied",
                    );
                    return denial;
                }
            }
            session_contract = crate::tool_runtime::sessions::session_tool_contract("run_shell");
            shell_normalization = Some(if login {
                webcodex_tool_contracts::ToolInputNormalizationCode::RunProcessBashLcToLoginRunShell
            } else {
                match &call {
                    ToolCall::RunShell {
                        shell: Some(shell), ..
                    } if shell.as_str() == "sh" => webcodex_tool_contracts::ToolInputNormalizationCode::RunProcessShCToRunShell,
                    _ => webcodex_tool_contracts::ToolInputNormalizationCode::RunProcessBashCToRunShell,
                }
            });
        }
        // Apply return-latency policy only after Session execution context and
        // exact shell recovery have resolved whether this call can use the
        // Runner-owned durable handoff path. Named SSH run_shell is intentionally
        // direct-only: an omitted legacy sync_wait_secs must stay omitted, while
        // an explicitly supplied legacy value is still rejected by the SSH
        // execution contract below.
        if ssh_resource.is_none() {
            crate::tool_runtime::return_timing::normalize_structured_handoff(
                &mut call,
                effective_return_timing,
            );
        }
        if let Some(session_id) = session_id.as_deref() {
            // Lifecycle denial is orthogonal to mode/guards and wins first.
            if let Some(denial) =
                self.sessions
                    .lifecycle_denial(session_id, call.tool_name(), session_contract)
            {
                let session_start = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
                    None,
                    recorder_metadata.clone(),
                    session_contract,
                );
                let mut result =
                    session_lifecycle_denied_result(session_id, call.tool_name(), denial);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "session_lifecycle_denied",
                );
                let error_kind = result
                    .output
                    .get("error_kind")
                    .and_then(Value::as_str)
                    .unwrap_or("session_closed")
                    .to_string();
                self.record_dispatch_session_result(
                    &mut result,
                    session_id,
                    session_start,
                    call.tool_name(),
                    Some(error_kind.as_str()),
                    inner_model_facing_recording,
                    inner_ack_observation.as_ref(),
                    inner_ack_requested,
                )
                .await;
                return result;
            }
            if let Some(denial) = self.sessions.guard_denial(session_id, session_contract) {
                let session_start = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
                    None,
                    recorder_metadata.clone(),
                    session_contract,
                );
                let mut result = session_guard_denied_result(session_id, call.tool_name(), denial);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "session_guard_denied",
                );
                self.record_dispatch_session_result(
                    &mut result,
                    session_id,
                    session_start,
                    call.tool_name(),
                    Some("session_guard_denied"),
                    inner_model_facing_recording,
                    inner_ack_observation.as_ref(),
                    inner_ack_requested,
                )
                .await;
                return result;
            }
        }
        let mut session_start = if session_id.is_some() {
            let resolved_project = resolved_project.map(|resolved| resolved.resolved_id.clone());
            self.sessions.record_tool_call_started_with_metadata(
                session_id.as_deref(),
                transport,
                call.tool_name(),
                &call.session_log_arguments(),
                resolved_project,
                recorder_metadata.clone(),
                session_contract,
            )
        } else {
            None
        };
        if let Err(err) = self
            .authorize_runner_tool(
                &call,
                ssh_resource.as_deref(),
                auth,
                project_resolution.as_ref(),
            )
            .await
        {
            let mut err = err;
            let failure_kind = crate::tool_runtime::process::classify_process_failure(
                err.error.as_deref().unwrap_or_default(),
            );
            decorate_structured_execution_prestart_denial(call.tool_name(), &mut err, failure_kind);
            if let Some(session_id) = session_id.as_deref() {
                self.record_dispatch_session_result(
                    &mut err,
                    session_id,
                    session_start,
                    call.tool_name(),
                    None,
                    inner_model_facing_recording,
                    inner_ack_observation.as_ref(),
                    inner_ack_requested,
                )
                .await;
            }
            return err;
        }
        // Authoritative single evaluation (kernel must not re-evaluate).
        // Order: session/auth guards above → permission gate → mutation below.
        // Path/sensitive hard checks still run inside tools; hard-deny filter
        // suppresses permission attach so soft policy never overrides them.
        let permission = crate::tool_runtime::permissions::evaluate_permission_for_tool(
            &self.permission_evaluator,
            call.tool_name(),
            activity_project.as_deref().or_else(|| call.project()),
        );
        if let Some(decision) = permission.as_ref() {
            if !decision.allows_execution() {
                let mut result = permissions::permission_execution_denied_result(decision);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "permission_denied",
                );
                if let Some(start) = session_start.as_mut() {
                    self.sessions
                        .record_permission_decision(start, decision.clone());
                }
                permissions::add_permission_to_result(&mut result, decision);
                if let Some(session_id) = session_id.as_deref() {
                    self.record_dispatch_session_result(
                        &mut result,
                        session_id,
                        session_start,
                        call.tool_name(),
                        None,
                        inner_model_facing_recording,
                        inner_ack_observation.as_ref(),
                        inner_ack_requested,
                    )
                    .await;
                }
                return result;
            }
        }
        let activity_context =
            Self::capture_workspace_activity_context(&call, activity_project.as_deref());
        let validation_assertion_name = recorder_metadata.expectation.assertion_name.as_deref();
        let logical_invocation_id = recorder_metadata.logical_invocation_id.as_deref();
        let tool_name = call.tool_name();
        let trusted_recording_session_id = recorder_metadata
            .recording_session_authorized
            .then_some(recorder_metadata.recording_session_id.as_deref())
            .flatten();
        let trusted_recording_session_project = recorder_metadata
            .recording_session_authorized
            .then_some(recorder_metadata.recording_session_project.as_deref())
            .flatten();
        let source_mutation =
            if crate::tool_runtime::validation_source::observes_potential_mutation(&call) {
                activity_project
                    .as_deref()
                    .and_then(|project| self.validation_sources.begin(project))
            } else {
                None
            };
        // Resolve model-facing Project selectors exactly once under the current
        // authenticated principal, then bind only legacy inner adapters that still
        // perform auth-less ProjectConfig lookups to the authorized canonical id.
        // Do not rewrite every Project-bearing ToolCall: work_on_project preserves
        // the caller selector in its output, Work Result deliberately requires an
        // exact canonical input, and newer adapters consume the retained
        // ResolvedProject or re-resolve with the current AuthContext themselves.
        let mut requested_project_output = None;
        if let Some(resolved) = project_resolution
            .as_ref()
            .and_then(|resolution| resolution.as_ref().ok())
        {
            if let Some((project, output_mode)) = canonical_execution_project_binding(&mut call) {
                if output_mode == CanonicalProjectOutput::Requested {
                    requested_project_output = Some(project.clone());
                }
                project.clone_from(&resolved.resolved_id);
            }
        }
        let read_scope = crate::tool_runtime::read_cache::ReadScope::new(
            auth,
            session_id.as_deref().or(trusted_recording_session_id),
        );
        let mut bootstrap_context = None;
        let mut result = crate::tool_runtime::read_cache::READ_SCOPE
            .scope(
                read_scope,
                self.dispatch_authorized_inner(
                    call,
                    auth,
                    transport,
                    window,
                    ssh_resource.as_deref(),
                    validation_assertion_name,
                    project_resolution,
                    trusted_recording_session_id,
                    trusted_recording_session_project,
                    logical_invocation_id,
                    effective_return_timing.max_handoff_secs(),
                    protocol_capabilities,
                    correlation,
                    &mut bootstrap_context,
                ),
            )
            .await;
        if let Some(requested_project) = requested_project_output {
            if result.output.get("project").is_some() {
                result.output["project"] = serde_json::Value::String(requested_project);
            }
        }
        if let Some(observation) = source_mutation {
            observation.finish(&result);
        }
        if let Some(code) = shell_normalization {
            result.output["requested_surface"] = serde_json::json!("run_process");
            result.output["execution_source"] = serde_json::json!("run_shell");
            if result.success {
                result.output["input_normalization"] =
                    serde_json::json!({"code": code, "hint": code.model_hint()});
            }
        }
        let permission = permission.filter(|_| {
            !permissions::is_hard_denied_output(&result.output, result.error.as_deref())
        });
        if let Some(permission) = permission.as_ref() {
            if let Some(start) = session_start.as_mut() {
                self.sessions
                    .record_permission_decision(start, permission.clone());
            }
            permissions::add_permission_to_result(&mut result, permission);
        }
        if let Some(session_id) = session_id.as_deref() {
            self.record_dispatch_session_result(
                &mut result,
                session_id,
                session_start,
                tool_name,
                None,
                inner_model_facing_recording,
                inner_ack_observation.as_ref(),
                inner_ack_requested,
            )
            .await;
        }
        add_run_process_expectation_projection(
            tool_name,
            &recorder_metadata.expectation,
            &mut result,
        );
        self.record_dispatch_activity_and_context(
            &mut result,
            tool_name,
            activity_context,
            &activity_project,
            &session_id,
            auth,
            transport,
            &context_request,
            material_capabilities,
            context_guidance_profile,
            window,
            &bootstrap_context,
            &context_projection_project,
        )
        .await;
        result
    }
}

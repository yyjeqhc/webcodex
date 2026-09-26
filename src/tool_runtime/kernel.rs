use super::model_ergonomics_telemetry::{ModelErgonomicsCompletion, ModelErgonomicsTimer};
use super::sessions::{
    strip_tool_call_expectation_metadata, SessionTransport, ToolCallRecorderMetadata,
    ToolCallSessionMessageResolution,
};
use super::tool_audit::{
    session_log_arguments_for_tool_request, session_log_arguments_for_typed_call,
    session_log_result_for_tool,
};
use super::tool_definition::{runtime_tool_operator_extension_family, ToolOperatorExtensionFamily};
use super::{session_context, HostFileImportProvenance, ToolCall, ToolResult, ToolRuntime};
use crate::auth::scopes::OAuthToolScopePolicy;
use crate::auth::AuthContext;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolTransport {
    Api,
    Mcp,
}

impl From<ToolTransport> for SessionTransport {
    fn from(value: ToolTransport) -> Self {
        match value {
            ToolTransport::Api => SessionTransport::Api,
            ToolTransport::Mcp => SessionTransport::Mcp,
        }
    }
}

pub(crate) use super::HostFileImportProvenance as HostFileImportTrust;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolCallContext<'a> {
    pub(crate) transport: ToolTransport,
    pub(crate) session_id: Option<&'a str>,
    pub(crate) auth: Option<&'a AuthContext>,
    pub(crate) window: Option<&'a crate::client_window::ClientWindow>,
    /// REST records scope denials with session metadata. MCP rejects scope
    /// denials before `_session_id` becomes recorder metadata. Keep both
    /// adapter-visible behaviors stable.
    pub(crate) record_oauth_scope_denials: bool,
    /// Server-derived provenance for ChatGPT host file references. Raw tool
    /// arguments cannot set this value.
    pub(crate) host_file_import_trust: HostFileImportProvenance,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolCallRequest {
    pub(crate) tool_name: String,
    pub(crate) arguments: Value,
}

/// Protocol/session metadata already parsed and provenance-bound by the adapter.
/// None of these fields are concrete tool business arguments or execution
/// authority. Trusted protocol capabilities remain a separate adapter-derived
/// input and the kernel continues to own all authority checks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ToolInvocationMetadata {
    pub(crate) control: Option<super::control_sidecar::ControlSidecars>,
    // Historical wrapper name retained for protocol compatibility. Exact IDs may
    // acknowledge either legacy Session-board messages or Window collaboration
    // messages; each collaboration store still performs its own principal/Window checks.
    pub(crate) ack_session_message_ids: Vec<String>,
    pub(crate) ack_ref: Option<String>,
    pub(crate) session_message_resolution: Option<ToolCallSessionMessageResolution>,
    pub(crate) window_reply: Option<super::window_collaboration::ToolCallWindowReply>,
    pub(crate) context_request: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ToolProtocolCapabilities {
    /// Explicit model-facing control wrapper support; internal/App calls default off.
    pub(crate) control_sidecars: bool,
    pub(crate) context_sidecar: bool,
    pub(crate) skill_runtime: bool,
    pub(crate) skill_management: bool,
    /// Protocol-surface support for the Control-owned Memory runtime. Per-tool
    /// read, manage, and administrator authority comes only from canonical
    /// ToolDefinition metadata.
    pub(crate) memory_surface: bool,
    /// Protocol-surface support for privileged forensic trace retrieval. Caller
    /// authority is still derived only from the canonical ToolDefinition.
    pub(crate) trace_diagnostics: bool,
    /// Protocol-surface support for the ModelHidden Goal Plan App polling read.
    /// This never replaces canonical communication/Goal authorization.
    pub(crate) goal_plan_app: bool,
    /// Protocol-surface support for Work Result App live refresh and frozen
    /// lazy diff reads. Exact Project and optional Session context are checked per call;
    /// lazy reads additionally fence caller, snapshot, and advertised path.
    pub(crate) work_result_app: bool,
    /// Protocol-surface support for ModelHidden MCP App Host-continuation
    /// coordination. Canonical communication authorization and exact
    /// process-local Host binding validation remain mandatory in the runtime.
    pub(crate) agent_continuation_app: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolCallErrorStatus {
    InvalidArguments {
        message: String,
    },
    InsufficientScope {
        required_scope: Option<&'static str>,
        description: String,
    },
}

#[derive(Debug)]
pub(crate) struct ToolCallOutcome {
    pub(crate) success: bool,
    pub(crate) result: Option<ToolResult>,
    pub(crate) error_status: Option<ToolCallErrorStatus>,
    pub(crate) project: Option<String>,
    pub(crate) model_ergonomics: Option<ModelErgonomicsCompletion>,
    /// Trusted internal Window/Workflow Session correlation evidence. This is
    /// adapter metadata only and is never part of the public ToolResult.
    pub(crate) correlation: super::window_activity::ToolCallCorrelation,
}

pub(crate) fn check_runtime_tool_scope(
    auth: Option<&AuthContext>,
    tool_name: &str,
) -> Result<(), ToolCallErrorStatus> {
    let policy = crate::auth::scopes::oauth_scope_policy_for_runtime_tool(tool_name);
    let Some(auth) = auth else {
        // Preserve historical unauthenticated compatibility for unrelated
        // internal tools, but explicit Memory and administrator authority is
        // intentionally never inferred from surface presence or a missing
        // credential. This derives only from the canonical ToolDefinition
        // authority policy; it is not a tool-name registry.
        let required_explicit_scope = match policy {
            OAuthToolScopePolicy::RequireAny(scopes) => {
                // RequireAny used to be exclusive to explicit-only Plugin
                // authority. Computer consolidation also needs an OR policy for
                // control-vs-launch discovery without changing the legacy
                // unauthenticated compatibility of the underlying operations.
                if scopes
                    .iter()
                    .copied()
                    .all(crate::auth::scopes::scope_requires_explicit_unauthenticated_authority)
                {
                    return Err(ToolCallErrorStatus::InsufficientScope {
                        required_scope: None,
                        description: format!("missing any required scope: {}", scopes.join(", ")),
                    });
                }
                None
            }
            OAuthToolScopePolicy::Require(scope)
                if matches!(
                    scope,
                    crate::auth::SCOPE_MEMORY_READ
                        | crate::auth::SCOPE_MEMORY_MANAGE
                        | crate::auth::SCOPE_ADMIN
                ) =>
            {
                Some(scope)
            }
            OAuthToolScopePolicy::RequireAll(scopes) => scopes.iter().copied().find(|scope| {
                matches!(
                    *scope,
                    crate::auth::SCOPE_MEMORY_READ
                        | crate::auth::SCOPE_MEMORY_MANAGE
                        | crate::auth::SCOPE_ADMIN
                )
            }),
            _ => None,
        };
        if let Some(scope) = required_explicit_scope {
            return Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(scope),
                description: format!("missing required scope: {scope}"),
            });
        }
        return Ok(());
    };

    match policy {
        OAuthToolScopePolicy::RequireAny(scopes) => {
            if scopes.iter().copied().any(|scope| auth.has_scope(scope)) {
                Ok(())
            } else {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: None,
                    description: format!("missing any required scope: {}", scopes.join(", ")),
                })
            }
        }
        OAuthToolScopePolicy::Require(scope) => {
            if auth.has_scope(scope) {
                Ok(())
            } else {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: Some(scope),
                    description: format!("missing required scope: {}", scope),
                })
            }
        }
        OAuthToolScopePolicy::RequireAll(scopes) => {
            if let Some(scope) = scopes.iter().copied().find(|scope| !auth.has_scope(scope)) {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: Some(scope),
                    description: format!("missing required scope: {}", scope),
                })
            } else {
                Ok(())
            }
        }
        OAuthToolScopePolicy::Unknown => {
            if auth.is_bootstrap() {
                Ok(())
            } else {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: None,
                    description: "runtime tool has no declared scope policy".to_string(),
                })
            }
        }
    }
}

fn check_session_message_resolution_scope(
    auth: Option<&AuthContext>,
    requested: bool,
) -> Result<(), ToolCallErrorStatus> {
    if requested {
        // Piggyback resolution is the same business mutation as the dedicated
        // Session tool. Reuse its canonical scope policy so a caller cannot
        // acquire Session-closure authority from an unrelated main tool scope.
        check_runtime_tool_scope(auth, "resolve_session_message")
    } else {
        Ok(())
    }
}

impl ToolRuntime {
    #[cfg(feature = "experimental-code-mode")]
    pub(crate) fn call_tool_with_context_and_return_timing<'a>(
        &'a self,
        request: ToolCallRequest,
        context: ToolCallContext<'a>,
        return_timing: super::return_timing::ToolReturnTimingPolicy,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolCallOutcome> + Send + 'a>> {
        Box::pin(async move {
            self.call_tool_with_invocation_metadata_and_return_timing(
                request,
                context,
                ToolInvocationMetadata::default(),
                ToolProtocolCapabilities::default(),
                return_timing,
            )
            .await
        })
    }

    pub(crate) fn call_tool_with_context<'a>(
        &'a self,
        request: ToolCallRequest,
        context: ToolCallContext<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolCallOutcome> + Send + 'a>> {
        // Keep the large canonical kernel/dispatch future off adapter thread
        // stacks. Workspace-wide dependency feature unification can enlarge
        // serde-backed state enough for ordinary REST/Host calls to overflow the
        // default libtest/worker stack even though the selected tool is bounded.
        Box::pin(async move {
            self.call_tool_with_protocol_capabilities(
                request,
                context,
                ToolProtocolCapabilities::default(),
            )
            .await
        })
    }

    /// Test-only compatibility shim for Phase-2/3 fixtures that predate the
    /// explicit capability bundle. Production adapters with invocation wrapper
    /// metadata use `call_tool_with_invocation_metadata`; callers without such
    /// metadata may use `call_tool_with_protocol_capabilities`. Management is
    /// intentionally never enabled here.
    #[cfg(test)]
    pub(crate) async fn call_tool_with_context_protocol_capability(
        &self,
        request: ToolCallRequest,
        context: ToolCallContext<'_>,
        context_sidecar_capable: bool,
    ) -> ToolCallOutcome {
        self.call_tool_with_protocol_capabilities(
            request,
            context,
            ToolProtocolCapabilities {
                context_sidecar: context_sidecar_capable,
                control_sidecars: false,
                skill_runtime: context_sidecar_capable,
                skill_management: false,
                memory_surface: false,
                trace_diagnostics: false,
                goal_plan_app: false,
                work_result_app: false,
                agent_continuation_app: false,
            },
        )
        .await
    }

    pub(crate) async fn call_tool_with_protocol_capabilities(
        &self,
        request: ToolCallRequest,
        context: ToolCallContext<'_>,
        capabilities: ToolProtocolCapabilities,
    ) -> ToolCallOutcome {
        self.call_tool_with_invocation_metadata(
            request,
            context,
            ToolInvocationMetadata::default(),
            capabilities,
        )
        .await
    }

    pub(crate) fn call_tool_with_invocation_metadata<'a>(
        &'a self,
        request: ToolCallRequest,
        context: ToolCallContext<'a>,
        invocation_metadata: ToolInvocationMetadata,
        capabilities: ToolProtocolCapabilities,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolCallOutcome> + Send + 'a>> {
        self.call_tool_with_invocation_metadata_and_return_timing(
            request,
            context,
            invocation_metadata,
            capabilities,
            super::return_timing::ToolReturnTimingPolicy::unconstrained(),
        )
    }

    fn call_tool_with_invocation_metadata_and_return_timing<'a>(
        &'a self,
        request: ToolCallRequest,
        context: ToolCallContext<'a>,
        mut invocation_metadata: ToolInvocationMetadata,
        capabilities: ToolProtocolCapabilities,
        return_timing: super::return_timing::ToolReturnTimingPolicy,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolCallOutcome> + Send + 'a>> {
        // MCP enters the kernel here directly rather than through
        // call_tool_with_context, so give it the same bounded adapter future.
        Box::pin(async move {
            let mut telemetry =
                ModelErgonomicsTimer::start_with_arguments(&request.tool_name, &request.arguments);
            if let Some(telemetry) = telemetry.as_mut() {
                telemetry.resolve_work_on_project_guidance_profile(
                    self.mcp_host_policy,
                    matches!(context.transport, ToolTransport::Mcp),
                );
            }
            let tool_name = request.tool_name.clone();
            let mut control = invocation_metadata
                .control
                .take()
                .map(super::control_sidecar::ControlExecution::new);
            let mut outcome = self
                .call_tool_with_context_inner(
                    request,
                    context,
                    invocation_metadata,
                    capabilities,
                    return_timing,
                    &mut control,
                )
                .await;
            if let Some(control) = control {
                control.decorate(&mut outcome);
            }
            outcome.model_ergonomics = telemetry.map(ModelErgonomicsTimer::finish);
            if let (Some(completion), Some(result)) =
                (&mut outcome.model_ergonomics, &outcome.result)
            {
                completion.job_convergence = self.job_convergence_record(
                    &tool_name,
                    result,
                    &outcome.correlation,
                    context.auth,
                    context.window,
                );
            }
            outcome
        })
    }

    async fn call_tool_with_context_inner(
        &self,
        request: ToolCallRequest,
        context: ToolCallContext<'_>,
        invocation_metadata: ToolInvocationMetadata,
        capabilities: ToolProtocolCapabilities,
        return_timing: super::return_timing::ToolReturnTimingPolicy,
        control: &mut Option<super::control_sidecar::ControlExecution>,
    ) -> ToolCallOutcome {
        if let Some(control) = control.as_ref() {
            if let Err(outcome) = control.validate(
                &request.tool_name,
                context,
                capabilities,
                invocation_metadata.session_message_resolution.is_some(),
            ) {
                return outcome;
            }
        }
        let ack_ref = invocation_metadata.ack_ref.clone();
        let window_reply = invocation_metadata.window_reply.clone();
        let mut recorder_metadata =
            ToolCallRecorderMetadata::from_business_arguments(&request.arguments);
        recorder_metadata.ack_session_message_ids = invocation_metadata.ack_session_message_ids;
        recorder_metadata.session_message_resolution =
            invocation_metadata.session_message_resolution;
        // One trusted identity per real kernel request. The outer recorder and
        // inner business ledger pairs inherit it, but it never affects execution.
        recorder_metadata.assign_logical_invocation();
        let operator_extension_family = runtime_tool_operator_extension_family(&request.tool_name);
        if matches!(
            operator_extension_family,
            Some(ToolOperatorExtensionFamily::TraceDiagnostics)
        ) && !capabilities.trace_diagnostics
        {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "Tool trace diagnostics are available only on Stateless MCP 2026"
                        .to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        if request.tool_name == "goal_plan_sync" && !capabilities.goal_plan_app {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "Goal Plan App state is available only on Stateless MCP 2026 requests with Goal Plan App capability"
                        .to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        if matches!(
            request.tool_name.as_str(),
            "work_result_state" | "work_result_send_message"
        ) && !capabilities.work_result_app
        {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "Work Result App operations are available only on Stateless MCP 2026 requests with Work Result App capability"
                        .to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        if request.tool_name == "changes_file_diff" && !capabilities.work_result_app {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "Work Result App lazy diff is available only on Stateless MCP 2026 requests with Work Result App capability"
                        .to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        if matches!(
            request.tool_name.as_str(),
            "agent_continuation_bind"
                | "agent_continuation_recover_endpoint"
                | "agent_continuation_state"
                | "agent_continuation_wake_acquire"
                | "agent_continuation_wake_prepare"
                | "agent_continuation_wake_finish"
                | "agent_continuation_unbind"
                | "agent_wait_state"
        ) && !capabilities.agent_continuation_app
        {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "Agent continuation App coordination is available only on Stateless MCP 2026 requests with Agent Continuation App capability"
                        .to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        if matches!(
            request.tool_name.as_str(),
            "job_terminal_continuation_bind"
                | "job_terminal_continuation_state"
                | "job_terminal_continuation_prepare"
                | "job_terminal_continuation_finish"
                | "job_terminal_continuation_unbind"
        ) && !capabilities.agent_continuation_app
        {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "Job terminal continuation App coordination is available only on Stateless MCP 2026 requests with MCP App continuation capability"
                        .to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        // Project Memory tools are kernel-known but globally model-hidden. One
        // explicit protocol capability gates all six fixed tools; their
        // canonical ToolDefinition authority decides caller access below.
        if matches!(
            operator_extension_family,
            Some(
                ToolOperatorExtensionFamily::MemoryRuntime
                    | ToolOperatorExtensionFamily::MemoryManagement
            )
        ) && !capabilities.memory_surface
        {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "Memory tools are available only on Stateless MCP 2026".to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        // Phase-3 Skill tools are kernel-known only so ToolCall parsing stays
        // typed, but execution is gated by explicit protocol capability. A private
        // tool name from REST or legacy MCP cannot enable this runtime.
        if matches!(
            operator_extension_family,
            Some(ToolOperatorExtensionFamily::SkillRuntime)
        ) && !capabilities.skill_runtime
        {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "Skill runtime tools are available only on Stateless MCP 2026"
                        .to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        if matches!(
            operator_extension_family,
            Some(ToolOperatorExtensionFamily::SkillManagement)
        ) && !capabilities.skill_management
        {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "Skill management tools are available only on Stateless MCP 2026"
                        .to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        if matches!(
            operator_extension_family,
            Some(ToolOperatorExtensionFamily::SkillManagement)
        ) && !context
            .auth
            .is_some_and(|auth| auth.has_scope(crate::auth::SCOPE_ADMIN))
        {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InsufficientScope {
                    required_scope: Some(crate::auth::SCOPE_ADMIN),
                    description: "missing required scope: admin".to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        let canonical_recording_session_id = match context.session_id {
            Some(raw) => match self.canonicalize_explicit_session_selector(raw, context.auth) {
                Ok(canonical) => Some(canonical),
                Err(message) => {
                    return ToolCallOutcome {
                        success: false,
                        result: None,
                        error_status: Some(ToolCallErrorStatus::InvalidArguments { message }),
                        project: None,
                        model_ergonomics: None,
                        correlation: Default::default(),
                    };
                }
            },
            None => None,
        };
        let context = ToolCallContext {
            session_id: canonical_recording_session_id.as_deref(),
            ..context
        };
        // Action-dependent gateways resolve exact policy before the generic
        // static Session/permission lifecycle and own one specialized ledger.
        if let Some(mut outcome) =
            super::specialized::try_dispatch_specialized_gateway(self, &request, context).await
        {
            if super::tool_definition::is_model_visible_tool_name(&request.tool_name) {
                let peer_project = outcome.project.clone();
                if let Some(result) = outcome.result.as_mut() {
                    self.add_window_model_reply_sidecar(
                        result,
                        context.auth,
                        context.window,
                        window_reply.as_ref(),
                    );
                    if request.tool_name != "present_work_result" {
                        self.add_window_operator_projection(
                            result,
                            context.auth,
                            context.window,
                            &recorder_metadata.ack_session_message_ids,
                        );
                    }
                    self.add_peer_collaboration_projection(
                        result,
                        context.auth,
                        context.window,
                        peer_project.as_deref(),
                        &recorder_metadata.ack_session_message_ids,
                    );
                }
            }
            return outcome;
        }
        let mut concrete_arguments =
            strip_tool_call_expectation_metadata(request.arguments.clone());
        let context_request = if capabilities.context_sidecar {
            invocation_metadata.context_request
        } else {
            Vec::new()
        };
        // A wrapper recording_session_id is authority-bearing internal context,
        // not a ledger address. Authorize it before any lifecycle/guard lookup,
        // project mismatch computation, provenance derivation, or ledger write.
        if let Some(recorder_session_id) = context.session_id {
            if let Err(mut result) = self
                .authorize_session_target(recorder_session_id, &request.tool_name, context.auth)
                .await
            {
                super::dispatch::decorate_structured_execution_prestart_denial(
                    &request.tool_name,
                    &mut result,
                    "session_authority_denied",
                );
                return ToolCallOutcome {
                    success: false,
                    result: Some(result),
                    error_status: None,
                    project: None,
                    model_ergonomics: None,
                    correlation: Default::default(),
                };
            }
        }
        let recorder_ack_requested =
            !recorder_metadata.ack_session_message_ids.is_empty() || ack_ref.is_some();
        let explicit_session_ack_ids = context.session_id.map(|recorder_session_id| {
            session_context::session_ack_message_ids(
                &self.sessions,
                recorder_session_id,
                &recorder_metadata.ack_session_message_ids,
                ack_ref.as_deref(),
            )
        });
        if let Some(recorder_session_id) = context.session_id {
            recorder_metadata.recording_session_id = Some(recorder_session_id.to_string());
            recorder_metadata.recording_session_project = self
                .sessions
                .session_project(recorder_session_id)
                .expect("authorized recording Session must exist");
            recorder_metadata.recording_session_authorized = true;
        }
        let session_message_resolution = (context.transport == ToolTransport::Mcp)
            .then(|| recorder_metadata.session_message_resolution.clone())
            .flatten();
        if session_message_resolution.is_some() && context.session_id.is_none() {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments {
                    message: "session_message_resolution requires recording_session_id".to_string(),
                }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        if let Err(error_status) = check_session_message_resolution_scope(
            context.auth,
            session_message_resolution.is_some(),
        ) {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(error_status),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        if let Err(message) =
            self.canonicalize_session_reference_argument(&mut concrete_arguments, context.auth)
        {
            return ToolCallOutcome {
                success: false,
                result: None,
                error_status: Some(ToolCallErrorStatus::InvalidArguments { message }),
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }

        let outer_ack_observation = context.session_id.map(|recorder_session_id| {
            session_context::observe_session_attention_acks(
                &self.sessions,
                recorder_session_id,
                explicit_session_ack_ids
                    .as_deref()
                    .unwrap_or(&recorder_metadata.ack_session_message_ids),
            )
        });
        if collaboration_session_tool(&request.tool_name) {
            if let (Some(recorder_session_id), Some(target_session_id)) = (
                context.session_id,
                collaboration_target_session_id(&request.tool_name, &concrete_arguments),
            ) {
                // Both ends of a cross-Session collaboration relationship must
                // be independently authorized before comparing scope. This makes
                // None/None safe while rejecting either mixed scoped/unscoped
                // direction without any cross-project escape.
                if let Err(result) = self
                    .authorize_session_target(target_session_id, &request.tool_name, context.auth)
                    .await
                {
                    return ToolCallOutcome {
                        success: false,
                        result: Some(result),
                        error_status: None,
                        project: None,
                        model_ergonomics: None,
                        correlation: Default::default(),
                    };
                }
                let recorder_project = self
                    .sessions
                    .session_project(recorder_session_id)
                    .expect("authorized recording Session must exist");
                let target_project = self
                    .sessions
                    .session_project(target_session_id)
                    .expect("authorized collaboration target Session must exist");
                if recorder_project != target_project {
                    let result = session_context::session_project_mismatch_result(
                        target_session_id,
                        &request.tool_name,
                        &session_context::SessionProjectMismatch {
                            session_project: target_project
                                .unwrap_or_else(|| "<unscoped>".to_string()),
                            request_project: recorder_project
                                .unwrap_or_else(|| "<unscoped>".to_string()),
                        },
                    );
                    return ToolCallOutcome {
                        success: false,
                        result: Some(result),
                        error_status: None,
                        project: None,
                        model_ergonomics: None,
                        correlation: Default::default(),
                    };
                }
            }
        }
        let session_contract = super::sessions::session_tool_contract(&request.tool_name);
        let recording_session_project_mismatch = match context.session_id {
            Some(session_id) => {
                self.recording_session_project_mismatch(
                    session_id,
                    &request.tool_name,
                    &concrete_arguments,
                    context.auth,
                )
                .await
            }
            None => None,
        };
        if let (Some(session_id), Some(mismatch)) = (
            context.session_id,
            recording_session_project_mismatch.as_ref(),
        ) {
            let session_event = self.sessions.record_tool_call_started_with_metadata(
                Some(session_id),
                context.transport.into(),
                &request.tool_name,
                &session_log_arguments_for_tool_request(&request.tool_name, &concrete_arguments),
                Some(mismatch.request_project.clone()),
                recorder_metadata.clone(),
                session_contract,
            );
            let mut result = session_context::session_project_mismatch_result(
                session_id,
                &request.tool_name,
                mismatch,
            );
            super::dispatch::decorate_structured_execution_prestart_denial(
                &request.tool_name,
                &mut result,
                session_context::SESSION_PROJECT_MISMATCH_KIND,
            );
            self.sessions.record_tool_call_finished(
                session_event,
                false,
                &result.output,
                result.error.as_deref(),
                Some(session_context::SESSION_PROJECT_MISMATCH_KIND),
            );
            super::add_session_hint(&mut result, &self.sessions, session_id);
            session_context::add_session_attention_projection(
                &mut result,
                &self.sessions,
                session_id,
                "recording_session",
                outer_ack_observation
                    .as_ref()
                    .expect("authorized outer recorder must have ACK observation"),
                recorder_ack_requested,
            );
            return ToolCallOutcome {
                success: false,
                result: Some(result),
                error_status: None,
                project: None,
                model_ergonomics: None,
                correlation: Default::default(),
            };
        }
        // The outer recording Session is provenance/context only. Its lifecycle,
        // mode, and guards never become business execution policy. In particular,
        // closed recorders remain valid evidence sinks (the Session store supports
        // append-only recorder events on cold closed records). Concrete business
        // Session lifecycle/guards are enforced later from ToolCall::session_id().

        if !context.record_oauth_scope_denials {
            if let Err(error_status) = check_runtime_tool_scope(context.auth, &request.tool_name) {
                return ToolCallOutcome {
                    success: false,
                    result: None,
                    error_status: Some(error_status),
                    project: None,
                    model_ergonomics: None,
                    correlation: Default::default(),
                };
            }
        }

        // Parse the business request once. Successful typed input drives both
        // pre-execution audit projection and later dispatch. Malformed input is
        // recorded with an empty request projection rather than reparsed through
        // a schema-filter fallback.
        let parsed_call =
            ToolCall::from_tool_name_with_normalization(&request.tool_name, concrete_arguments);
        let session_log_arguments = parsed_call
            .as_ref()
            .map(|(call, _)| session_log_arguments_for_typed_call(&request.tool_name, call))
            .unwrap_or_else(|_| Value::Object(Default::default()));
        let mut session_event = self.sessions.record_tool_call_started_with_metadata(
            context.session_id,
            context.transport.into(),
            &request.tool_name,
            &session_log_arguments,
            None,
            recorder_metadata.clone(),
            session_contract,
        );

        if context.record_oauth_scope_denials {
            if let Err(error_status) = check_runtime_tool_scope(context.auth, &request.tool_name) {
                let error_message = match &error_status {
                    ToolCallErrorStatus::InsufficientScope { description, .. } => {
                        description.as_str()
                    }
                    ToolCallErrorStatus::InvalidArguments { message } => message.as_str(),
                };
                self.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &Value::Null,
                    Some(error_message),
                    Some("insufficient_scope"),
                );
                return ToolCallOutcome {
                    success: false,
                    result: None,
                    error_status: Some(error_status),
                    project: None,
                    model_ergonomics: None,
                    correlation: Default::default(),
                };
            }
        }

        let (mut call, input_normalization) = match parsed_call {
            Ok(parsed) => parsed,
            Err(message) => {
                self.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &Value::Null,
                    Some(&message),
                    Some("invalid_arguments"),
                );
                return ToolCallOutcome {
                    success: false,
                    result: None,
                    error_status: Some(ToolCallErrorStatus::InvalidArguments { message }),
                    project: None,
                    model_ergonomics: None,
                    correlation: Default::default(),
                };
            }
        };
        if let (Some(session_id), Some(message_resolution)) =
            (context.session_id, session_message_resolution.as_ref())
        {
            let current_request_acknowledged = outer_ack_observation
                .as_ref()
                .is_some_and(|ack| ack.accepted_ids.contains(&message_resolution.message_id));
            if let Err(error) = self.sessions.resolve_message_from_wrapper(
                session_id,
                &message_resolution.message_id,
                message_resolution.resolution.clone(),
                current_request_acknowledged,
            ) {
                let mut result = session_context::session_message_error_result(
                    session_id,
                    Some(&message_resolution.message_id),
                    error,
                );
                super::dispatch::decorate_structured_execution_prestart_denial(
                    &request.tool_name,
                    &mut result,
                    "session_message_resolution_failed",
                );
                self.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some("session_message_resolution_failed"),
                );
                super::add_session_hint(&mut result, &self.sessions, session_id);
                session_context::add_session_attention_projection(
                    &mut result,
                    &self.sessions,
                    session_id,
                    "recording_session",
                    outer_ack_observation
                        .as_ref()
                        .expect("authorized outer recorder must have ACK observation"),
                    recorder_ack_requested,
                );
                return ToolCallOutcome {
                    success: false,
                    result: Some(result),
                    error_status: None,
                    project: None,
                    model_ergonomics: None,
                    correlation: Default::default(),
                };
            }
        }
        if let ToolCall::ImportConversationFilesToProject {
            host_file_import_provenance,
            ..
        } = &mut call
        {
            *host_file_import_provenance = context.host_file_import_trust;
        }
        if let ToolCall::CompleteSessionMessage {
            trusted_recording_session_id,
            ..
        } = &mut call
        {
            // Private provenance is derived from the already-authorized outer
            // recording Session. Public arguments can never populate this field.
            *trusted_recording_session_id = context.session_id.map(str::to_string);
        }

        let project = tool_project(&call);
        if let Some(control) = control.as_mut() {
            if let Err(outcome) = control.before(self, &call, context).await {
                if let Some(result) = outcome.result.as_ref() {
                    self.sessions.record_tool_call_finished(
                        session_event,
                        false,
                        &result.output,
                        result.error.as_deref(),
                        Some("control_before_failed"),
                    );
                }
                return outcome;
            }
            control.main_dispatched = true;
        }
        // Preserve the concrete business Session for final presentation and
        // bounded ActionAudit evidence. The generic recorder remains independent
        // provenance and Window affinity never becomes execution or Session authority.
        let business_session_id = call.session_id().map(str::to_string);
        // Permission is evaluated once inside dispatch (pre-exec gate). Kernel
        // only reuses the attached decision for the outer recording session —
        // never re-evaluate (no second request id / inconsistent outcome).
        let (mut result, result_projection, mut correlation) = self
            .dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context_with_result_projection(
                call,
                context.auth,
                context.transport.into(),
                recorder_metadata.clone(),
                context.window,
                context.session_id.is_none(),
                context_request,
                super::context_projection::ContextMaterialCapabilities {
                    skill_runtime: capabilities.skill_runtime,
                    memory_surface: capabilities.memory_surface,
                },
                capabilities,
                return_timing,
            )
            .await;
        if result.success {
            if let Some(code) = input_normalization {
                let hint = match code {
                    "argv_to_args" => "normalized argv→args",
                    _ => unreachable!("parser returns only stable known normalization codes"),
                };
                result.output["input_normalization"] =
                    serde_json::json!({"code": code, "hint": hint});
            }
        }
        if let Some(control) = control.as_mut() {
            control
                .after(self, &request.tool_name, &result, context)
                .await;
        }
        if result.success {
            // The concrete ToolCall has already passed canonical business Session
            // lifecycle/authority checks. Retain only its exact identity as bounded
            // audit evidence; it never becomes recorder or execution authority.
            correlation.business_session_id = business_session_id.clone();
        }
        if let Some(session_id) = context.session_id {
            correlation.add_workflow_session(super::window_activity::WorkflowSessionCorrelation {
                session_id: session_id.to_string(),
                project: recorder_metadata.recording_session_project.clone(),
                relation: super::window_activity::WorkflowSessionCorrelationRelation::Recording,
            });
        }
        let window_attention_session_id = if context.session_id.is_none() {
            self.workflow_window_affinity_candidate(
                &request.tool_name,
                context.window,
                context.auth,
                &correlation,
            )
            .await
        } else {
            None
        };
        if result.success {
            correlation.recorder_gap_session_id = window_attention_session_id.clone();
        }
        if let Some(start) = session_event.as_mut() {
            if let Some(permission) =
                super::permissions::permission_decision_from_output(&result.output)
            {
                self.sessions.record_permission_decision(start, permission);
            }
        }
        let session_log_result = session_log_result_for_tool(&request.tool_name, &result.output);
        self.sessions.record_tool_call_finished(
            session_event,
            result.success,
            &session_log_result,
            result.error.as_deref(),
            None,
        );
        // Recorder provenance remains ledger-only. Surface only collaboration
        // guidance that can affect the model's next action, while preserving any
        // business `output.session_id` emitted by the concrete tool.
        if let Some(session_id) = context.session_id {
            super::add_session_hint(&mut result, &self.sessions, session_id);
            session_context::add_session_attention_projection(
                &mut result,
                &self.sessions,
                session_id,
                "recording_session",
                outer_ack_observation
                    .as_ref()
                    .expect("authorized outer recorder must have ACK observation"),
                recorder_ack_requested,
            );
        } else if let Some(session_id) = window_attention_session_id
            .as_deref()
            .filter(|_| result.output.get("session_attention").is_none())
        {
            // Window affinity is correlation evidence only. It may locate one
            // authorized active Session message board for request-scoped ACK and
            // delivery, but never becomes recorder, business input, execution
            // context, or durable resolution authority.
            let session_ack_ids = session_context::session_ack_message_ids(
                &self.sessions,
                session_id,
                &recorder_metadata.ack_session_message_ids,
                ack_ref.as_deref(),
            );
            let ack = session_context::observe_session_attention_acks(
                &self.sessions,
                session_id,
                &session_ack_ids,
            );
            session_context::add_session_attention_projection(
                &mut result,
                &self.sessions,
                session_id,
                "window_affinity",
                &ack,
                recorder_ack_requested,
            );
        }
        // Canonical execution evidence and every Session/context overlay are now
        // complete. Consume the request-scoped plan exactly once to produce the
        // final model-facing read/search result.
        result_projection.project(&mut result);
        if let (Some(session_id), Some(project)) = (
            correlation.recorder_gap_session_id.as_deref(),
            correlation.resolved_project.as_deref(),
        ) {
            // The gap remains correlation/audit truth. It is not actionable model
            // guidance when this exact call already supplied the same authorized
            // business Session for the same resolved Project.
            if business_session_id.as_deref() != Some(session_id) {
                if let Some(output) = result.output.as_object_mut() {
                    output.insert(
                        "workflow_recording_attention".to_string(),
                        serde_json::json!({
                            "status": "recording_session_missing",
                            "candidate_session_id": session_id,
                            "project": project,
                            "reason": "same_window_recent_explicit_association"
                        }),
                    );
                }
            }
        }
        if request.tool_name == "tool_manifest" {
            super::surface::sparsify_tool_manifest_model_result(&mut result);
        }
        // Final model-facing projection: authoritative permission decisions and
        // recorder events have already been consumed by the Session ledger.
        super::dispatch::sparsify_failure_model_result_metadata(&request.tool_name, &mut result);
        if !result.success
            && request.tool_name != "read_tool_trace"
            && capabilities.trace_diagnostics
            && context
                .auth
                .is_some_and(|auth| auth.has_scope(crate::auth::SCOPE_ADMIN))
        {
            if let (Some(trace_ref), Some(output)) = (
                crate::tool_request_trace::current_full_trace_ref(),
                result.output.as_object_mut(),
            ) {
                output.insert("trace_ref".to_string(), Value::String(trace_ref));
            }
        }
        super::dispatch::sparsify_success_model_result_metadata(&request.tool_name, &mut result);
        // The continuity hint is diagnostic only. Keep it inside the shared
        // model-result hard ceiling; if an unrelated producer already consumed
        // the full envelope, omit this non-authoritative overlay rather than
        // changing the business result.
        if result
            .output
            .as_object()
            .is_some_and(|output| output.contains_key("workflow_recording_attention"))
            && crate::json_measurement::serialized_json_len(&result).is_ok_and(|bytes| {
                bytes > webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES
            })
        {
            if let Some(output) = result.output.as_object_mut() {
                output.remove("workflow_recording_attention");
            }
        }
        if super::tool_definition::is_model_visible_tool_name(&request.tool_name) {
            self.add_window_model_reply_sidecar(
                &mut result,
                context.auth,
                context.window,
                window_reply.as_ref(),
            );
            let peer_project = correlation
                .resolved_project
                .as_deref()
                .or(recorder_metadata.recording_session_project.as_deref());
            if request.tool_name != "present_work_result" {
                self.add_window_operator_projection(
                    &mut result,
                    context.auth,
                    context.window,
                    &recorder_metadata.ack_session_message_ids,
                );
            }
            self.add_peer_collaboration_projection(
                &mut result,
                context.auth,
                context.window,
                peer_project,
                &recorder_metadata.ack_session_message_ids,
            );
        }
        if request.tool_name == "observe_jobs" {
            super::observe_jobs::sparsify_observe_jobs_model_result(&mut result);
        }
        self.add_passive_job_attention(
            &mut result,
            &request.tool_name,
            correlation.resolved_project.as_deref(),
            correlation.business_session_id.as_deref(),
            context.window,
            context.auth,
        )
        .await;
        ToolCallOutcome {
            success: result.success,
            result: Some(result),
            error_status: None,
            project,
            model_ergonomics: None,
            correlation,
        }
    }

    async fn workflow_window_affinity_candidate(
        &self,
        tool_name: &str,
        window: Option<&crate::client_window::ClientWindow>,
        auth: Option<&AuthContext>,
        correlation: &super::window_activity::ToolCallCorrelation,
    ) -> Option<String> {
        if tool_name == "work_on_project"
            || !webcodex_tool_contracts::runtime_tool_activity_interaction(tool_name)
                .is_meaningful()
        {
            return None;
        }
        let window = window?;
        let project = correlation.resolved_project.as_deref()?;
        let db = self.window_activity_db.as_ref()?;
        let (principal_kind, principal_id) =
            session_context::runtime_observation_principal(auth).ok()?;
        let affinity = db
            .latest_window_workflow_affinity(window.key(), &principal_kind, &principal_id, project)
            .ok()??;
        if affinity.project.as_deref() != Some(project)
            || self.sessions.lifecycle_state(&affinity.workflow_session_id)
                != Some(super::sessions::SessionLifecycle::Active)
            || self.sessions.session_project(&affinity.workflow_session_id)
                != Some(Some(project.to_string()))
        {
            return None;
        }
        if self
            .authorize_session_target(&affinity.workflow_session_id, tool_name, auth)
            .await
            .is_err()
        {
            return None;
        }
        Some(affinity.workflow_session_id)
    }

    async fn recording_session_project_mismatch(
        &self,
        session_id: &str,
        _tool_name: &str,
        arguments: &Value,
        auth: Option<&AuthContext>,
    ) -> Option<session_context::SessionProjectMismatch> {
        let session_project = self.sessions.session_project(session_id)??;
        let request_project = arguments
            .as_object()
            .and_then(|obj| obj.get("project"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|project| !project.is_empty())?;
        let resolved = self
            .resolve_project_input_for_auth(request_project, auth)
            .await
            .ok()?;
        if session_project == resolved.resolved_id {
            return None;
        }
        Some(session_context::SessionProjectMismatch {
            session_project,
            request_project: resolved.resolved_id,
        })
    }
}

fn collaboration_session_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "post_session_message"
            | "list_session_messages"
            | "get_session_assignment"
            | "observe_session_messages"
            | "resolve_session_message"
            | "complete_session_message"
            | "session_discussion_summary"
            | "session_handoff_summary"
    )
}

fn collaboration_target_session_id<'a>(tool_name: &str, arguments: &'a Value) -> Option<&'a str> {
    if !collaboration_session_tool(tool_name) {
        return None;
    }
    arguments
        .as_object()
        .and_then(|obj| obj.get("session_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())
}

fn tool_project(call: &ToolCall) -> Option<String> {
    call.project().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthContext, AuthKind};
    use serde_json::json;

    fn test_runtime() -> ToolRuntime {
        ToolRuntime::new_for_tests()
    }

    fn oauth(scopes: &[&str]) -> AuthContext {
        AuthContext {
            user_id: Some("u".to_string()),
            username: Some("alice".to_string()),
            role: Some("user".to_string()),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            token_kind: Some("oauth2".to_string()),
            ..AuthContext::new(AuthKind::OAuth2Token)
        }
    }

    #[tokio::test]
    async fn generic_kernel_uses_typed_invocation_metadata_with_business_only_arguments() {
        let runtime = test_runtime();
        let session = runtime
            .sessions
            .start_session(None, Some("typed invocation boundary".to_string()));
        let guidance = runtime
            .sessions
            .post_message_with_ack(
                crate::tool_runtime::sessions::PostSessionMessageInput {
                    session_id: session.session_id.clone(),
                    kind: crate::tool_runtime::sessions::SessionMessageKind::Guidance,
                    message: "retain typed invocation metadata".to_string(),
                    tags: Vec::new(),
                    reply_to: None,
                    priority: crate::tool_runtime::sessions::SessionMessagePriority::High,
                },
                true,
            )
            .unwrap();
        let business_arguments = json!({});
        let business_object = business_arguments.as_object().unwrap();
        for wrapper in [
            crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD,
            crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD,
            crate::tool_runtime::sessions::TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD,
            crate::tool_runtime::window_collaboration::TOOL_CALL_WINDOW_REPLY_FIELD,
            crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_FIELD,
        ] {
            assert!(!business_object.contains_key(wrapper));
        }
        assert!(business_object
            .keys()
            .all(|key| !key.starts_with("__webcodex_")));
        crate::tool_runtime::ToolCall::from_tool_name("tool_manifest", business_arguments.clone())
            .expect("typed ToolCall parsing must accept business-only arguments");

        let invocation_metadata = ToolInvocationMetadata {
            ack_session_message_ids: vec![guidance.message_id],
            context_request: vec!["webcodex.workflow".to_string()],

            ..Default::default()
        };
        let outcome = runtime
            .call_tool_with_invocation_metadata(
                ToolCallRequest {
                    tool_name: "tool_manifest".to_string(),
                    arguments: business_arguments,
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: Some(&session.session_id),
                    auth: None,
                    window: None,
                    record_oauth_scope_denials: false,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
                invocation_metadata,
                ToolProtocolCapabilities {
                    context_sidecar: true,
                    ..Default::default()
                },
            )
            .await;
        assert!(outcome.error_status.is_none(), "{:?}", outcome.error_status);
        let result = outcome.result.expect("model-facing result");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(
            result.output["session_attention"]["ack"]["accepted_count"],
            1
        );
        assert_eq!(
            result.output["context_projection"]["materials"][0]["key"],
            "webcodex.workflow"
        );
    }

    #[tokio::test]
    async fn tool_kernel_records_success_event() {
        let runtime = test_runtime();
        let session = runtime.sessions.start_session(None, None);
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "list_projects".to_string(),
                    arguments: json!({}),
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: Some(&session.session_id),
                    auth: None,
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;

        assert!(outcome.success);
        assert!(outcome.error_status.is_none());
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 1);
        assert_eq!(summary.counts.succeeded, 1);
        assert_eq!(summary.events[0].kind, "tool_call_started");
        assert_eq!(summary.events[1].kind, "tool_call_finished");
        assert_eq!(summary.events[1].status.as_deref(), Some("succeeded"));
    }

    #[tokio::test]
    async fn tool_kernel_records_failure_event() {
        let runtime = test_runtime();
        let session = runtime.sessions.start_session(None, None);
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "read_files".to_string(),
                    arguments: json!({"project": "demo"}),
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: Some(&session.session_id),
                    auth: None,
                    window: None,
                    record_oauth_scope_denials: false,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;

        assert!(!outcome.success);
        assert!(matches!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InvalidArguments { .. })
        ));
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 1);
        assert_eq!(summary.counts.failed, 1);
        let finished = &summary.events[1];
        assert_eq!(finished.transport, "mcp");
        assert_eq!(finished.error_kind.as_deref(), Some("invalid_arguments"));
    }

    #[test]
    fn start_coding_task_uses_generic_unknown_scope_policy() {
        let runtime_read = oauth(&[crate::auth::SCOPE_RUNTIME_READ]);
        assert_eq!(
            check_runtime_tool_scope(Some(&runtime_read), "start_coding_task"),
            check_runtime_tool_scope(Some(&runtime_read), "definitely_unknown_tool")
        );
    }

    #[tokio::test]
    async fn piggyback_resolution_cannot_inherit_main_tool_scope() {
        let runtime = test_runtime();
        let auth = oauth(&["project:read"]);
        let fingerprint = crate::tool_runtime::workflow_session_authority_fingerprint(Some(&auth))
            .expect("OAuth test authority must have a stable identity");
        let session = runtime
            .sessions
            .start_session_with_options(
                crate::tool_runtime::sessions::SessionCreateOptions::new(
                    None,
                    Some("piggyback scope fence".to_string()),
                    crate::tool_runtime::SessionMode::Normal,
                    crate::tool_runtime::sessions::SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(fingerprint)),
            )
            .unwrap();
        let message = runtime
            .sessions
            .post_message_with_ack(
                crate::tool_runtime::sessions::PostSessionMessageInput {
                    session_id: session.session_id.clone(),
                    kind: crate::tool_runtime::sessions::SessionMessageKind::Note,
                    message: "close only with Session mutation authority".to_string(),
                    tags: Vec::new(),
                    reply_to: None,
                    priority: crate::tool_runtime::sessions::SessionMessagePriority::Normal,
                },
                false,
            )
            .unwrap();
        let arguments = json!({
            "project": "demo",
            "items": [{"path": "README.md"}]
        });

        let outcome = runtime
            .call_tool_with_invocation_metadata(
                ToolCallRequest {
                    tool_name: "read_files".to_string(),
                    arguments,
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: Some(&session.session_id),
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: false,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
                ToolInvocationMetadata {
                    session_message_resolution: Some(ToolCallSessionMessageResolution {
                        message_id: message.message_id.clone(),
                        resolution: "handled".to_string(),
                    }),
                    ..Default::default()
                },
                ToolProtocolCapabilities::default(),
            )
            .await;

        assert_eq!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_SESSION_COLLABORATE),
                description: "missing required scope: session:collaborate".to_string(),
            })
        );
        assert!(outcome.result.is_none());
        let retained = runtime
            .sessions
            .list_messages(
                &session.session_id,
                crate::tool_runtime::sessions::ListSessionMessagesFilter {
                    message_id: Some(message.message_id),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(
            retained[0].status,
            crate::tool_runtime::sessions::SessionMessageStatus::Open,
            "scope denial must happen before the piggyback closure mutation"
        );
        assert!(retained[0].resolution.is_none());
    }

    #[test]
    fn session_message_resolution_reuses_dedicated_resolve_scope() {
        let project_read_only = oauth(&["project:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&project_read_only), "read_files"),
            Ok(()),
            "main project read authority must remain independent"
        );
        assert_eq!(
            check_session_message_resolution_scope(Some(&project_read_only), true),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_SESSION_COLLABORATE),
                description: "missing required scope: session:collaborate".to_string(),
            }),
            "piggyback resolution must not inherit the main tool scope"
        );
        assert_eq!(
            check_session_message_resolution_scope(Some(&project_read_only), false),
            Ok(()),
            "ordinary calls without resolution keep their existing scope contract"
        );
        let runtime_read = oauth(&["runtime:read"]);
        assert_eq!(
            check_session_message_resolution_scope(Some(&runtime_read), true),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_SESSION_COLLABORATE),
                description: "missing required scope: session:collaborate".to_string(),
            }),
            "runtime observation authority must not resolve Session messages"
        );
        let collaborator = oauth(&["session:collaborate"]);
        assert_eq!(
            check_session_message_resolution_scope(Some(&collaborator), true),
            Ok(()),
            "piggyback resolution must track the dedicated resolve tool policy"
        );
    }

    #[test]
    fn coding_agent_tools_require_independent_execution_scope() {
        for insufficient in [
            oauth(&["project:write"]),
            oauth(&["job:run"]),
            oauth(&["mcp:local"]),
            oauth(&["project:write", "job:run", "mcp:local"]),
        ] {
            for tool in [
                "coding_agent_start",
                "coding_agent_observe",
                "coding_agent_cancel",
            ] {
                assert_eq!(
                    check_runtime_tool_scope(Some(&insufficient), tool),
                    Err(ToolCallErrorStatus::InsufficientScope {
                        required_scope: Some(crate::auth::SCOPE_CODING_AGENT_RUN),
                        description: "missing required scope: coding_agent:run".to_string(),
                    }),
                    "{tool} must not inherit project/job/MCP authority"
                );
            }
        }
        let run_only = oauth(&["coding_agent:run"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&run_only), "coding_agent_start"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_PROJECT_WRITE),
                description: "missing required scope: project:write".to_string(),
            })
        );
        for tool in ["coding_agent_observe", "coding_agent_cancel"] {
            assert_eq!(
                check_runtime_tool_scope(Some(&run_only), tool),
                Ok(()),
                "{tool}"
            );
        }
        let start_allowed = oauth(&["coding_agent:run", "project:write"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&start_allowed), "coding_agent_start"),
            Ok(())
        );
    }

    #[test]
    fn agent_task_coding_run_tools_require_task_and_execution_authority() {
        let communication_only = oauth(&["communication:read", "communication:manage"]);
        for tool in [
            "start_agent_task_coding_run",
            "reconcile_agent_task_coding_run",
        ] {
            assert_eq!(
                check_runtime_tool_scope(Some(&communication_only), tool),
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: Some(crate::auth::SCOPE_CODING_AGENT_RUN),
                    description: "missing required scope: coding_agent:run".to_string(),
                }),
                "communication authority must not grant CodingAgent execution/observation authority for {tool}"
            );
        }

        let task_and_run = oauth(&[
            "communication:read",
            "communication:manage",
            "coding_agent:run",
        ]);
        assert_eq!(
            check_runtime_tool_scope(Some(&task_and_run), "start_agent_task_coding_run"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_PROJECT_WRITE),
                description: "missing required scope: project:write".to_string(),
            }),
            "Task ownership and CodingAgent authority must not imply Project write authority"
        );
        assert_eq!(
            check_runtime_tool_scope(
                Some(&task_and_run),
                "reconcile_agent_task_coding_run"
            ),
            Ok(()),
            "reconciliation observes exact bound execution and must not require a new Project write grant"
        );

        let start_allowed = oauth(&[
            "communication:read",
            "communication:manage",
            "coding_agent:run",
            "project:write",
        ]);
        assert_eq!(
            check_runtime_tool_scope(Some(&start_allowed), "start_agent_task_coding_run"),
            Ok(())
        );
    }

    #[test]
    fn computer_gateway_outer_scopes_are_minimal_and_action_neutral() {
        assert_eq!(check_runtime_tool_scope(None, "computer_control"), Ok(()));
        assert!(check_runtime_tool_scope(None, "plugin_tool").is_err());

        let denied = oauth(&["runtime:read", "project:read"]);
        assert!(check_runtime_tool_scope(Some(&denied), "computer_observe").is_err());
        assert!(check_runtime_tool_scope(Some(&denied), "computer_control").is_err());

        let observe = oauth(&["computer:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&observe), "computer_observe"),
            Ok(())
        );
        assert!(check_runtime_tool_scope(Some(&observe), "computer_control").is_err());

        let control = oauth(&["computer:control"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&control), "computer_control"),
            Ok(())
        );
        assert!(check_runtime_tool_scope(Some(&control), "computer_observe").is_err());

        let launch = oauth(&["computer:launch"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&launch), "computer_control"),
            Ok(())
        );
        assert!(check_runtime_tool_scope(Some(&launch), "computer_observe").is_err());
    }

    #[test]
    fn computer_save_snapshot_requires_project_write_and_computer_read() {
        let read_only = oauth(&["computer:read"]);
        assert!(check_runtime_tool_scope(Some(&read_only), "computer_save_snapshot").is_err());
        let write_only = oauth(&["project:write"]);
        assert!(check_runtime_tool_scope(Some(&write_only), "computer_save_snapshot").is_err());
        let both = oauth(&["project:write", "computer:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&both), "computer_save_snapshot"),
            Ok(())
        );
    }

    #[test]
    fn tool_scope_enforcement_applies_to_pat_and_direct_shared_key() {
        let mut pat = AuthContext::new(crate::auth::AuthKind::ApiToken);
        pat.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.to_string()];
        assert_eq!(
            check_runtime_tool_scope(Some(&pat), "read_files"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_PROJECT_READ),
                description: "missing required scope: project:read".to_string(),
            })
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&pat), "runtime_status"),
            Ok(())
        );

        let shared = crate::auth::shared_key_context("kernel-scope-matrix");
        assert_eq!(
            check_runtime_tool_scope(Some(&shared), "read_files"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&shared), "computer_observe"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&shared), "computer_control"),
            Ok(())
        );
    }

    #[tokio::test]
    async fn tool_kernel_rejects_missing_scope() {
        let runtime = test_runtime();
        let auth = oauth(&["runtime:read"]);
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "read_files".to_string(),
                    arguments: json!({"project": "demo", "items": [{"path": "README.md"}]}),
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: None,
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;

        assert!(!outcome.success);
        assert_eq!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_PROJECT_READ),
                description: "missing required scope: project:read".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn tool_kernel_unknown_tool_fails_closed_or_invalid() {
        let runtime = test_runtime();
        let auth = oauth(&["runtime:read", "project:read"]);
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "definitely_not_a_tool".to_string(),
                    arguments: Value::Null,
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: None,
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: false,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;

        assert!(!outcome.success);
        assert!(matches!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InsufficientScope {
                required_scope: None,
                ..
            })
        ));

        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "definitely_not_a_tool".to_string(),
                    arguments: Value::Null,
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: None,
                    auth: None,
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust:
                        crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                },
            )
            .await;
        assert!(matches!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InvalidArguments { .. })
        ));
    }
}

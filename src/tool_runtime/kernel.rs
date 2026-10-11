use super::model_ergonomics_telemetry::{ModelErgonomicsCompletion, ModelErgonomicsTimer};
use super::sessions::{
    SessionTransport, ToolCallRecorderMetadata, ToolCallSessionMessageResolution,
};
use super::{session_context, HostFileImportProvenance, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde_json::Value;

mod admission;
mod postprocess;
mod recording;

pub(crate) use admission::{check_runtime_tool_scope, runtime_tool_scope_allows_discovery};

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
    /// Explicit presentation request; never dispatch or continuation authority.
    pub(crate) compact_execution: bool,
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
    /// Adapter admission for ModelHidden presentation-artifact reads; never supplies Project authority.
    pub(crate) artifact_app: bool,
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
    /// Existing privacy-safe audit projection captured before model compaction.
    /// Never serialized into ToolResult or interpreted as execution authority.
    pub(crate) canonical_audit_output: Option<Value>,
    /// Canonical pre-model-projection state-change truth for request-local
    /// internal effect accounting. This is never serialized into ToolResult and
    /// carries no authorization, source-fence, or replay authority.
    #[cfg_attr(not(feature = "experimental-code-mode"), allow(dead_code))]
    pub(crate) canonical_state_changed: Option<bool>,
    /// Trusted internal Window/Workflow Session correlation evidence. This is
    /// adapter metadata only and is never part of the public ToolResult.
    pub(crate) correlation: super::window_activity::ToolCallCorrelation,
}

impl ToolCallOutcome {
    fn rejected(error_status: ToolCallErrorStatus) -> Self {
        Self {
            error_status: Some(error_status),
            ..Self::rejected_without_result()
        }
    }

    fn rejected_result(result: ToolResult) -> Self {
        Self {
            result: Some(result),
            ..Self::rejected_without_result()
        }
    }

    fn rejected_without_result() -> Self {
        Self {
            success: false,
            result: None,
            error_status: None,
            project: None,
            model_ergonomics: None,
            canonical_audit_output: None,
            canonical_state_changed: None,
            correlation: Default::default(),
        }
    }
}

fn session_selector_failure(error: super::SessionSelectorError) -> ToolCallOutcome {
    match error {
        super::SessionSelectorError::UnknownRef(message) => {
            ToolCallOutcome::rejected(ToolCallErrorStatus::InvalidArguments { message })
        }
        super::SessionSelectorError::RetentionExpired { session_id } => ToolCallOutcome {
            success: false,
            result: Some(session_context::session_retention_expired_result(
                &session_id,
            )),
            error_status: None,
            project: None,
            model_ergonomics: None,
            canonical_audit_output: None,
            canonical_state_changed: None,
            correlation: Default::default(),
        },
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
                artifact_app: false,
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
                telemetry.invocation =
                    super::model_ergonomics_telemetry::invocation::InvocationFacts::from_metadata(
                        &invocation_metadata,
                        context.session_id.is_some(),
                    );
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
                    &mut telemetry,
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
        telemetry: &mut Option<ModelErgonomicsTimer>,
    ) -> ToolCallOutcome {
        // Admit before any tool-specific await or side effect. The permit is
        // retained through the complete call, including direct SSH effects.
        let _maintenance_permit = match self.runner_registry.admit_runtime_call().await {
            Ok(permit) => permit,
            Err(message) => {
                return ToolCallOutcome::rejected(ToolCallErrorStatus::InvalidArguments {
                    message: message.to_string(),
                })
            }
        };
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
        if let Err(outcome) =
            admission::check_protocol_capabilities(&request.tool_name, context.auth, capabilities)
        {
            return outcome;
        }
        let canonical_recording_session_id = match context.session_id {
            Some(raw) => match self.canonicalize_explicit_session_selector(raw, context.auth) {
                Ok(canonical) => Some(canonical),
                Err(error) => return session_selector_failure(error),
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
        let context_request = if capabilities.context_sidecar {
            invocation_metadata.context_request
        } else {
            Vec::new()
        };
        let recording::PreparedInvocation {
            call,
            input_normalization,
            mut recording,
        } = match recording::PreparedInvocation::prepare(
            self,
            &request,
            context,
            recorder_metadata,
            ack_ref.as_deref(),
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(outcome) => return outcome,
        };

        let project = tool_project(&call);
        if let Some(control) = control.as_mut() {
            if let Err(outcome) = control.before(self, &call, context).await {
                if let Some(result) = outcome.result.as_ref() {
                    self.sessions.record_tool_call_finished(
                        recording.event.take(),
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
        let business_session_id = match &call {
            // Presentation has an explicit business selector, but deliberately
            // has no generic Session recorder/lifecycle target.
            ToolCall::PresentWorkResult { session_id, .. } => session_id.as_deref(),
            _ => call.session_id(),
        }
        .map(str::to_string);
        // Permission is evaluated once inside dispatch (pre-exec gate). Kernel
        // only reuses the attached decision for the outer recording session —
        // never re-evaluate (no second request id / inconsistent outcome).
        let execution_started = std::time::Instant::now();
        let (mut result, result_projection, mut correlation) = self
            .dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context_with_result_projection(
                call,
                context.auth,
                context.transport.into(),
                recording.metadata.clone(),
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
        crate::tool_request_trace::record_phase_latency(
            "canonical_execution",
            execution_started,
            if result.success {
                "completed"
            } else {
                "failed"
            },
        );
        if result.success {
            if let Some(code) = input_normalization {
                result.output["input_normalization"] =
                    serde_json::json!({"code": code, "hint": code.model_hint()});
            }
        }
        if let Some(control) = control.as_mut() {
            control
                .after(self, &request.tool_name, &result, context)
                .await;
        }
        recording
            .finish(
                self,
                &request.tool_name,
                context,
                business_session_id.as_deref(),
                &mut result,
                &mut correlation,
                ack_ref.as_deref(),
            )
            .await;
        let canonical_state_changed = result.output.get("state_changed").and_then(Value::as_bool);
        // Session/permission evidence is sealed above. The response stage owns
        // canonical audit capture, one-shot model projection, and late sidecars.
        let postprocess::PostRecordResult {
            result,
            canonical_audit_output,
        } = postprocess::PostRecordResponse {
            compact_execution: invocation_metadata.compact_execution,
            tool_name: &request.tool_name,
            context,
            capabilities,
            recorder: &recording.metadata,
            correlation: &correlation,
            business_session_id: business_session_id.as_deref(),
            window_reply: window_reply.as_ref(),
        }
        .finish(self, result, result_projection, telemetry.as_mut())
        .await;
        ToolCallOutcome {
            success: result.success,
            result: Some(result),
            error_status: None,
            project,
            model_ergonomics: None,
            canonical_audit_output,
            canonical_state_changed,
            correlation,
        }
    }
}

fn tool_project(call: &ToolCall) -> Option<String> {
    call.project().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::admission::check_session_message_resolution_scope;
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
        crate::tool_runtime::ToolCall::from_tool_name(
            "read_tool_manifest",
            business_arguments.clone(),
        )
        .expect("typed ToolCall parsing must accept business-only arguments");

        let invocation_metadata = ToolInvocationMetadata {
            ack_session_message_ids: vec![guidance.message_id],
            context_request: vec!["webcodex.workflow".to_string()],

            ..Default::default()
        };
        let outcome = runtime
            .call_tool_with_invocation_metadata(
                ToolCallRequest {
                    tool_name: "read_tool_manifest".to_string(),
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
                "start_coding_agent",
                "observe_coding_agent",
                "cancel_coding_agent",
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
            check_runtime_tool_scope(Some(&run_only), "start_coding_agent"),
            Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_PROJECT_WRITE),
                description: "missing required scope: project:write".to_string(),
            })
        );
        for tool in ["observe_coding_agent", "cancel_coding_agent"] {
            assert_eq!(
                check_runtime_tool_scope(Some(&run_only), tool),
                Ok(()),
                "{tool}"
            );
        }
        let start_allowed = oauth(&["coding_agent:run", "project:write"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&start_allowed), "start_coding_agent"),
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
        assert_eq!(check_runtime_tool_scope(None, "control_computer"), Ok(()));
        assert!(check_runtime_tool_scope(None, "plugin_tool").is_err());

        let denied = oauth(&["runtime:read", "project:read"]);
        assert!(check_runtime_tool_scope(Some(&denied), "observe_computer").is_err());
        assert!(check_runtime_tool_scope(Some(&denied), "control_computer").is_err());

        let observe = oauth(&["computer:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&observe), "observe_computer"),
            Ok(())
        );
        assert!(check_runtime_tool_scope(Some(&observe), "control_computer").is_err());

        let control = oauth(&["computer:control"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&control), "control_computer"),
            Ok(())
        );
        assert!(check_runtime_tool_scope(Some(&control), "observe_computer").is_err());

        let launch = oauth(&["computer:launch"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&launch), "control_computer"),
            Ok(())
        );
        assert!(check_runtime_tool_scope(Some(&launch), "observe_computer").is_err());
    }

    #[test]
    fn computer_save_snapshot_requires_project_write_and_computer_read() {
        let read_only = oauth(&["computer:read"]);
        assert!(check_runtime_tool_scope(Some(&read_only), "save_computer_snapshot").is_err());
        let write_only = oauth(&["project:write"]);
        assert!(check_runtime_tool_scope(Some(&write_only), "save_computer_snapshot").is_err());
        let both = oauth(&["project:write", "computer:read"]);
        assert_eq!(
            check_runtime_tool_scope(Some(&both), "save_computer_snapshot"),
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
            check_runtime_tool_scope(Some(&pat), "get_runtime_status"),
            Ok(())
        );

        let shared = crate::auth::shared_key_context("kernel-scope-matrix");
        assert_eq!(
            check_runtime_tool_scope(Some(&shared), "read_files"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&shared), "observe_computer"),
            Ok(())
        );
        assert_eq!(
            check_runtime_tool_scope(Some(&shared), "control_computer"),
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

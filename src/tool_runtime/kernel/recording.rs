//! Explicit recorder authorization, parsing and evidence. Recorder provenance
//! stays independent of the business Session and never authorizes execution.
use super::admission::check_session_message_resolution_scope;
use super::{
    check_runtime_tool_scope, session_selector_failure, ToolCallContext, ToolCallErrorStatus,
    ToolCallOutcome, ToolCallRequest, ToolTransport,
};
use crate::auth::AuthContext;
use crate::tool_runtime::sessions::{
    self, strip_tool_call_expectation_metadata, SessionAckObservation, ToolCallRecorderMetadata,
    ToolCallStart,
};
use crate::tool_runtime::tool_audit::{
    session_log_arguments_for_tool_request, session_log_arguments_for_typed_call,
    session_log_result_for_tool,
};
use crate::tool_runtime::{session_context, ToolCall, ToolResult, ToolRuntime};
use serde_json::Value;

pub(super) struct PreparedInvocation {
    pub call: ToolCall,
    pub input_normalization: Option<webcodex_tool_contracts::ToolInputNormalizationCode>,
    pub recording: InvocationRecording,
}

pub(super) struct InvocationRecording {
    pub metadata: ToolCallRecorderMetadata,
    pub event: Option<ToolCallStart>,
    ack: Option<SessionAckObservation>,
    ack_requested: bool,
}

impl PreparedInvocation {
    pub(super) async fn prepare(
        runtime: &ToolRuntime,
        request: &ToolCallRequest,
        context: ToolCallContext<'_>,
        mut recorder_metadata: ToolCallRecorderMetadata,
        ack_ref: Option<&str>,
    ) -> Result<Self, ToolCallOutcome> {
        let mut concrete_arguments =
            strip_tool_call_expectation_metadata(request.arguments.clone());
        // A wrapper recording_session_id is authority-bearing internal context,
        // not a ledger address. Authorize it before any lifecycle/guard lookup,
        // project mismatch computation, provenance derivation, or ledger write.
        if let Some(recorder_session_id) = context.session_id {
            if let Err(mut result) = runtime
                .authorize_session_target(recorder_session_id, &request.tool_name, context.auth)
                .await
            {
                crate::tool_runtime::dispatch::decorate_structured_execution_prestart_denial(
                    &request.tool_name,
                    &mut result,
                    "session_authority_denied",
                );
                return Err(ToolCallOutcome::rejected_result(result));
            }
        }
        let recorder_ack_requested =
            !recorder_metadata.ack_session_message_ids.is_empty() || ack_ref.is_some();
        let explicit_session_ack_ids = context.session_id.map(|recorder_session_id| {
            session_context::session_ack_message_ids(
                &runtime.sessions,
                recorder_session_id,
                &recorder_metadata.ack_session_message_ids,
                ack_ref,
            )
        });
        if let Some(recorder_session_id) = context.session_id {
            recorder_metadata.recording_session_id = Some(recorder_session_id.to_string());
            recorder_metadata.recording_session_project = runtime
                .sessions
                .session_project(recorder_session_id)
                .expect("authorized recording Session must exist");
            recorder_metadata.recording_session_authorized = true;
        }
        let session_message_resolution = (context.transport == ToolTransport::Mcp)
            .then(|| recorder_metadata.session_message_resolution.clone())
            .flatten();
        if session_message_resolution.is_some() && context.session_id.is_none() {
            return Err(ToolCallOutcome::rejected(
                ToolCallErrorStatus::InvalidArguments {
                    message: "session_message_resolution requires recording_session_id".to_string(),
                },
            ));
        }
        if let Err(error_status) = check_session_message_resolution_scope(
            context.auth,
            session_message_resolution.is_some(),
        ) {
            return Err(ToolCallOutcome::rejected(error_status));
        }
        if let Err(error) =
            runtime.canonicalize_session_reference_argument(&mut concrete_arguments, context.auth)
        {
            return Err(session_selector_failure(error));
        }

        let outer_ack_observation = context.session_id.map(|recorder_session_id| {
            session_context::observe_session_attention_acks(
                &runtime.sessions,
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
                if let Err(result) = runtime
                    .authorize_session_target(target_session_id, &request.tool_name, context.auth)
                    .await
                {
                    return Err(ToolCallOutcome::rejected_result(result));
                }
                let recorder_project = runtime
                    .sessions
                    .session_project(recorder_session_id)
                    .expect("authorized recording Session must exist");
                let target_project = runtime
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
                    return Err(ToolCallOutcome::rejected_result(result));
                }
            }
        }
        let session_contract = sessions::session_tool_contract(&request.tool_name);
        let recording_session_project_mismatch = match context.session_id {
            Some(session_id) => {
                runtime
                    .recording_session_project_mismatch(
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
            let session_event = runtime.sessions.record_tool_call_started_with_metadata(
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
            crate::tool_runtime::dispatch::decorate_structured_execution_prestart_denial(
                &request.tool_name,
                &mut result,
                session_context::SESSION_PROJECT_MISMATCH_KIND,
            );
            runtime.sessions.record_tool_call_finished(
                session_event,
                false,
                &result.output,
                result.error.as_deref(),
                Some(session_context::SESSION_PROJECT_MISMATCH_KIND),
            );
            crate::tool_runtime::add_session_hint(&mut result, &runtime.sessions, session_id);
            session_context::add_session_attention_projection(
                &mut result,
                &runtime.sessions,
                session_id,
                "recording_session",
                outer_ack_observation
                    .as_ref()
                    .expect("authorized outer recorder must have ACK observation"),
                recorder_ack_requested,
            );
            return Err(ToolCallOutcome::rejected_result(result));
        }
        // The outer recording Session is provenance/context only. Its lifecycle,
        // mode, and guards never become business execution policy. In particular,
        // closed recorders remain valid evidence sinks (the Session store supports
        // append-only recorder events on cold closed records). Concrete business
        // Session lifecycle/guards are enforced later from ToolCall::session_id().

        if !context.record_oauth_scope_denials {
            if let Err(error_status) = check_runtime_tool_scope(context.auth, &request.tool_name) {
                return Err(ToolCallOutcome::rejected(error_status));
            }
        }

        // Parse the business request once. Successful typed input drives both
        // pre-execution audit projection and later dispatch. Malformed input is
        // recorded with an empty request projection rather than reparsed through
        // a schema-filter fallback.
        crate::tool_request_trace::capture_effective_arguments(
            &request.tool_name,
            &concrete_arguments,
        );
        let parsed_call =
            ToolCall::from_tool_name_with_normalization(&request.tool_name, concrete_arguments);
        let session_log_arguments = parsed_call
            .as_ref()
            .map(|(call, _)| session_log_arguments_for_typed_call(&request.tool_name, call))
            .unwrap_or_else(|_| Value::Object(Default::default()));
        let session_event = runtime.sessions.record_tool_call_started_with_metadata(
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
                runtime.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &Value::Null,
                    Some(error_message),
                    Some("insufficient_scope"),
                );
                return Err(ToolCallOutcome::rejected(error_status));
            }
        }

        let (mut call, input_normalization) = match parsed_call {
            Ok(parsed) => parsed,
            Err(message) => {
                runtime.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &Value::Null,
                    Some(&message),
                    Some("invalid_arguments"),
                );
                return Err(ToolCallOutcome::rejected(
                    ToolCallErrorStatus::InvalidArguments { message },
                ));
            }
        };
        if let (Some(session_id), Some(message_resolution)) =
            (context.session_id, session_message_resolution.as_ref())
        {
            let current_request_acknowledged = outer_ack_observation
                .as_ref()
                .is_some_and(|ack| ack.accepted_ids.contains(&message_resolution.message_id));
            if let Err(error) = runtime.sessions.resolve_message_from_wrapper(
                session_id,
                &message_resolution.message_id,
                message_resolution.resolution.clone(),
                current_request_acknowledged,
            ) {
                let mut result = session_context::session_message_error_result(
                    &runtime.sessions,
                    context.auth,
                    session_id,
                    Some(&message_resolution.message_id),
                    error,
                );
                crate::tool_runtime::dispatch::decorate_structured_execution_prestart_denial(
                    &request.tool_name,
                    &mut result,
                    "session_message_resolution_failed",
                );
                runtime.sessions.record_tool_call_finished(
                    session_event,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some("session_message_resolution_failed"),
                );
                crate::tool_runtime::add_session_hint(&mut result, &runtime.sessions, session_id);
                session_context::add_session_attention_projection(
                    &mut result,
                    &runtime.sessions,
                    session_id,
                    "recording_session",
                    outer_ack_observation
                        .as_ref()
                        .expect("authorized outer recorder must have ACK observation"),
                    recorder_ack_requested,
                );
                return Err(ToolCallOutcome::rejected_result(result));
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

        Ok(Self {
            call,
            input_normalization,
            recording: InvocationRecording {
                metadata: recorder_metadata,
                event: session_event,
                ack: outer_ack_observation,
                ack_requested: recorder_ack_requested,
            },
        })
    }
}

impl InvocationRecording {
    pub(super) async fn finish(
        &mut self,
        runtime: &ToolRuntime,
        tool_name: &str,
        context: ToolCallContext<'_>,
        business_session_id: Option<&str>,
        result: &mut ToolResult,
        correlation: &mut crate::tool_runtime::ToolCallCorrelation,
        ack_ref: Option<&str>,
    ) {
        if result.success {
            // The concrete ToolCall has already passed canonical business Session
            // lifecycle/authority checks. Retain only its exact identity as bounded
            // audit evidence; it never becomes recorder or execution authority.
            correlation.business_session_id = business_session_id.map(str::to_string);
        }
        if let Some(session_id) = context.session_id {
            correlation.add_workflow_session(crate::tool_runtime::window_activity::WorkflowSessionCorrelation {
                session_id: session_id.to_string(),
                project: self.metadata.recording_session_project.clone(),
                relation: crate::tool_runtime::window_activity::WorkflowSessionCorrelationRelation::Recording,
            });
        }
        let window_attention_session_id = if context.session_id.is_none() {
            runtime
                .workflow_window_affinity_candidate(
                    tool_name,
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
        if let Some(start) = self.event.as_mut() {
            if let Some(permission) =
                crate::tool_runtime::permissions::permission_decision_from_output(&result.output)
            {
                runtime
                    .sessions
                    .record_permission_decision(start, permission);
            }
        }
        let session_log_result = session_log_result_for_tool(tool_name, &result.output);
        runtime.sessions.record_tool_call_finished(
            self.event.take(),
            result.success,
            &session_log_result,
            result.error.as_deref(),
            None,
        );
        // Recorder provenance remains ledger-only. Surface only collaboration
        // guidance that can affect the model's next action, while preserving any
        // business `output.session_id` emitted by the concrete tool.
        if let Some(session_id) = context.session_id {
            crate::tool_runtime::add_session_hint(result, &runtime.sessions, session_id);
            session_context::add_session_attention_projection(
                result,
                &runtime.sessions,
                session_id,
                "recording_session",
                self.ack
                    .as_ref()
                    .expect("authorized outer recorder must have ACK observation"),
                self.ack_requested,
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
                &runtime.sessions,
                session_id,
                &self.metadata.ack_session_message_ids,
                ack_ref,
            );
            let ack = session_context::observe_session_attention_acks(
                &runtime.sessions,
                session_id,
                &session_ack_ids,
            );
            session_context::add_session_attention_projection(
                result,
                &runtime.sessions,
                session_id,
                "window_affinity",
                &ack,
                self.ack_requested,
            );
        }
    }
}

impl ToolRuntime {
    async fn workflow_window_affinity_candidate(
        &self,
        tool_name: &str,
        window: Option<&crate::client_window::ClientWindow>,
        auth: Option<&AuthContext>,
        correlation: &crate::tool_runtime::window_activity::ToolCallCorrelation,
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
                != Some(sessions::SessionLifecycle::Active)
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
            | "read_session_discussion_summary"
            | "read_session_handoff"
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

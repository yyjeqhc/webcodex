//! Runtime tool dispatch and session/permission guard flow.

use super::context_projection::ContextProjectionRequest;
use super::edit_tool_telemetry;
use super::kernel::{ToolCallContext, ToolProtocolCapabilities, ToolTransport};
use super::result_projection::ModelFacingProjectionPlan;
use super::return_timing::ToolReturnTimingPolicy;
use super::session_context::{
    add_session_hint, session_guard_denied_result, session_lifecycle_denied_result,
    session_project_mismatch_result, SessionProjectMismatch,
};
use super::{permissions, session_context, sessions, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::tool_runtime::project_resolution::{ProjectResolverError, ResolvedProject};
use crate::tool_runtime::tool_audit::ToolCallAuditProjection;
use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
use serde_json::Value;
mod agent_work;
mod agents;
#[cfg(feature = "experimental-code-mode")]
mod code_mode;
mod execution;
mod gateways;
mod goals;
mod governance;
mod job_continuation;
mod memory;
mod observations;
mod preflight;
mod skills;
mod workflows;

use observations::add_run_process_expectation_projection;
pub(super) use preflight::decorate_structured_execution_prestart_denial;
use preflight::{canonical_execution_project_binding, CanonicalProjectOutput};

/// Request metadata carried intact from the adapter through governance. The
/// caller.session_id is only the outer Kernel recorder; the ToolCall retains
/// the business Session. Recorder presence selects the existing recording owner,
/// never the business Session, execution target or authority.
pub(crate) struct DispatchContext<'a> {
    pub(crate) caller: ToolCallContext<'a>,
    pub(crate) recorder: sessions::ToolCallRecorderMetadata,
    pub(crate) context_request: Vec<String>,
    pub(crate) capabilities: ToolProtocolCapabilities,
    pub(crate) return_timing: ToolReturnTimingPolicy,
}

impl<'a> DispatchContext<'a> {
    pub(crate) fn new(caller: ToolCallContext<'a>) -> Self {
        Self {
            caller,
            recorder: Default::default(),
            context_request: Vec::new(),
            capabilities: Default::default(),
            return_timing: ToolReturnTimingPolicy::unconstrained(),
        }
    }

    pub(crate) fn internal(
        auth: Option<&'a AuthContext>,
        transport: sessions::SessionTransport,
        window: Option<&'a crate::client_window::ClientWindow>,
    ) -> Self {
        Self::new(ToolCallContext {
            auth,
            transport: match transport {
                sessions::SessionTransport::Api => ToolTransport::Api,
                sessions::SessionTransport::Mcp => ToolTransport::Mcp,
            },
            session_id: None,
            window,
            record_oauth_scope_denials: false,
            host_file_import_trust: super::HostFileImportProvenance::Untrusted,
        })
    }

    fn material_capabilities(&self) -> super::context_projection::ContextMaterialCapabilities {
        super::context_projection::ContextMaterialCapabilities {
            skill_runtime: self.capabilities.skill_runtime,
            memory_surface: self.capabilities.memory_surface,
        }
    }
}

impl ToolRuntime {
    /// No-auth convenience for local-executor tests. Runner-backed tools still
    /// require the owner context supplied by `dispatch_with_auth`.
    #[cfg(test)]
    pub async fn dispatch(&self, call: ToolCall) -> ToolResult {
        self.dispatch_with_auth(call, None).await
    }

    /// Keep the canonical dispatch future on the heap at the public boundary.
    pub fn dispatch_with_auth<'a>(
        &'a self,
        call: ToolCall,
        auth: Option<&'a AuthContext>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
        Box::pin(async move {
            self.dispatch_with_context(
                call,
                DispatchContext::internal(auth, sessions::SessionTransport::Api, None),
            )
            .await
        })
    }

    pub(crate) async fn dispatch_with_context(
        &self,
        call: ToolCall,
        context: DispatchContext<'_>,
    ) -> ToolResult {
        let (mut result, projection, _) = self.dispatch_canonical(call, context).await;
        projection.project(&mut result);
        result
    }

    fn context_guidance_profile(
        &self,
        call: &ToolCall,
        transport: sessions::SessionTransport,
    ) -> CodingGuidanceProfile {
        let requested = match call {
            ToolCall::WorkOnProject {
                guidance_profile, ..
            } => *guidance_profile,
            _ => None,
        };
        self.mcp_host_policy.effective_guidance_profile(
            requested,
            matches!(transport, sessions::SessionTransport::Mcp),
        )
    }

    /// Return canonical evidence and a one-shot model projection. The Kernel
    /// records the evidence before consuming the projection.
    pub(super) async fn dispatch_canonical(
        &self,
        call: ToolCall,
        mut context: DispatchContext<'_>,
    ) -> (
        ToolResult,
        ModelFacingProjectionPlan,
        super::window_activity::ToolCallCorrelation,
    ) {
        let ToolCallContext {
            auth,
            window,
            transport,
            ..
        } = context.caller;
        let transport = transport.into();
        let mut result_projection = ModelFacingProjectionPlan::capture(&call);
        let context_guidance_profile = self.context_guidance_profile(&call, transport);
        let immediate_tool_name = call.tool_name();
        let immediate_expectation = context.recorder.expectation.clone();
        let mut correlation = super::window_activity::ToolCallCorrelation::default();
        // Edit usage telemetry retains only fixed safe classifications. For
        // apply_patch it captures the requested matching enum before the call is
        // moved, never the patch/path/content arguments.
        let mut edit_usage = edit_tool_telemetry::start_edit_tool_usage_for_call(&call);
        let mut result = self
            .govern_dispatch(call, &mut context, &mut correlation, &mut result_projection)
            .await;
        super::mcp_timing::normalize_result_timing(&mut result, transport, self.mcp_host_policy);
        correlation.failure_expectation_result =
            webcodex_workflow_session::tool_result_expectation_classification(
                result.success,
                &immediate_expectation,
                &result.output,
                result.error.as_deref(),
            )
            .map(str::to_string);
        // Early project/session/auth failures can return before the normal
        add_run_process_expectation_projection(
            immediate_tool_name,
            &immediate_expectation,
            &mut result,
        );
        // Early project/session/auth failures can return before the normal
        // resolved-project sidecar hook. Preserve the main ToolResult while still
        // answering the explicit sidecar request conservatively: static material
        // remains available, but project-scoped material must not guess a target.
        if !context.context_request.is_empty() && result.output.get("context_projection").is_none()
        {
            self.add_requested_context_projection_with_guidance(
                &mut result,
                ContextProjectionRequest {
                    requested: &context.context_request,
                    resolved_project: None,
                    auth,
                    capabilities: context.material_capabilities(),
                    guidance_profile: context_guidance_profile,
                    window,
                    instructions: None,
                },
            )
            .await;
        }
        if let Some(guard) = edit_usage.as_mut() {
            guard.finish_with_result(&result);
        }
        (result, result_projection, correlation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_execution_binding_preserves_specialized_selector_semantics() {
        let mut shell = ToolCall::RunShell {
            login: false,
            project: "~p7".to_string(),
            command: "true".to_string(),
            session_id: None,
            timeout_secs: None,
            sync_wait_secs: None,
            cwd: None,
            purpose: None,
            shell: None,
        };
        let (shell_project, shell_output) = canonical_execution_project_binding(&mut shell)
            .expect("legacy shell execution needs canonical binding");
        assert_eq!(shell_output, CanonicalProjectOutput::Canonical);
        shell_project.clone_from(&"agent:special:webcodex".to_string());
        assert_eq!(shell.project(), Some("agent:special:webcodex"));

        let mut hygiene = ToolCall::WorkspaceHygieneCheck {
            project: "~p7".to_string(),
            max_findings: None,
            include_tracked: None,
            session_id: None,
        };
        let (_, hygiene_output) = canonical_execution_project_binding(&mut hygiene).unwrap();
        assert_eq!(hygiene_output, CanonicalProjectOutput::Requested);

        let mut show_changes = ToolCall::ShowChanges {
            project: "~p7".to_string(),
            session_id: None,
            include_diff: Some(false),
            max_hunks: None,
            max_hunk_lines: None,
            session_event_limit: None,
        };
        let (_, show_changes_output) =
            canonical_execution_project_binding(&mut show_changes).unwrap();
        assert_eq!(show_changes_output, CanonicalProjectOutput::Requested);

        let mut work_on_project = ToolCall::WorkOnProject {
            project: "~p7".to_string(),
            client_id: None,
            path: None,
            mode: None,
            base_ref: None,
            instruction: "inspect".to_string(),
            guidance_profile: None,
            include_extension_catalog: false,
            session_id: None,
        };
        assert!(canonical_execution_project_binding(&mut work_on_project).is_none());
        assert_eq!(work_on_project.project(), Some("~p7"));

        let mut work_result = ToolCall::WorkResultState {
            collaboration: None,
            automatic: false,
            files: None,
            project: "demo".to_string(),
            session_id: Some("wc_sess_x".to_string()),
        };
        assert!(canonical_execution_project_binding(&mut work_result).is_none());
        assert_eq!(work_result.project(), Some("demo"));
    }
}

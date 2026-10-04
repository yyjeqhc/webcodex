//! Runtime tool dispatch and session/permission guard flow.

use super::context_projection::ContextProjectionRequest;
use super::edit_tool_telemetry;
use super::result_projection::ModelFacingProjectionPlan;
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
mod routing;
mod skills;
mod workflows;

use observations::add_run_process_expectation_projection;
pub(super) use preflight::decorate_structured_execution_prestart_denial;
use preflight::{canonical_execution_project_binding, CanonicalProjectOutput};

impl ToolRuntime {
    /// Main dispatch — call from MCP or Runtime HTTP handlers.
    ///
    /// This no-auth convenience defaults the caller context to `None`, which
    /// means Runner-backed tools are rejected (no owner can be proven). HTTP
    /// wrappers should prefer `dispatch_with_auth` so the depot `AuthContext`
    /// is forwarded. Tests use this wrapper for local-executor projects.
    #[cfg(test)]
    pub async fn dispatch(&self, call: ToolCall) -> ToolResult {
        self.dispatch_with_auth(call, None).await
    }

    /// Dispatch carrying the caller's auth context. Runner-backed tools enforce
    /// the owner boundary and capability requirements through
    /// `authorize_runner_tool`; local-executor tools are unaffected. Wrappers
    /// stay thin: they only forward the depot `AuthContext` here.
    pub fn dispatch_with_auth<'a>(
        &'a self,
        call: ToolCall,
        auth: Option<&'a AuthContext>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
        // The canonical dispatcher has grown into a large multi-specialist future. Keep that
        // state on the heap at the public dispatch boundary so direct callers do not need a
        // multi-megabyte stack frame merely to enter ToolRuntime.
        Box::pin(async move {
            self.dispatch_with_auth_transport(call, auth, sessions::SessionTransport::Api)
                .await
        })
    }

    pub(crate) async fn dispatch_with_auth_transport(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options(call, auth, transport)
            .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata(
            call,
            auth,
            transport,
            sessions::ToolCallRecorderMetadata::default(),
        )
        .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata_with_window(
            call,
            auth,
            transport,
            recorder_metadata,
            None,
        )
        .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata_with_window(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata_with_recording_mode(
            call,
            auth,
            transport,
            recorder_metadata,
            window,
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata_with_recording_mode(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
        inner_model_facing_recording: bool,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context(
            call,
            auth,
            transport,
            recorder_metadata,
            window,
            inner_model_facing_recording,
            Vec::new(),
            super::context_projection::ContextMaterialCapabilities::default(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
        inner_model_facing_recording: bool,
        context_request: Vec<String>,
        material_capabilities: super::context_projection::ContextMaterialCapabilities,
    ) -> ToolResult {
        let (mut result, projection, _) = self
            .dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context_with_result_projection(
            call,
            auth,
            transport,
            recorder_metadata,
            window,
            inner_model_facing_recording,
            context_request,
            material_capabilities,
            super::kernel::ToolProtocolCapabilities::default(),
            super::return_timing::ToolReturnTimingPolicy::unconstrained(),
        )
        .await;
        projection.project(&mut result);
        result
    }

    /// Kernel-only companion that returns the terminal model-facing projection
    /// plan after the same authoritative Project resolution used for execution.
    /// The returned ToolResult is still canonical with respect to domain-local
    /// budgeting and sparse projection so an outer recorder can consume it first.
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

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context_with_result_projection(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
        inner_model_facing_recording: bool,
        context_request: Vec<String>,
        material_capabilities: super::context_projection::ContextMaterialCapabilities,
        protocol_capabilities: super::kernel::ToolProtocolCapabilities,
        return_timing: super::return_timing::ToolReturnTimingPolicy,
    ) -> (
        ToolResult,
        ModelFacingProjectionPlan,
        super::window_activity::ToolCallCorrelation,
    ) {
        let mut result_projection = ModelFacingProjectionPlan::capture(&call);
        let context_guidance_profile = self.context_guidance_profile(&call, transport);
        let immediate_tool_name = call.tool_name();
        let immediate_expectation = recorder_metadata.expectation.clone();
        let mut correlation = super::window_activity::ToolCallCorrelation::default();
        // Edit usage telemetry retains only fixed safe classifications. For
        // apply_patch it captures the requested matching enum before the call is
        // moved, never the patch/path/content arguments.
        let mut edit_usage = edit_tool_telemetry::start_edit_tool_usage_for_call(&call);
        let mut result = self
            .dispatch_with_auth_transport_options_and_metadata_inner(
                call,
                auth,
                transport,
                recorder_metadata.clone(),
                window,
                inner_model_facing_recording,
                context_request.clone(),
                material_capabilities,
                protocol_capabilities,
                return_timing,
                &mut correlation,
                &mut result_projection,
            )
            .await;
        super::mcp_timing::normalize_result_timing(&mut result, transport, self.mcp_host_policy);
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
        if !context_request.is_empty() && result.output.get("context_projection").is_none() {
            self.add_requested_context_projection_with_guidance(
                &mut result,
                ContextProjectionRequest {
                    requested: &context_request,
                    resolved_project: None,
                    auth,
                    capabilities: material_capabilities,
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
            automatic: false,
            files: None,
            project: "demo".to_string(),
            session_id: Some("wc_sess_x".to_string()),
        };
        assert!(canonical_execution_project_binding(&mut work_result).is_none());
        assert_eq!(work_result.project(), Some("demo"));
    }
}

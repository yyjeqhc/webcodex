//! Post-record response finalization for the ordinary Kernel path.
//!
//! Authorization, dispatch, control transitions and Session recording finish in
//! the Kernel before entry. Capture privacy-safe canonical evidence first, consume
//! the request's projection plan once, then attach optional presentation sidecars.
//! This module never dispatches/retries the original operation or selects a Session.
use super::{ToolCallContext, ToolProtocolCapabilities};
use crate::tool_runtime::model_ergonomics_telemetry::ModelErgonomicsTimer;
use crate::tool_runtime::result_projection::ModelFacingProjectionPlan;
use crate::tool_runtime::sessions::ToolCallRecorderMetadata;
use crate::tool_runtime::tool_audit::session_log_result_for_tool;
use crate::tool_runtime::window_collaboration::ToolCallWindowReply;
use crate::tool_runtime::{ToolCallCorrelation, ToolResult, ToolRuntime};
use serde_json::Value;

/// Borrow only already-established request facts. No raw business arguments or
/// permission evaluator are accepted; a Window/recorder cannot retarget a call.
pub(super) struct PostRecordResponse<'a> {
    pub tool_name: &'a str,
    pub context: ToolCallContext<'a>,
    pub capabilities: ToolProtocolCapabilities,
    pub recorder: &'a ToolCallRecorderMetadata,
    pub correlation: &'a ToolCallCorrelation,
    pub business_session_id: Option<&'a str>,
    pub window_reply: Option<&'a ToolCallWindowReply>,
}

/// Keep canonical audit evidence separate from the model result by construction.
/// Neither this internal envelope nor its context is a public wire contract.
pub(super) struct PostRecordResult {
    pub result: ToolResult,
    pub canonical_audit_output: Option<Value>,
}

impl PostRecordResponse<'_> {
    pub(super) async fn finish(
        self,
        runtime: &ToolRuntime,
        mut result: ToolResult,
        plan: ModelFacingProjectionPlan,
        telemetry: Option<&mut ModelErgonomicsTimer>,
    ) -> PostRecordResult {
        if let Some(telemetry) = telemetry {
            telemetry.capture_canonical_result(&result);
        }
        crate::tool_request_trace::capture_execution_evidence(self.tool_name, &result.output);
        let canonical_audit_output = canonical_audit_output(self.tool_name, &result.output);
        plan.project(&mut result);
        self.add_recorder_gap_hint(&mut result);
        if self.tool_name == "read_tool_manifest" {
            crate::tool_runtime::surface::sparsify_tool_manifest_model_result(&mut result);
        }
        crate::tool_runtime::result_projection::sparsify_failure_model_result_metadata(
            self.tool_name,
            &mut result,
        );
        if !result.success
            && self.tool_name != "read_tool_trace"
            && self.capabilities.trace_diagnostics
            && self
                .context
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
        crate::tool_runtime::result_projection::sparsify_success_model_result_metadata(
            self.tool_name,
            &mut result,
        );
        // Optional continuity guidance may not expand an already full response.
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
        let enrichment_deadline = crate::tool_runtime::optional_enrichment::deadline();
        if crate::tool_runtime::tool_definition::is_model_visible_tool_name(self.tool_name) {
            let reply_started = std::time::Instant::now();
            runtime.add_window_model_reply_sidecar(
                &mut result,
                self.context.auth,
                self.context.window,
                self.window_reply,
            );
            crate::tool_request_trace::record_phase_latency(
                "explicit_window_reply",
                reply_started,
                if self.window_reply.is_some() {
                    "required"
                } else {
                    "not_requested"
                },
            );
            let peer_project = self
                .correlation
                .resolved_project
                .as_deref()
                .or(self.recorder.recording_session_project.as_deref());
            if self.tool_name != "present_work_result" {
                runtime.add_window_operator_projection_until(
                    &mut result,
                    self.context.auth,
                    self.context.window,
                    &self.recorder.ack_session_message_ids,
                    enrichment_deadline,
                );
            }
            runtime.add_peer_collaboration_projection_until(
                &mut result,
                self.context.auth,
                self.context.window,
                peer_project,
                &self.recorder.ack_session_message_ids,
                enrichment_deadline,
            );
        }
        if self.tool_name == "observe_jobs" {
            crate::tool_runtime::observe_jobs::sparsify_observe_jobs_model_result(&mut result);
        }
        runtime
            .add_passive_job_attention_until(
                &mut result,
                self.tool_name,
                self.correlation.resolved_project.as_deref(),
                self.correlation.business_session_id.as_deref(),
                self.context.window,
                self.context.auth,
                enrichment_deadline,
            )
            .await;
        PostRecordResult {
            result,
            canonical_audit_output,
        }
    }

    fn add_recorder_gap_hint(&self, result: &mut ToolResult) {
        if let (Some(session_id), Some(project)) = (
            self.correlation.recorder_gap_session_id.as_deref(),
            self.correlation.resolved_project.as_deref(),
        ) {
            // Correlation is diagnostic only; an exact business target already
            // supplied by this call needs no redundant recorder guidance.
            if self.business_session_id != Some(session_id) {
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
    }
}

fn canonical_audit_output(tool_name: &str, output: &Value) -> Option<Value> {
    match tool_name {
        "project_build" | "run_process" | "run_script" | "run_skill_resource" | "run_shell"
        | "project_validate" | "cargo_fmt" | "cargo_check" | "cargo_test" | "go_test" => Some(
            crate::tool_runtime::tool_audit::canonical_execution_audit_result_for_tool(
                tool_name, output,
            ),
        ),
        "edit_project_files" | "read_files" | "search_project_texts" | "wait_for_job_readiness" => {
            Some(session_log_result_for_tool(tool_name, output))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;

//! General task output verification reuses canonical artifact observation.
use super::kernel::{HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport};
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde_json::{json, Value};
use webcodex_core::task_outputs::{
    valid_task_output_path, TaskOutput, TaskOutputStatus, TaskOutputs, MAX_TASK_OUTPUTS,
};

pub(crate) fn validate_task_output_paths(paths: &[String]) -> bool {
    let mut seen = std::collections::HashSet::new();
    paths.len() <= MAX_TASK_OUTPUTS
        && paths
            .iter()
            .all(|path| valid_task_output_path(path) && seen.insert(path))
}

pub(crate) fn task_output_observation(path: String, result: &ToolResult) -> TaskOutput {
    let output = &result.output;
    let status = if result.success
        && output["exists"] == false
        && output.get("path").and_then(Value::as_str) == Some(path.as_str())
    {
        TaskOutputStatus::Missing
    } else if result.success && output["exists"] == true {
        TaskOutputStatus::Verified
    } else {
        TaskOutputStatus::Unavailable
    };
    let mut item = TaskOutput {
        path,
        status,
        file_bytes: None,
        sha256: None,
        mime_type: None,
    };
    if item.status == TaskOutputStatus::Verified {
        item.file_bytes = output.get("bytes").and_then(Value::as_u64);
        item.sha256 = output
            .get("sha256")
            .and_then(Value::as_str)
            .map(str::to_string);
        item.mime_type = match output.get("mime_type") {
            // Unknown MIME is valid for regular artifacts. Use the same safe
            // presentation fallback as artifact export; bytes and SHA still
            // come exclusively from the Runner observation.
            None | Some(Value::Null) => Some(
                webcodex_core::artifact_policy::export_presentation_mime(&item.path, None),
            ),
            Some(Value::String(mime)) => Some(mime.clone()),
            _ => None,
        };
        let probe = TaskOutputs {
            items: vec![item.clone()],
            verified_count: 1,
            missing_count: 0,
            unavailable_count: 0,
            observed_at: 0,
        };
        if !probe.valid() || output.get("path").and_then(Value::as_str) != Some(item.path.as_str())
        {
            item.status = TaskOutputStatus::Unavailable;
            item.file_bytes = None;
            item.sha256 = None;
            item.mime_type = None;
        }
    }
    item
}

pub(crate) fn attach_task_outputs(output: &mut Value, outputs: &TaskOutputs) {
    output["task_outputs"] = json!(outputs);
    if outputs.missing_count + outputs.unavailable_count > 0 {
        let outcome = &mut output["task_outcome"];
        outcome["status"] = json!("fail");
        outcome["blocking"] = json!(true);
        let reasons = outcome
            .as_object_mut()
            .expect("closeout outcome object")
            .entry("blocking_reasons")
            .or_insert_with(|| json!([]));
        if let Some(reasons) = reasons.as_array_mut() {
            reasons.push(json!("task_outputs_unverified"));
        }
        if let Some(blockers) = output["hard_blockers"].as_array_mut() {
            blockers.push(json!("task_outputs_unverified"));
        }
        if let Some(actions) = output["suggested_next_actions"].as_array_mut() {
            actions.push(json!("Inspect missing/unavailable task outputs, correct the task, verify its content/counts, then finish again with the exact output paths."));
        }
    }
}

impl ToolRuntime {
    pub(crate) async fn observe_task_outputs(
        &self,
        project: &str,
        session_id: &str,
        outputs: Vec<String>,
        auth: Option<&AuthContext>,
    ) -> TaskOutputs {
        let mut items = Vec::with_capacity(outputs.len());
        for path in outputs {
            // The canonical kernel owns scope, Session, Project and Runner checks
            // and records the observation in the exact business Session.
            let outcome = self.call_tool_with_context(ToolCallRequest {
                tool_name: "read_project_artifact_metadata".to_string(),
                arguments: json!({"project": project, "session_id": session_id, "path": path, "allow_missing": true}),
            }, ToolCallContext {
                transport: ToolTransport::Api, session_id: None, auth, window: None,
                record_oauth_scope_denials: false, host_file_import_trust: HostFileImportTrust::Untrusted,
            }).await;
            let observed = outcome
                .result
                .unwrap_or_else(|| ToolResult::err("Task output observation unavailable"));
            items.push(task_output_observation(path, &observed));
        }
        TaskOutputs {
            verified_count: items
                .iter()
                .filter(|item| item.status == TaskOutputStatus::Verified)
                .count(),
            missing_count: items
                .iter()
                .filter(|item| item.status == TaskOutputStatus::Missing)
                .count(),
            unavailable_count: items
                .iter()
                .filter(|item| item.status == TaskOutputStatus::Unavailable)
                .count(),
            items,
            observed_at: chrono::Utc::now().timestamp(),
        }
    }
}

pub(crate) fn retained_task_outputs(
    summary: &webcodex_workflow_session::SessionSummary,
) -> Option<Value> {
    // The latest finish invalidates an earlier manifest even if it omitted outputs.
    // These are explicitly time-stamped observations, not current filesystem truth.
    let event = summary.events.iter().rev().find(|event| {
        event.tool_name == "finish_coding_task"
            && event.kind == "tool_call_finished"
            && event.session_id == summary.session_id
            && match event.logical_invocation_role.as_deref() {
                Some("business") => true,
                Some(_) => false,
                // Direct/internal dispatch has no kernel correlation role.
                // Its explicit target is retained on the matching start event.
                None => {
                    event.call_id.is_some()
                        && summary.events.iter().any(|start| {
                            start.kind == "tool_call_started"
                                && start.tool_name == event.tool_name
                                && start.call_id == event.call_id
                                && start
                                    .input_summary
                                    .as_ref()
                                    .and_then(|input| input.get("session_id"))
                                    .and_then(Value::as_str)
                                    == Some(summary.session_id.as_str())
                        })
                }
            }
    })?;
    let value = event.context_result_summary.as_ref()?.get("task_outputs")?;
    let snapshot: TaskOutputs = serde_json::from_value(value.clone()).ok()?;
    snapshot.valid().then(|| json!(snapshot))
}

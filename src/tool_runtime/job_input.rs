//! Explicit pipe interaction on the existing public Job and authorization path.
use super::{ExecutionPurpose, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde_json::json;
use std::time::Duration;
use webcodex_core::job_input::{validate_input, InputWriteState, JobInputReceipt};
use webcodex_core::runner_protocol::{ShellJobOpRequest, ShellProcessArgv};
use webcodex_runner_registry::{ShellJobStartMetadata, ShellJobVisibility, StructuredJobExecution};

fn valid_receipt(receipt: &JobInputReceipt, id: &str, bytes: usize, close: bool) -> bool {
    if receipt.input_id != id || receipt.bytes_written > bytes {
        return false;
    }
    match receipt.state {
        InputWriteState::Pending => receipt.bytes_written == 0 && !receipt.stdin_closed,
        InputWriteState::Written => {
            !close && receipt.bytes_written == bytes && !receipt.stdin_closed
        }
        InputWriteState::Closed => close && receipt.bytes_written == bytes && receipt.stdin_closed,
        InputWriteState::OutcomeUnknown => true,
    }
}

fn rejected(kind: &str) -> ToolResult {
    ToolResult::err_with_output(
        kind,
        json!({"error_kind":kind,"execution_state":"not_started","state_changed":false,"command_started":false,"command_completed":false}),
    )
}
fn uncertain(job_id: &str, input_id: &str) -> ToolResult {
    ToolResult::err_with_output("input delivery outcome is unknown; reconcile the same input identity, never resend under a new id",
        json!({"error_kind":"job_input_outcome_unknown","execution_state":"outcome_unknown",
            "job_id":job_id,"input_id":input_id,"state":"outcome_unknown"}))
}
impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_interactive_process(
        &self,
        project: String,
        executable: String,
        args: Vec<String>,
        stdin: Option<String>,
        timeout: Option<u64>,
        cwd: Option<String>,
        purpose: Option<ExecutionPurpose>,
        ssh_resource: Option<&str>,
        session_id: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if stdin.is_some() {
            return rejected("interactive_process_initial_stdin_unsupported");
        }
        if ssh_resource.is_some() {
            return rejected("interactive_process_ssh_resource_unsupported");
        }
        let process = ShellProcessArgv { executable, args };
        if super::process::validate_process_input(&process, None, cwd.as_deref()).is_err() {
            return rejected("interactive_process_arguments_invalid");
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(v) => v,
            Err(error) => return error.into_tool_result(),
        };
        let target = match self.resolve_project(&resolved.resolved_id).await {
            Ok(v) => v,
            Err(_) => return rejected("interactive_process_project_unavailable"),
        };
        let effective_cwd = match super::helpers::resolve_runner_cwd(&target, cwd.as_deref()) {
            Ok(v) => v,
            Err(_) => return rejected("interactive_process_cwd_invalid"),
        };
        let budget = match super::structured_execution::StructuredExecutionBudget::resolve_process(
            timeout,
        ) {
            Ok(v) => v,
            Err(_) => return rejected("interactive_process_timeout_invalid"),
        };
        let job = self
            .runner_registry
            .start_job_with_metadata_for_access(
                ShellJobOpRequest {
                    op: "start".into(),
                    client_id: Some(target.client_id),
                    cwd: Some(effective_cwd),
                    command: Some(String::new()),
                    timeout_secs: Some(budget.effective_timeout_secs),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                    login: false,
                },
                "tool_runtime".into(),
                ShellJobStartMetadata {
                    project_id: Some(resolved.resolved_id),
                    session_id,
                    project_cwd: cwd,
                    purpose: purpose.map(|p| p.as_str().into()),
                    shell: Some("direct_argv".into()),
                    visibility: ShellJobVisibility::Public,
                    structured_execution: Some(StructuredJobExecution::InteractiveProcess(process)),
                    // Later interactive input is not a pre-known validation recipe or a
                    // command hash covering the full interaction. Never mint test evidence.
                    ..Default::default()
                },
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
                None,
            )
            .await;
        match job {
            Err(error) => ToolResult::err_with_output(
                error,
                json!({"error_kind":"interactive_process_not_admitted","execution_state":"not_started","state_changed":false,"command_started":false,"command_completed":false}),
            ),
            Ok(job) => ToolResult::ok(
                json!({"execution_state":"pending","job_id":job.job_id,"job_status":job.status,
                "interactive":true,"input_mode":"pipe","effective_timeout_secs":budget.effective_timeout_secs,
                "continuation":super::jobs::observe_job_continuation(&job.job_id,job.observation_token.as_deref())}),
            ),
        }
    }

    pub(crate) async fn job_write_input(
        &self,
        project: String,
        job_id: String,
        input_id: String,
        data: String,
        close: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(kind) = validate_input(&input_id, &data, close) {
            return rejected(&kind);
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(v) => v,
            Err(error) => return error.into_tool_result(),
        };
        let (_request_id, rx) = match self
            .runner_registry
            .enqueue_job_input(
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
                &resolved.resolved_id,
                &job_id,
                input_id.clone(),
                data.clone(),
                close,
            )
            .await
        {
            Ok(v) => v,
            Err(kind) => return rejected(&kind),
        };
        // This is a bounded input receipt, not a new validation Job. Timeout
        // never re-enqueues input or terminates the still-running process.
        let response = match tokio::time::timeout(Duration::from_secs(3), rx).await {
            Ok(Ok(v)) => v,
            _ => return uncertain(&job_id, &input_id),
        };
        if !response.success {
            let kind = response.error.as_deref().unwrap_or("");
            if matches!(
                kind,
                "job_input_conflict"
                    | "job_input_closed"
                    | "job_input_busy"
                    | "job_input_capacity"
                    | "job_input_not_ready"
                    | "job_input_unavailable"
                    | "job_input_instance_changed"
                    | "job_input_target_mismatch"
                    | "job_input_disabled"
                    | "job_input_capability_unavailable"
            ) {
                return rejected(kind);
            }
            return uncertain(&job_id, &input_id);
        }
        let receipt = match response
            .stdout
            .as_deref()
            .filter(|s| s.len() <= 2048)
            .and_then(|s| serde_json::from_str::<JobInputReceipt>(s).ok())
        {
            Some(v) if valid_receipt(&v, &input_id, data.len(), close) => v,
            _ => return uncertain(&job_id, &input_id),
        };
        let mut output = json!({"job_id":job_id,"input_id":input_id,"state":receipt.state,
            "bytes_written":receipt.bytes_written,"stdin_closed":receipt.stdin_closed,
            "execution_state":if receipt.state==InputWriteState::OutcomeUnknown {"outcome_unknown"} else {"completed"}});
        if receipt.state == InputWriteState::OutcomeUnknown {
            output["error_kind"] = json!("job_input_outcome_unknown");
            ToolResult::err_with_output(
                "pipe delivery incomplete or interrupted; do not resend with a new id",
                output,
            )
        } else {
            ToolResult::ok(output)
        }
    }
}

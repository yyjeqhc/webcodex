//! Explicit-file formatter gateway. Native Job admission owns every effect.
use super::helpers::{project_relative_runner_cwd, resolve_runner_cwd};
use super::structured_execution::{
    await_hidden_structured_job, HiddenStructuredJobWait, StructuredExecutionBudget,
};
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::runner_http::{JobStartContext, JobStartExecution, JobStartRequest, ShellJobVisibility};
use crate::runner_protocol::{ShellCommandExecutionState, ShellJobInfo};
use serde_json::json;
use std::time::Duration;
use webcodex_core::project_format::*;
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;

fn rejected(code: &str) -> ToolResult {
    ToolResult::err_with_output(
        "Project formatting was not admitted.",
        json!({
            "failure_kind": code, "execution_state": "not_started", "state_changed": false,
        }),
    )
}

pub(super) fn terminal_result(job: &ShellJobInfo) -> ToolResult {
    let report = job.format_mutation.unwrap_or_default();
    let known = report.state_changed();
    let success = job.status == "completed"
        && job.exit_code == Some(0)
        && job.command_execution_state == Some(ShellCommandExecutionState::Completed)
        && job.error.is_none()
        && known.is_some();
    let mut output = json!({"state_changed": known});
    if success {
        return ToolResult::ok(output);
    }
    let code = project_format_safe_failure_kind(job).unwrap_or("format_execution_unknown");
    output["failure_kind"] = json!(code);
    output["execution_state"] = json!("outcome_unknown");
    output["job_id"] = json!(job.job_id);
    output["continuation"] =
        super::jobs::observe_job_continuation(&job.job_id, job.observation_token.as_deref());
    ToolResult::err_with_output("Formatting did not produce a complete verified mutation receipt. Observe the original Job and source before another mutation.", output)
}

pub(crate) fn valid_failure_code(code: &str) -> bool {
    code.len() <= 80
        && (code.starts_with("format_")
            || code == "capability_unavailable"
            || code == "invalid_arguments"
            || code == "permission_denied"
            || code == "unknown_project"
            || code == "invalid_project_path")
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

pub(crate) fn project_format_safe_failure_kind(job: &ShellJobInfo) -> Option<&str> {
    let report = job.format_mutation.unwrap_or_default();
    let known = report.state_changed();
    let success = job.status == "completed"
        && job.exit_code == Some(0)
        && job.command_execution_state == Some(ShellCommandExecutionState::Completed)
        && job.error.is_none()
        && known.is_some();
    let active = job.status == "recovering"
        || job.recovery_state.as_deref() == Some("recovering")
        || RunnerJobLifecycle::from_wire(&job.status).is_ok_and(RunnerJobLifecycle::is_active);
    if success {
        None
    } else if active && job.error.is_none() {
        None
    } else {
        Some(
            job.error
                .as_deref()
                .filter(|code| valid_failure_code(code))
                .unwrap_or("format_execution_unknown"),
        )
    }
}

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn project_format(
        &self,
        project: String,
        session_id: Option<String>,
        cwd: Option<String>,
        adapter: Option<ProjectFormatAdapter>,
        files: Vec<String>,
        timeout_secs: Option<u64>,
        handoff_max_secs: Option<u64>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let timeout = timeout_secs.unwrap_or(60);
        if !(1..=PROJECT_FORMAT_TIMEOUT_MAX_SECS).contains(&timeout) {
            return rejected("invalid_arguments");
        }
        let budget = match StructuredExecutionBudget::resolve_process_with_sync_wait(
            Some(timeout),
            handoff_max_secs,
        ) {
            Ok(budget) => budget,
            Err(_) => return rejected("invalid_arguments"),
        };
        if ssh_resource.is_some() {
            return rejected("format_resource_unavailable");
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(_) => return rejected("format_project_unavailable"),
        };
        let Some((_, project_id)) = resolved
            .resolved_id
            .strip_prefix("agent:")
            .and_then(|v| v.split_once(':'))
        else {
            return rejected("format_project_unavailable");
        };
        let request = ProjectFormatRequest {
            project_id: project_id.into(),
            cwd,
            adapter: adapter.unwrap_or_default(),
            files,
        };
        if request.validate().is_err() {
            return rejected("invalid_arguments");
        }
        let access = crate::runner_http::runner_access_from_auth(auth);
        let (request_id, runner_instance_id, rx) = match self
            .runner_registry
            .enqueue_project_format_plan(
                resolved.config.client_id.clone(),
                request.clone(),
                access.as_ref(),
            )
            .await
        {
            Ok(value) => value,
            Err(error) => {
                let code = if valid_failure_code(&error) {
                    &error
                } else if error.starts_with("capability_unavailable:") {
                    "capability_unavailable"
                } else {
                    "format_planning_unavailable"
                };
                return rejected(code);
            }
        };
        let response = match tokio::time::timeout(Duration::from_secs(32), rx).await {
            Ok(Ok(response)) => response,
            _ => {
                self.runner_registry.cancel_request(&request_id).await;
                return rejected("format_planning_unavailable");
            }
        };
        if let Some(err) = response.error.as_deref() {
            let code = if err.contains("replaced") {
                "format_runner_replaced"
            } else {
                "format_plan_invalid"
            };
            return rejected(code);
        }
        if response
            .stdout
            .as_ref()
            .is_none_or(|s| s.len() > PROJECT_FORMAT_PLAN_MAX_BYTES + 128)
        {
            return rejected("format_plan_invalid");
        }
        let plan = match serde_json::from_str::<ProjectFormatPlanningResult>(
            response.stdout.as_deref().unwrap_or(""),
        ) {
            Ok(ProjectFormatPlanningResult::Ready { plan })
                if plan.is_valid() && plan.request == request =>
            {
                plan
            }
            Ok(ProjectFormatPlanningResult::Unavailable { code }) if valid_failure_code(&code) => {
                return rejected(&code)
            }
            _ => return rejected("format_plan_invalid"),
        };
        let effective_cwd = match resolve_runner_cwd(&resolved.config, Some(&plan.recipe_root)) {
            Ok(cwd) => cwd,
            Err(_) => return rejected("format_path_invalid"),
        };
        let project_cwd = project_relative_runner_cwd(&resolved.config, &effective_cwd).ok();
        let job = match self
            .runner_registry
            .start_job(
                JobStartRequest {
                    client_id: resolved.config.client_id.clone(),
                    cwd: Some(effective_cwd),
                    timeout_secs: timeout,
                    execution: JobStartExecution::Format {
                        plan,
                        runner_instance_id,
                    },
                    context: JobStartContext {
                        project_id: Some(resolved.resolved_id),
                        session_id: session_id,
                        project_cwd: project_cwd,
                        purpose: Some("format".into()),
                        shell: Some("direct_argv".into()),
                        visibility: ShellJobVisibility::Public,
                    },
                    codex: None,
                },
                "tool_runtime".to_string(),
                access.as_ref(),
                None,
            )
            .await
        {
            Ok(job) => job,
            Err(error) => {
                let code = error.reason_code().unwrap_or_else(|| {
                    if error.failure_kind() == "capability_unavailable"
                        && !error.is_async_jobs_unavailable()
                    {
                        "capability_unavailable"
                    } else {
                        "format_admission_unavailable"
                    }
                });
                return rejected(code);
            }
        };
        match await_hidden_structured_job(self.runner_registry.clone(), job.job_id.clone(),
            self.structured_execution_sync_wait.min(Duration::from_secs(budget.sync_wait_secs)), auth.cloned()).await {
            Ok(HiddenStructuredJobWait::Terminal { job, .. }) => terminal_result(&job),
            Ok(HiddenStructuredJobWait::Continued { observation, .. }) => ToolResult::ok(json!({
                "execution_state": "pending",
                "state_changed": null,
                "job_id": observation.job.job_id,
                "continuation": super::jobs::observe_job_continuation(&observation.job.job_id, observation.job.observation_token.as_deref()),
            })),
            Err(_) => ToolResult::err_with_output("Formatting delivery is uncertain; observe the original Job before another mutation.", json!({
                "state_changed": null, "failure_kind": "format_execution_unknown", "execution_state": "outcome_unknown",
                "job_id": job.job_id,
                "continuation": super::jobs::observe_job_continuation(&job.job_id, job.observation_token.as_deref()),
            })),
        }
    }
}

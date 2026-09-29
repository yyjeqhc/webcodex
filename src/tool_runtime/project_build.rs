use serde_json::json;
use std::time::Duration;

use super::helpers::{command_rejected_message, project_relative_runner_cwd, resolve_runner_cwd};
use super::process::{
    add_structured_continuation_facts, classify_process_failure, process_tool_failure_result,
    terminal_structured_job_result,
};
use super::structured_execution::{
    await_hidden_structured_job, finalize_hidden_terminal_projection, HiddenStructuredJobWait,
    StructuredExecutionBudget,
};
use super::{ExecutionPurpose, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::runner_http::{
    process_preview, ShellJobStartMetadata, ShellJobVisibility, StructuredJobExecution,
};
use crate::runner_protocol::{ShellCommandExecutionState, ShellJobOpRequest};
use webcodex_core::project_build::{
    ProjectBuildAdapter, ProjectBuildPlanningResult, ProjectBuildRequest, ProjectBuildScope,
};

const PROJECT_BUILD_DEFAULT_TIMEOUT_SECS: u64 = 1800;

fn decorate(output: &mut serde_json::Value, backend: &str, summary: &str, cwd: &str) {
    output["execution_source"] = json!("project_build");
    output["purpose"] = json!(ExecutionPurpose::Build.as_str());
    output["process_summary"] = json!(summary);
    output["cwd"] = json!(cwd);
    output["executor"] = json!("agent");
    output["backend"] = json!(backend);
}

impl ToolRuntime {
    pub(crate) async fn project_build(
        &self,
        project: String,
        session_id: Option<String>,
        cwd: Option<String>,
        adapter: Option<ProjectBuildAdapter>,
        scope: Option<ProjectBuildScope>,
        timeout_secs: Option<u64>,
        handoff_max_secs: Option<u64>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let budget = match StructuredExecutionBudget::resolve_process_with_sync_wait(
            Some(timeout_secs.unwrap_or(PROJECT_BUILD_DEFAULT_TIMEOUT_SECS)),
            handoff_max_secs,
        ) {
            Ok(budget) => budget,
            Err(error) => {
                return process_tool_failure_result(
                    command_rejected_message(
                        format!("project_build {error}"),
                        "pass a positive timeout_secs or omit it for the default build budget.",
                    ),
                    "invalid_arguments",
                    ShellCommandExecutionState::NotStarted,
                )
            }
        };
        if ssh_resource.is_some() {
            return process_tool_failure_result(
                command_rejected_message(
                    "project_build does not support named Session SSH resources",
                    "run the build against the Runner-host project.",
                ),
                "unsupported_resource",
                ShellCommandExecutionState::NotStarted,
            );
        }

        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(error) => {
                return process_tool_failure_result(
                    command_rejected_message(
                        error.to_message(),
                        "verify the exact Runner project and retry.",
                    ),
                    "agent_offline",
                    ShellCommandExecutionState::NotStarted,
                )
            }
        };
        let Some((_, project_id)) = resolved
            .resolved_id
            .strip_prefix("agent:")
            .and_then(|value| value.split_once(':'))
        else {
            return process_tool_failure_result(
                command_rejected_message(
                    "project_build requires an exact Runner Project",
                    "select a registered Runner project.",
                ),
                "invalid_arguments",
                ShellCommandExecutionState::NotStarted,
            );
        };
        let request = ProjectBuildRequest {
            project_id: project_id.to_string(),
            cwd,
            adapter: adapter.unwrap_or_default(),
            scope,
        };
        if let Err(error) = request.validate() {
            return process_tool_failure_result(
                command_rejected_message(error, "correct the project build request and retry."),
                "invalid_arguments",
                ShellCommandExecutionState::NotStarted,
            );
        }

        let access = crate::runner_http::runner_access_from_auth(auth);
        let (request_id, rx) = match self
            .runner_registry
            .enqueue_project_build_plan(
                resolved.config.client_id.clone(),
                request.clone(),
                access.as_ref(),
            )
            .await
        {
            Ok(value) => value,
            Err(error) => {
                return process_tool_failure_result(
                    command_rejected_message(
                        &error,
                        "upgrade or select a Runner that advertises project_build_v1.",
                    ),
                    classify_process_failure(&error),
                    ShellCommandExecutionState::NotStarted,
                )
            }
        };
        let response = match tokio::time::timeout(Duration::from_secs(32), rx).await {
            Ok(Ok(response)) => response,
            _ => {
                self.runner_registry.cancel_request(&request_id).await;
                return process_tool_failure_result(
                    command_rejected_message(
                        "project build planning unavailable",
                        "retry only after confirming no build execution was admitted.",
                    ),
                    "runtime_error",
                    ShellCommandExecutionState::NotStarted,
                );
            }
        };
        if response.error.is_some() {
            return process_tool_failure_result(
                "Runner project build planning failed",
                "runtime_error",
                ShellCommandExecutionState::NotStarted,
            );
        }
        let plan = match serde_json::from_str::<ProjectBuildPlanningResult>(
            response.stdout.as_deref().unwrap_or(""),
        ) {
            Ok(ProjectBuildPlanningResult::Ready { plan }) => plan,
            Ok(ProjectBuildPlanningResult::Unavailable {
                code,
                detected_backend,
            }) => {
                return ToolResult::err_with_output(
                    "Project build unavailable for the requested project recipe.",
                    json!({
                        "execution_source": "project_build",
                        "execution_state": "not_started",
                        "command_started": false,
                        "command_completed": false,
                        "command_ok": false,
                        "failure_kind": code,
                        "detected_backend": detected_backend,
                        "tool_failure": true,
                    }),
                )
            }
            Err(_) => {
                return process_tool_failure_result(
                    "invalid Runner project build plan; upgrade Server and Runner together",
                    "runtime_error",
                    ShellCommandExecutionState::NotStarted,
                )
            }
        };
        if !plan.is_valid() || plan.provenance.request != request {
            return process_tool_failure_result(
                "invalid Runner project build plan",
                "runtime_error",
                ShellCommandExecutionState::NotStarted,
            );
        }

        let backend = plan.provenance.backend.clone();
        let recipe_root = plan.provenance.recipe_root.clone();
        let summary = process_preview(
            &plan.process.executable,
            plan.process.args.iter().map(String::as_str),
        );
        let effective_cwd = match resolve_runner_cwd(&resolved.config, Some(&recipe_root)) {
            Ok(cwd) => cwd,
            Err(error) => {
                return process_tool_failure_result(
                    command_rejected_message(error, "resolve project_build again."),
                    "permission_denied",
                    ShellCommandExecutionState::NotStarted,
                )
            }
        };
        let resolved_cwd = project_relative_runner_cwd(&resolved.config, &effective_cwd)
            .unwrap_or_else(|_| recipe_root.clone());

        let job = self
            .runner_registry
            .start_job_with_metadata_for_access(
                ShellJobOpRequest {
                    login: false,
                    op: "start".to_string(),
                    client_id: Some(resolved.config.client_id.clone()),
                    cwd: Some(effective_cwd),
                    command: Some(String::new()),
                    timeout_secs: Some(budget.effective_timeout_secs),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tool_runtime".to_string(),
                ShellJobStartMetadata {
                    project_id: Some(resolved.resolved_id.clone()),
                    session_id,
                    project_cwd: Some(resolved_cwd.clone()),
                    purpose: Some(ExecutionPurpose::Build.as_str().to_string()),
                    shell: Some("direct_argv".to_string()),
                    visibility: ShellJobVisibility::HiddenUntilHandoff,
                    structured_execution: Some(StructuredJobExecution::ProjectBuild(plan)),
                    ..Default::default()
                },
                access.as_ref(),
                None,
            )
            .await;
        let job = match job {
            Ok(job) => job,
            Err(error) => {
                let mut result = process_tool_failure_result(
                    command_rejected_message(
                        &error,
                        "confirm Runner project_build_v1 and structured Job support, then retry only if no build started.",
                    ),
                    classify_process_failure(&error),
                    ShellCommandExecutionState::NotStarted,
                );
                decorate(&mut result.output, &backend, &summary, &resolved_cwd);
                add_structured_continuation_facts(
                    &mut result,
                    budget.effective_timeout_secs,
                    budget.sync_wait_secs,
                    true,
                );
                return result;
            }
        };
        let wait = self
            .structured_execution_sync_wait
            .min(Duration::from_secs(budget.sync_wait_secs));
        let handoff = await_hidden_structured_job(
            self.runner_registry.clone(),
            job.job_id.clone(),
            wait,
            auth.cloned(),
        )
        .await;
        let mut result = match handoff {
            Ok(HiddenStructuredJobWait::Terminal {
                job,
                stdout,
                stderr,
            }) => {
                let mut result = terminal_structured_job_result(
                    &job,
                    stdout,
                    stderr,
                    budget.effective_timeout_secs,
                );
                finalize_hidden_terminal_projection(
                    self.runner_registry.as_ref(),
                    auth,
                    &job,
                    &mut result,
                    budget,
                )
                .await;
                result
            }
            Ok(HiddenStructuredJobWait::Continued {
                observation,
                execution_state,
                command_started,
            }) => {
                let detected_summary =
                    crate::tool_runtime::jobs::detected_job_summary_with_activity(
                        Some(&summary),
                        Some(ExecutionPurpose::Build.as_str()),
                        &observation.job.status,
                        observation.job.exit_code.map(i64::from),
                        &observation.stdout_tail,
                        &observation.stderr_tail,
                        observation.stdout_truncated || observation.stderr_truncated,
                        observation.job.activity.as_ref(),
                    );
                ToolResult::ok(json!({
                    "execution_state": execution_state,
                    "command_started": command_started,
                    "command_completed": false,
                    "command_ok": false,
                    "exit_code": null,
                    "failure_kind": null,
                    "tool_failure": false,
                    "promoted_to_job": true,
                    "terminal": false,
                    "job_id": observation.job.job_id,
                    "job_status": observation.job.status,
                    "observation_token": observation.job.observation_token,
                    "continuation_semantics": crate::tool_runtime::jobs::job_observation_continuation_semantics(),
                    "activity": observation.job.activity,
                    "effective_timeout_secs": budget.effective_timeout_secs,
                    "sync_wait_secs": budget.sync_wait_secs,
                    "async_handoff_available": true,
                    "stdout_tail": observation.stdout_tail,
                    "stderr_tail": observation.stderr_tail,
                    "stdout_lines": observation.stdout_lines,
                    "stderr_lines": observation.stderr_lines,
                    "stdout_truncated": observation.stdout_truncated,
                    "stderr_truncated": observation.stderr_truncated,
                    "detected_summary": detected_summary,
                    "continuation": crate::tool_runtime::jobs::observe_job_continuation(
                        &observation.job.job_id,
                        observation.job.observation_token.as_deref(),
                    ),
                }))
            }
            Err(failure) => failure.into_tool_result(&project, budget),
        };
        if result.output["promoted_to_job"] != json!(true) {
            add_structured_continuation_facts(
                &mut result,
                budget.effective_timeout_secs,
                budget.sync_wait_secs,
                true,
            );
        }
        decorate(&mut result.output, &backend, &summary, &resolved_cwd);
        result
    }
}

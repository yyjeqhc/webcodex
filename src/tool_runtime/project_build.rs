use serde_json::json;
use std::time::Duration;

use super::helpers::{command_rejected_message, project_relative_runner_cwd, resolve_runner_cwd};
use super::process::{
    add_structured_continuation_facts, classify_process_failure,
    execution_admission_failure_result, process_tool_failure_result,
    terminal_structured_job_result,
};
use super::structured_execution::{await_structured_job_result, StructuredExecutionBudget};
use super::{ExecutionPurpose, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::runner_http::{
    process_preview, JobStartContext, JobStartExecution, JobStartRequest, ShellJobVisibility,
};
use crate::runner_protocol::{ShellCommandExecutionState, ShellJobInfo};
use webcodex_core::project_build::{
    ProjectBuildAdapter, ProjectBuildPlanningResult, ProjectBuildRequest, ProjectBuildScope,
};

const PROJECT_BUILD_DEFAULT_TIMEOUT_SECS: u64 = 1800;

/// Artifact observation errors are not native process failures. Keep the same
/// safe classification after an asynchronous handoff without exposing raw errors.
pub(super) fn artifact_observation_failure_kind(job: &ShellJobInfo) -> Option<&'static str> {
    if job.kind != "project_build"
        || job.status != "failed"
        || job.command_execution_state != Some(ShellCommandExecutionState::Completed)
        || job.exit_code != Some(0)
        || job.error.is_none()
    {
        return None;
    }
    Some(if job.error.as_deref() == Some("build_artifacts_missing") {
        "build_artifacts_missing"
    } else {
        "build_artifact_observation_failed"
    })
}

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
        dependency_policy: Option<webcodex_core::project_build::ProjectDependencyPolicy>,
        dependency_resolution_policy: Option<
            webcodex_core::project_build::ProjectDependencyResolutionPolicy,
        >,
        profile: Option<String>,
        targets: Option<webcodex_core::project_build::ProjectBuildTargets>,
        artifact_policy: Option<webcodex_core::project_build::ProjectBuildArtifactPolicy>,
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
            profile,
            targets,
            artifact_policy,
            project_id: project_id.to_string(),
            cwd,
            adapter: adapter.unwrap_or_default(),
            scope,
            dependency_policy,
            dependency_resolution_policy,
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
            .start_job(
                JobStartRequest {
                    client_id: resolved.config.client_id.clone(),
                    cwd: Some(effective_cwd),
                    timeout_secs: budget.effective_timeout_secs,
                    execution: JobStartExecution::Build(plan),
                    context: JobStartContext {
                        project_id: Some(resolved.resolved_id.clone()),
                        session_id: session_id,
                        project_cwd: Some(resolved_cwd.clone()),
                        purpose: Some(ExecutionPurpose::Build.as_str().to_string()),
                        shell: Some("direct_argv".to_string()),
                        visibility: ShellJobVisibility::HiddenUntilHandoff,
                    },
                    codex: None,
                },
                "tool_runtime".to_string(),
                access.as_ref(),
                None,
            )
            .await;
        let job = match job {
            Ok(job) => job,
            Err(error) => {
                let mut result = execution_admission_failure_result(
                    error,
                    "confirm Runner project_build_v1 and structured Job support, then retry only if no build started.",
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
        let mut result = match await_structured_job_result(
            self.runner_registry.clone(),
            job.job_id,
            self.structured_execution_sync_wait,
            auth,
            budget,
            &summary,
            ExecutionPurpose::Build,
            terminal_structured_job_result,
        )
        .await
        {
            Ok(result) => result,
            Err(failure) => failure.into_tool_result(&project, budget),
        };
        decorate(&mut result.output, &backend, &summary, &resolved_cwd);
        result
    }
}

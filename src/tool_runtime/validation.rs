use serde_json::{json, Value};
use std::time::Duration;

#[cfg(test)]
use super::helpers::DEFAULT_CARGO_FMT_TIMEOUT_SECS;
use super::helpers::{
    bounded_tail, command_outcome_unknown_message, command_rejected_message,
    command_timeout_message, looks_like_command_timeout, validate_project_relative_path,
    DEFAULT_CARGO_CHECK_TIMEOUT_SECS, DEFAULT_CARGO_TEST_TIMEOUT_SECS, MAX_VALIDATION_TIMEOUT_SECS,
    MIN_VALIDATION_TIMEOUT_SECS,
};
use super::shell::{command_execution_state_name, ProjectCommandOutput};
use super::structured_execution::{
    recover_hidden_structured_job, structured_job_observation, HiddenStructuredJobWait,
    StructuredExecutionBudget, StructuredJobHandoffFailure, STRUCTURED_EXECUTION_SYNC_WAIT_SECS,
};
use super::tool_result::ToolResult;
use super::validation_profile::{
    requires_multi_package_cargo_check, runtime_profile, validation_evidence_profile_for_recipe,
    CargoReadOnlyValidationOperation, ReadOnlyValidationOperation, ValidationEvidenceProfile,
    ValidationFailureEvidence,
};
#[cfg(test)]
use super::validation_profile::{validation_adapter_for_tool, ValidationCommandOptions};
use super::ToolRuntime;
use crate::auth::AuthContext;
use crate::runner_http::{RunnerFeature, ShellJobStartMetadata};
use crate::runner_protocol::{
    ShellCommandExecutionState, ShellJobOpRequest, ShellJobValidationMetadata,
    ShellJobValidationStep,
};
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;
use webcodex_core::runtime_contract::STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS;
use webcodex_core::validation_source::ValidationSourceFence;
use webcodex_core::workflow_session_contract::ExecutionPurpose;
use webcodex_validation::execution_purpose_for_validation_kind;

const CARGO_STDIO_TAIL_CHARS: usize = 12_000;
const CARGO_VALIDATION_FAILURE_KIND: &str = "validation_failed";
const VALIDATION_FAILURE_GUIDANCE: &str =
    "this result is terminal and has no active Job continuation; inspect bounded validation evidence, fix the reported issue, then run a new structured validation.";

pub(super) fn test_count_assertion_failure_message(tool_name: &str, payload: &Value) -> String {
    if tool_name == "cargo_test"
        && payload.get("zero_tests_run").and_then(Value::as_bool) == Some(true)
    {
        return "cargo_test completed but 0 tests executed; broaden or remove the substring filter, use the test's full qualified name if needed, or verify the selected package/target. Do not put --exact, --nocapture, or other Cargo/libtest flags in filter, and do not treat this result as validation success.".to_string();
    }
    if tool_name == "cargo_test" {
        return "cargo_test test-count assertion was not proven; inspect test_count_assertion and rerun with a scope that executes enough tests.".to_string();
    }
    "structured test-count assertion was not proven; inspect test_count_assertion and rerun with a scope that executes enough tests.".to_string()
}

pub(super) fn terminal_test_count_assertion_failure_message(
    tool_name: &str,
    payload: &Value,
) -> String {
    format!(
        "structured validation command completed with validation failure; {VALIDATION_FAILURE_GUIDANCE} {}",
        test_count_assertion_failure_message(tool_name, payload)
    )
}

pub(super) fn cargo_fmt_check_is_stable_diff(
    adapter: &'static dyn ValidationEvidenceProfile,
    output: &ProjectCommandOutput,
) -> bool {
    if output.execution_state != ShellCommandExecutionState::Completed
        || output.exit_code == Some(0)
    {
        return false;
    }
    adapter.map_failure_kind(ValidationFailureEvidence {
        success: false,
        reported_failure_kind: None,
        exit_code: output.exit_code.map(i64::from),
        diagnostics: None,
        stdout_excerpt: &output.stdout,
        stderr_excerpt: &output.stderr,
    }) == "format_diff"
}

pub(super) fn annotate_cargo_fmt_effect(
    result: &mut ToolResult,
    changed: Option<bool>,
    state_changed: Option<bool>,
) {
    result.output["changed"] = changed.map(Value::Bool).unwrap_or(Value::Null);
    result.output["state_changed"] = state_changed.map(Value::Bool).unwrap_or(Value::Null);
}

pub(super) fn append_cargo_fmt_uncertainty_guidance(result: &mut ToolResult) {
    let guidance =
        "cargo fmt mutation may have changed source; inspect the worktree before retrying.";
    result.error = Some(match result.error.take() {
        Some(error) => format!("{error} {guidance}"),
        None => guidance.to_string(),
    });
}

pub(super) fn validate_cwd(cwd: Option<String>) -> Result<Option<String>, String> {
    match cwd {
        Some(raw) => {
            let trimmed = raw.trim().trim_start_matches("./").trim_end_matches('/');
            validate_project_relative_path(trimmed)?;
            if trimmed.is_empty() || trimmed == "." {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(None),
    }
}

pub(crate) fn count_rustc_diagnostics(text: &str, prefix: &str) -> usize {
    let stem = prefix.trim_end_matches(':');
    let coded_prefix = format!("{stem}[");
    text.lines()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with(prefix) || line.starts_with(&coded_prefix)
        })
        .count()
}

pub(super) fn is_cargo_validation_failure(
    output: &ProjectCommandOutput,
    timeout_secs: u64,
) -> bool {
    output.execution_state == ShellCommandExecutionState::Completed
        && output.exit_code.is_some_and(|exit_code| exit_code != 0)
        && !looks_like_command_timeout(output.exit_code, &output.stderr, timeout_secs)
        && !looks_like_command_infrastructure_failure(&output.stderr)
}

pub(super) fn looks_like_command_infrastructure_failure(stderr: &str) -> bool {
    let trimmed = stderr.trim_start();
    trimmed.starts_with("Failed to execute command:")
        || trimmed.starts_with("Failed to wait for command:")
        || trimmed.starts_with("Failed to collect command output:")
}

pub(super) fn cargo_prestart_failure_kind(error: Option<&str>) -> &'static str {
    let lower = error.unwrap_or_default().to_ascii_lowercase();
    if lower.contains("permission") || lower.contains("denied") || lower.contains("not allowed") {
        "permission_denied"
    } else if lower.contains("cwd")
        || lower.contains("working directory")
        || lower.contains("directory")
    {
        "cwd_invalid"
    } else if lower.contains("project") {
        "project_not_found"
    } else {
        "executor_unavailable"
    }
}

/// Shared structured-validation runtime budget + effective sync window.
struct ValidationBudget {
    /// Total runtime budget of the command (`timeout_secs`), 1..=3600.
    effective_timeout_secs: u64,
    /// How long the tool call blocks in-process before promoting to a Job.
    sync_wait_secs: u64,
}

/// Project validation has no public sync-wait input. Start from the canonical
/// structured-execution grace and allow only trusted return policy to shorten it.
fn project_validation_trusted_sync_wait(structured_handoff_max_secs: Option<u64>) -> Option<u64> {
    structured_handoff_max_secs
        .map(|max_handoff_secs| STRUCTURED_EXECUTION_SYNC_WAIT_SECS.min(max_handoff_secs))
}

/// Resolve a read-only structured validation budget.
///
/// `timeout_secs` is the total runtime budget of the command, not the tool
/// call's synchronous wait. Positive values above the supported ceilings are
/// caller preferences and are clamped before dispatch. Explicit
/// `sync_wait_secs` is likewise clamped to the shared Host-safe model-facing
/// ceiling and the effective total budget. When omitted, use the same canonical
/// early-handoff default as ordinary structured execution, bounded by the
/// effective total timeout.
fn resolve_validation_budget(
    tool_name: &str,
    timeout_secs: Option<u64>,
    sync_wait_secs: Option<u64>,
    default: u64,
) -> Result<ValidationBudget, ToolResult> {
    let requested_timeout_secs = timeout_secs.unwrap_or(default);
    if requested_timeout_secs < MIN_VALIDATION_TIMEOUT_SECS {
        return Err(sync_timeout_out_of_range_result_with_range(
            tool_name,
            MIN_VALIDATION_TIMEOUT_SECS,
            MAX_VALIDATION_TIMEOUT_SECS,
            default,
        ));
    }
    let effective_timeout_secs = requested_timeout_secs.min(MAX_VALIDATION_TIMEOUT_SECS);
    let sync_wait_secs = match sync_wait_secs {
        Some(0) => {
            return Err(validation_sync_wait_rejection(
                tool_name,
                format!("{tool_name} sync_wait_secs must be at least 1"),
                "pass a positive sync_wait_secs, or omit it for the Runtime early-handoff default.",
            ));
        }
        Some(sync_wait) => sync_wait
            .min(STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS)
            .min(effective_timeout_secs),
        None => STRUCTURED_EXECUTION_SYNC_WAIT_SECS.min(effective_timeout_secs),
    };
    Ok(ValidationBudget {
        effective_timeout_secs,
        sync_wait_secs,
    })
}

#[cfg(test)]
mod validation_budget_tests {
    use super::*;

    #[test]
    fn validation_budget_uses_early_handoff_default_and_preserves_explicit_clamps() {
        for (tool_name, default_timeout_secs) in [
            ("cargo_check", DEFAULT_CARGO_CHECK_TIMEOUT_SECS),
            ("cargo_test", DEFAULT_CARGO_TEST_TIMEOUT_SECS),
            ("cargo_fmt", DEFAULT_CARGO_FMT_TIMEOUT_SECS),
            ("go_test", DEFAULT_CARGO_TEST_TIMEOUT_SECS),
        ] {
            let default =
                resolve_validation_budget(tool_name, None, None, default_timeout_secs).unwrap();
            assert_eq!(default.effective_timeout_secs, default_timeout_secs);
            assert_eq!(default.sync_wait_secs, STRUCTURED_EXECUTION_SYNC_WAIT_SECS);
        }

        let short = resolve_validation_budget("cargo_check", Some(3), None, 600).unwrap();
        assert_eq!(short.effective_timeout_secs, 3);
        assert_eq!(short.sync_wait_secs, 3);

        let explicit = resolve_validation_budget("cargo_check", Some(600), Some(45), 600).unwrap();
        assert_eq!(explicit.effective_timeout_secs, 600);
        assert_eq!(explicit.sync_wait_secs, 45);

        let host_boundary =
            resolve_validation_budget("cargo_check", Some(600), Some(60), 600).unwrap();
        assert_eq!(host_boundary.effective_timeout_secs, 600);
        assert_eq!(
            host_boundary.sync_wait_secs,
            STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS
        );

        let oversized =
            resolve_validation_budget("cargo_check", Some(4_000), Some(600), 600).unwrap();
        assert_eq!(
            oversized.effective_timeout_secs,
            MAX_VALIDATION_TIMEOUT_SECS
        );
        assert_eq!(
            oversized.sync_wait_secs,
            STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS
        );

        let over_total =
            resolve_validation_budget("cargo_check", Some(30), Some(600), 600).unwrap();
        assert_eq!(over_total.effective_timeout_secs, 30);
        assert_eq!(over_total.sync_wait_secs, 30);

        assert!(resolve_validation_budget("cargo_check", Some(0), None, 600).is_err());
        assert!(resolve_validation_budget("cargo_check", Some(600), Some(0), 600).is_err());

        let project_default = resolve_validation_budget(
            "project_validate",
            Some(600),
            project_validation_trusted_sync_wait(None),
            600,
        )
        .unwrap();
        assert_eq!(
            project_default.sync_wait_secs,
            STRUCTURED_EXECUTION_SYNC_WAIT_SECS
        );

        let project_loose_cap = resolve_validation_budget(
            "project_validate",
            Some(600),
            project_validation_trusted_sync_wait(Some(55)),
            600,
        )
        .unwrap();
        assert_eq!(
            project_loose_cap.sync_wait_secs,
            STRUCTURED_EXECUTION_SYNC_WAIT_SECS
        );

        let project_strict_cap = resolve_validation_budget(
            "project_validate",
            Some(600),
            project_validation_trusted_sync_wait(Some(4)),
            600,
        )
        .unwrap();
        assert_eq!(project_strict_cap.effective_timeout_secs, 600);
        assert_eq!(project_strict_cap.sync_wait_secs, 4);

        let project_short_timeout = resolve_validation_budget(
            "project_validate",
            Some(3),
            project_validation_trusted_sync_wait(Some(5)),
            600,
        )
        .unwrap();
        assert_eq!(project_short_timeout.effective_timeout_secs, 3);
        assert_eq!(project_short_timeout.sync_wait_secs, 3);

        assert!(resolve_validation_budget(
            "project_validate",
            Some(600),
            project_validation_trusted_sync_wait(Some(0)),
            600,
        )
        .is_err());
    }
}

pub(super) fn validation_sync_wait_rejection(
    tool_name: &str,
    reason: impl Into<String>,
    guidance: impl Into<String>,
) -> ToolResult {
    ToolResult::err_with_output(
        command_rejected_message(reason.into(), guidance.into()),
        json!({
            "execution_source": tool_name,
            "execution_state": "not_started",
            "command_started": false,
            "command_completed": false,
            "failure_kind": "invalid_arguments",
            "tool_failure": true,
        }),
    )
}

pub(super) fn sync_timeout_out_of_range_result_with_range(
    tool_name: &str,
    min: u64,
    max: u64,
    default: u64,
) -> ToolResult {
    super::tool_result::ToolResult::err_with_output(
        command_rejected_message(
            format!("{tool_name} timeout_secs must be between {min} and {max}"),
            format!(
                "pass timeout_secs between {min} and {max}, or omit it for the default of {default} seconds."
            ),
        ),
        json!({
            "command_started": false,
            "command_completed": false,
            "command_ok": false,
            "exit_code": null,
            "failure_kind": "invalid_arguments",
            "tool_failure": true,
        }),
    )
}

pub(super) fn cargo_test_assertion_rejection(
    reason: impl AsRef<str>,
    guidance: &str,
) -> ToolResult {
    ToolResult::err_with_output(
        command_rejected_message(reason, guidance),
        json!({
            "execution_source": "cargo_test",
            "execution_state": "not_started",
            "command_started": false,
            "command_completed": false,
            "failure_kind": "invalid_arguments",
            "tool_failure": true,
        }),
    )
}

pub(super) fn resolve_cargo_test_minimum(
    require_tests: Option<bool>,
    min_tests: Option<u64>,
    no_run: Option<bool>,
) -> Result<Option<u64>, ToolResult> {
    if let Some(minimum) = min_tests {
        if !(1..=crate::runner_protocol::CARGO_TEST_MIN_TESTS_MAX).contains(&minimum) {
            return Err(cargo_test_assertion_rejection(
                format!(
                    "cargo_test min_tests must be between 1 and {}",
                    crate::runner_protocol::CARGO_TEST_MIN_TESTS_MAX
                ),
                "pass a bounded positive min_tests value, or omit it.",
            ));
        }
    }
    let minimum = match (require_tests.unwrap_or(false), min_tests) {
        (true, Some(minimum)) => Some(minimum.max(1)),
        (true, None) => Some(1),
        (false, minimum) => minimum,
    };
    if minimum.is_some() && no_run.unwrap_or(false) {
        return Err(cargo_test_assertion_rejection(
            "cargo_test test-count assertions cannot be combined with no_run=true",
            "remove no_run to execute tests, or omit require_tests and min_tests when only compiling tests.",
        ));
    }
    Ok(minimum)
}

pub(super) fn reject_structured_validation_ssh_resource(
    ssh_resource: Option<&str>,
) -> Option<ToolResult> {
    ssh_resource.map(|_| {
        ToolResult::err(command_rejected_message(
            "ssh_resource_unsupported_for_request: SSH resources do not support structured validation tools",
            "use the named runner host through run_shell or run_job instead.",
        ))
    })
}

fn validation_no_run(operation: &ReadOnlyValidationOperation) -> Option<bool> {
    match operation {
        ReadOnlyValidationOperation::Cargo(CargoReadOnlyValidationOperation::Test(options)) => {
            options.no_run
        }
        _ => None,
    }
}

impl ToolRuntime {
    pub(crate) async fn project_validate(
        &self,
        project: String,
        session_id: Option<String>,
        cwd: Option<String>,
        action: webcodex_core::project_validation::ProjectValidationAction,
        adapter_hint: Option<webcodex_core::project_validation::ProjectValidationAdapter>,
        scope: Option<webcodex_core::project_validation::ProjectValidationScope>,
        dependency_policy: Option<webcodex_core::project_validation::ProjectDependencyPolicy>,
        test: Option<webcodex_core::project_validation::ProjectValidationTestOptions>,
        timeout_secs: Option<u64>,
        structured_handoff_max_secs: Option<u64>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        use webcodex_core::project_validation::*;
        if let Some(result) = reject_structured_validation_ssh_resource(ssh_resource) {
            return result;
        }
        let budget = match resolve_validation_budget(
            "project_validate",
            timeout_secs,
            project_validation_trusted_sync_wait(structured_handoff_max_secs),
            if action == ProjectValidationAction::Test {
                DEFAULT_CARGO_TEST_TIMEOUT_SECS
            } else {
                DEFAULT_CARGO_CHECK_TIMEOUT_SECS
            },
        ) {
            Ok(v) => v,
            Err(e) => return e,
        };
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e.to_message()),
        };
        let Some((_, project_id)) = resolved
            .resolved_id
            .strip_prefix("agent:")
            .and_then(|s| s.split_once(':'))
        else {
            return ToolResult::err("project_validate requires an exact Runner Project");
        };
        let request = ProjectValidationRequest {
            project_id: project_id.into(),
            cwd,
            action,
            adapter: adapter_hint.unwrap_or_default(),
            scope,
            dependency_policy,
            test,
        };
        if let Err(e) = request.validate() {
            return ToolResult::err(e);
        }
        let access = crate::runner_http::runner_access_from_auth(auth);
        let (request_id, rx) = match self
            .runner_registry
            .enqueue_project_validation_plan(
                resolved.config.client_id.clone(),
                request.clone(),
                access.as_ref(),
            )
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        let response = match tokio::time::timeout(Duration::from_secs(32), rx).await {
            Ok(Ok(v)) => v,
            _ => {
                self.runner_registry.cancel_request(&request_id).await;
                return ToolResult::err(
                    "validation planning unavailable; no validation execution started",
                );
            }
        };
        if response.error.is_some() {
            return ToolResult::err("Runner validation planning failed; no execution started");
        }
        let plan = match serde_json::from_str::<ProjectValidationPlanningResult>(response.stdout.as_deref().unwrap_or("")) {
            Ok(ProjectValidationPlanningResult::Ready { plan }) => plan,
            Ok(ProjectValidationPlanningResult::Unavailable { code, detected_backend }) => return ToolResult::err_with_output(
                "Project validation unavailable; use a supported Rust/Go action, configured Python pytest/Ruff, or a Node >=22.3 native check script on a supported Runner.",
                json!({"execution_source":"project_validate", "execution_state":"not_started", "command_started":false, "command_completed":false, "failure_kind":code, "detected_backend":detected_backend})),
            Err(_) => return ToolResult::err("invalid Runner validation plan; upgrade Server and Runner together"),
        };
        let project_workspace = plan.provenance.backend == "rust"
            && request.scope.as_ref().is_some_and(
                webcodex_core::project_validation::ProjectValidationScope::selects_all_packages,
            );
        let canonical_step = if project_workspace {
            plan.step.is_project_workspace_cargo()
        } else {
            plan.step.is_canonical()
        };
        if !plan.provenance.is_valid() || plan.provenance.request != request || !canonical_step {
            return ToolResult::err("invalid Runner project validation plan");
        }
        let semantic_action = match action {
            ProjectValidationAction::FormatCheck => webcodex_validation::SemanticCheck::Format,
            ProjectValidationAction::Check => webcodex_validation::SemanticCheck::Check,
            ProjectValidationAction::Test => webcodex_validation::SemanticCheck::Test,
        };
        let Some(adapter) =
            validation_evidence_profile_for_recipe(&plan.provenance.backend, semantic_action)
        else {
            return ToolResult::err(
                "validation adapter unavailable; upgrade Server and Runner together",
            );
        };
        if adapter.tool_identity() != plan.adapter || adapter.validation_kind() != action.kind() {
            return ToolResult::err("invalid Runner project validation evidence profile");
        }
        let source_fence = self.validation_sources.capture(&resolved.resolved_id);
        let provenance = plan.provenance;
        let relative_root = provenance.recipe_root.clone();
        let command = format!("{} {}", plan.step.program, plan.step.args.join(" "));
        let (require_tests, minimum) = request.test_requirements();
        let target_id = plan.validation_target_id;
        let mut result = self
            .run_readonly_validation_agent(
                "project_validate",
                Some(provenance.clone()),
                &resolved.resolved_id,
                &resolved.config,
                Some(&relative_root),
                &command,
                adapter,
                plan.step,
                execution_purpose_for_validation_kind(action.kind()),
                budget.effective_timeout_secs,
                budget.sync_wait_secs,
                session_id,
                Some(target_id.clone()),
                source_fence.clone(),
                minimum,
                require_tests,
                None,
                None,
                auth,
            )
            .await;
        result.output["validation_target_id"] = json!(target_id);
        result.output["backend"] = json!(provenance.backend);
        result.output["action"] = json!(action);
        result.output["adapter"] = json!(adapter.tool_identity());
        result.output["source_state"] = json!(self
            .validation_sources
            .observe(&resolved.resolved_id, source_fence.as_ref()));
        sanitize_project_validation_output(&mut result.output);
        result
    }

    pub(super) async fn run_readonly_validation(
        &self,
        operation: ReadOnlyValidationOperation,
        request: ValidationRunRequest<'_>,
    ) -> ToolResult {
        let runtime_profile = runtime_profile(&operation);
        let compatibility = operation.compatibility_profile();
        let tool_name = compatibility.tool_identity;
        let budget = match resolve_validation_budget(
            tool_name,
            request.timeout_secs,
            request.sync_wait_secs,
            runtime_profile.default_timeout_secs,
        ) {
            Ok(budget) => budget,
            Err(result) => return result,
        };
        let cwd = match validate_cwd(request.cwd) {
            Ok(cwd) => cwd,
            Err(e) => {
                return ToolResult::err(command_rejected_message(
                    e,
                    "choose an existing project-relative cwd, then retry.",
                ))
            }
        };
        let adapter = operation.evidence_profile();
        let no_run = validation_no_run(&operation);
        let validation_target_id = operation.validation_target_id(cwd.as_deref());
        let plan = match operation.build_readonly_plan() {
            Ok(plan) => plan,
            Err(e) => {
                return ToolResult::err(command_rejected_message(
                    e,
                    runtime_profile.invalid_argument_guidance,
                ))
            }
        };
        let command = plan.compatibility_command;
        let step = plan.structured_step;
        let requires_multi_package_cargo_check = requires_multi_package_cargo_check(&operation);
        // Pre-execution validation happens before any execution is created, so
        // a rejection never leaves a Job or a running process behind.
        let resolved = match self
            .resolve_project_input_for_auth(&request.project, request.auth)
            .await
        {
            Ok(config) => config,
            Err(e) => return ToolResult::err(command_rejected_message(
                e.to_message(),
                "verify the project id with list_projects, then retry with a registered project.",
            )),
        };
        let source_project = resolved.resolved_id;
        let runner_client_id = resolved.config.client_id.clone();
        let resolved = resolved.config;
        if requires_multi_package_cargo_check {
            let access = crate::runner_http::runner_access_from_auth(request.auth);
            let runner = match self
                .runner_registry
                .get_runner_semantic_view_checked_for_auth(&runner_client_id, access.as_ref())
                .await
            {
                Ok(runner) => runner,
                Err(error) => {
                    return ToolResult::err(command_rejected_message(
                        error,
                        "verify the target Runner is connected and authorized, then retry.",
                    ))
                }
            };
            if !runner.supports(RunnerFeature::StructuredCargoCheckPackages) {
                return ToolResult::err(command_rejected_message(
                    format!(
                        "capability_unavailable: structured_cargo_check_packages_unavailable: runner {} does not support repeated Cargo check package selectors",
                        runner_client_id
                    ),
                    "upgrade the target Runner or use the legacy single-package selector.",
                ));
            }
        }
        let purpose = execution_purpose_for_validation_kind(adapter.validation_kind());
        let timeout_secs = budget.effective_timeout_secs;
        let sync_wait_secs = budget.sync_wait_secs;
        let session_id = request.session_id.clone();
        let ssh_resource = request.ssh_resource.map(str::to_string);

        // Structured validation tools never execute through a named SSH resource.
        // Reject at the shared Agent entry so direct sync, short sync, and
        // long Job handoff paths cannot silently fall back to the project root.
        if let Some(result) = reject_structured_validation_ssh_resource(ssh_resource.as_deref()) {
            return result;
        }
        let source_fence = self.validation_sources.capture(&source_project);
        let mut result = if runtime_profile.force_agent_handoff || sync_wait_secs < timeout_secs {
            // The effective synchronous grace is shorter than the total
            // validation budget, so there is headroom to expose the same
            // execution as a Job. The agent path enqueues exactly one
            // structured validation Job, waits up to `sync_wait_secs`, and
            // hands off if it is still running.
            self.run_readonly_validation_agent(
                tool_name,
                None,
                &source_project,
                &resolved,
                cwd.as_deref(),
                &command,
                adapter,
                step,
                purpose,
                timeout_secs,
                sync_wait_secs,
                session_id,
                validation_target_id,
                source_fence.clone(),
                request.minimum_tests,
                request.require_tests,
                no_run,
                ssh_resource.as_deref(),
                request.auth,
            )
            .await
        } else {
            // No handoff headroom: the requested budget is at most the
            // sync window, so there is no remaining runtime for a Job to
            // continue. Run synchronously through the existing capture
            // path and report a real terminal timeout at the budget
            // boundary. The command still starts exactly once.
            let output = match self
                    .run_project_command_capture(
                        &request.project,
                        command.clone(),
                        timeout_secs,
                        cwd.clone(),
                    )
                    .await
                {
                    Ok(output) => output,
                    Err(e) => {
                        return ToolResult::err(command_rejected_message(
                            e,
                            "verify the project id/cwd and agent connectivity, then retry or use run_shell for custom diagnostics.",
                        ))
                    }
                };
            self.build_cargo_result(
                &request.project,
                &command,
                cwd.as_deref(),
                &resolved,
                adapter,
                output,
                timeout_secs,
                sync_wait_secs,
                false,
                false,
                false,
                request.minimum_tests,
                request.require_tests,
                no_run,
            )
            .await
        };
        result.output["source_state"] = json!(self
            .validation_sources
            .observe(&source_project, source_fence.as_ref()));
        result
    }

    /// Agent-backed read-only validation. Enqueues exactly one validation
    /// Job, waits up to `sync_wait_secs`, and returns either the in-window
    /// terminal result (discarding the now-hidden Job) or a Job handoff for
    /// the same execution.
    #[allow(clippy::too_many_arguments)]
    async fn run_readonly_validation_agent(
        &self,
        tool_name: &str,
        project_validation: Option<webcodex_core::project_validation::ProjectValidationProvenance>,
        project: &str,
        config: &crate::projects::ProjectConfig,
        cwd: Option<&str>,
        command: &str,
        adapter: &'static dyn ValidationEvidenceProfile,
        step: ShellJobValidationStep,
        purpose: ExecutionPurpose,
        timeout_secs: u64,
        sync_wait_secs: u64,
        session_id: Option<String>,
        validation_target_id: Option<String>,
        source_fence: Option<ValidationSourceFence>,
        minimum_tests: Option<u64>,
        require_tests: Option<bool>,
        no_run: Option<bool>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let client_id = config.client_id.clone();
        let effective_cwd = match super::helpers::resolve_runner_cwd(config, cwd) {
            Ok(cwd) => cwd,
            Err(error) => {
                return ToolResult::err(command_rejected_message(
                    error,
                    "choose '.', an existing project-relative cwd, or a path inside the registered project root.",
                ))
            }
        };
        let resolved_cwd = super::helpers::project_relative_runner_cwd(config, &effective_cwd)
            .unwrap_or_else(|_| ".".to_string());
        let actual_shell = "configured";
        // The supplied structured step is the execution authority here. Direct
        // specialist callers build it through their adapter; project_validate
        // receives it from the Runner-resolved recipe. Carry that exact step into
        // the Job so sync and promoted paths cannot drift.
        let dispatched_command = match serde_json::to_string(std::slice::from_ref(&step)) {
            Ok(command) => command,
            Err(_) => {
                return ToolResult::err(command_rejected_message(
                    "could not serialize structured validation step",
                    "fix the structured validation argument format, then retry.",
                ))
            }
        };
        let access = crate::runner_http::runner_access_from_auth(auth);
        let job = match self
            .runner_registry
            .start_job_with_metadata_for_access(
                ShellJobOpRequest {
                    login: false,
                    op: "start".to_string(),
                    client_id: Some(client_id),
                    cwd: Some(effective_cwd),
                    command: Some(dispatched_command),
                    timeout_secs: Some(timeout_secs),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tool_runtime".to_string(),
                ShellJobStartMetadata {
                    project_id: Some(project.to_string()),
                    session_id: session_id.clone(),
                    ssh_resource: ssh_resource.map(str::to_string),
                    project_cwd: Some(resolved_cwd.clone()),
                    purpose: Some(purpose.as_str().to_string()),
                    shell: Some(actual_shell.to_string()),
                    explicit_shell: None,
                    validation_steps: vec![step.clone()],
                    validation: Some(ShellJobValidationMetadata {
                        project_validation,
                        tool: tool_name.to_string(),
                        kind: adapter.validation_kind().to_string(),
                        steps: vec![step],
                        effective_timeout_secs: timeout_secs,
                        sync_wait_secs,
                        adapter: adapter.tool_identity().to_string(),
                        validation_target_id: validation_target_id.clone(),
                        source_fence,
                        minimum_tests,
                        require_tests,
                        no_run,
                    }),
                    visibility: crate::runner_http::ShellJobVisibility::HiddenUntilHandoff,
                    validation_identity: None,
                    validation_tool: None,
                    assertion_name: None,
                    structured_execution: None,
                    stdin: None,
                    detached_idempotency_key: None,
                },
                access.as_ref(),
                None,
            )
            .await
        {
            Ok(job) => job,
            Err(e) => {
                return ToolResult::err(command_rejected_message(
                    e,
                    "confirm the agent is connected and structured validation jobs are allowed, then retry.",
                ))
            }
        };
        let job_id = job.job_id.clone();
        let handoff = ValidationHandoff {
            execution_source: tool_name.to_string(),
            purpose: purpose.as_str().to_string(),
            job_id: job_id.clone(),
            effective_timeout_secs: timeout_secs,
            sync_wait_secs,
            project: project.to_string(),
            cwd: resolved_cwd,
            shell: actual_shell.to_string(),
            executor: "agent".to_string(),
            command_summary: crate::runner_http::command_preview(command),
            minimum_tests,
            require_tests,
            no_run,
            auth: access,
        };
        self.await_validation_job(job_id, sync_wait_secs, adapter, handoff)
            .await
    }

    /// Wait up to `sync_wait_secs` for a structured validation Job to reach a
    /// terminal state. If it completes in-window, return the terminal result
    /// and discard the hidden Job; if it is still active, return a Job handoff
    /// for the same execution.
    ///
    /// The `ValidationCleanupGuard` arms cleanup while the sync wait is in
    /// flight. If the MCP request is cancelled before handoff, a queued start is
    /// removed atomically; an accepted/running Job is marked cleanup-pending,
    /// stopped, and retained for reconciliation until a terminal Runner update
    /// confirms cleanup. Once a terminal result or handoff is produced, the
    /// guard is disarmed.
    async fn await_validation_job(
        &self,
        job_id: String,
        sync_wait_secs: u64,
        adapter: &'static dyn ValidationEvidenceProfile,
        handoff: ValidationHandoff,
    ) -> ToolResult {
        let mut guard = ValidationCleanupGuard::new(
            self.runner_registry.clone(),
            job_id.clone(),
            handoff.auth.clone(),
        );
        // Poll the job status up to the effective synchronous grace. Each poll
        // is cheap and total sleep stays bounded by that grace. The wait is
        // taken from the runtime's injectable clock so tests can shrink it.
        let wait = self
            .validation_sync_wait
            .min(std::time::Duration::from_secs(sync_wait_secs));
        let deadline = std::time::Instant::now() + wait;
        loop {
            let status = self
                .runner_registry
                .get_hidden_job_for_auth(handoff.auth.as_ref(), &job_id)
                .await;
            let (terminal, observed_status) = match status {
                Ok(status) => (
                    crate::tool_runtime::jobs::is_terminal_job_status(&status.status),
                    status.status,
                ),
                Err(_) => (false, "unknown".to_string()),
            };
            if terminal {
                // The terminal result builder removes the hidden Job record.
                let result = self
                    .validation_terminal_result(job_id, adapter, &observed_status, handoff)
                    .await;
                guard.disarm_if_returnable(&result);
                return result;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        // Promote under the registry lock. A terminal update may race the
        // deadline; promote_hidden_job deliberately leaves such a record hidden
        // so the original Cargo call can still return its structured terminal
        // result instead of handing off an already-finished Job.
        let promoted = match self
            .runner_registry
            .promote_hidden_job(handoff.auth.as_ref(), &job_id)
            .await
        {
            Ok(job) => job,
            Err(_) => {
                return Box::pin(
                    self.recover_validation_handoff(job_id, adapter, handoff, &mut guard),
                )
                .await;
            }
        };
        let latest_status = promoted.status.clone();
        if crate::tool_runtime::jobs::is_terminal_job_status(&latest_status) {
            let result = self
                .validation_terminal_result(job_id, adapter, &latest_status, handoff)
                .await;
            guard.disarm_if_returnable(&result);
            return result;
        }
        let observation =
            match structured_job_observation(&self.runner_registry, handoff.auth.as_ref(), &job_id)
                .await
            {
                Ok(observation) => observation,
                Err(_) => {
                    return Box::pin(
                        self.recover_validation_handoff(job_id, adapter, handoff, &mut guard),
                    )
                    .await;
                }
            };
        let latest_status = observation.job.status.clone();
        if crate::tool_runtime::jobs::is_terminal_job_status(&latest_status) {
            let result = self
                .validation_terminal_result(job_id, adapter, &latest_status, handoff)
                .await;
            guard.disarm_if_returnable(&result);
            return result;
        }
        let observation_token = match observation.job.observation_token.clone() {
            Some(token) => token,
            None => {
                return Box::pin(
                    self.recover_validation_handoff(job_id, adapter, handoff, &mut guard),
                )
                .await;
            }
        };
        let (execution_state, command_started) = validation_handoff_execution_state(
            latest_status.as_str(),
            observation.job.started_at.is_some(),
        );
        let detected_summary = crate::tool_runtime::jobs::detected_job_summary_with_activity(
            Some(&handoff.command_summary),
            Some(&handoff.purpose),
            &latest_status,
            observation.job.exit_code.map(i64::from),
            &observation.stdout_tail,
            &observation.stderr_tail,
            observation.stdout_truncated || observation.stderr_truncated,
            observation.job.activity.as_ref(),
        );
        let continuation = crate::tool_runtime::jobs::observe_job_continuation(
            &handoff.job_id,
            Some(&observation_token),
        );
        let payload = json!({
            "execution_source": handoff.execution_source,
            "purpose": handoff.purpose,
            "execution_state": execution_state,
            "job_id": handoff.job_id,
            "job_status": latest_status,
            "observation_token": observation_token,
            "continuation_semantics": crate::tool_runtime::jobs::job_observation_continuation_semantics(),
            "activity": observation.job.activity,
            "promoted_to_job": true,
            "command_started": command_started,
            "command_completed": false,
            "effective_timeout_secs": handoff.effective_timeout_secs,
            "sync_wait_secs": handoff.sync_wait_secs,
            "project": handoff.project,
            "cwd": handoff.cwd,
            "shell": handoff.shell,
            "executor": handoff.executor,
            "command_summary": handoff.command_summary,
            "stdout_tail": observation.stdout_tail,
            "stderr_tail": observation.stderr_tail,
            "stdout_lines": observation.stdout_lines,
            "stderr_lines": observation.stderr_lines,
            "stdout_truncated": observation.stdout_truncated,
            "stderr_truncated": observation.stderr_truncated,
            "detected_summary": detected_summary,
            "terminal": false,
            "continuation": continuation,
        });
        guard.disarm();
        ToolResult::ok(payload)
    }

    /// Recover while the initiating cleanup guard still owns the hidden record.
    /// This calls only the same canonical visibility transition and observations.
    /// Callers box this cold future so terminal parsing/recovery does not inflate
    /// every ordinary dispatch future (including unrelated observations).
    async fn recover_validation_handoff(
        &self,
        job_id: String,
        adapter: &'static dyn ValidationEvidenceProfile,
        handoff: ValidationHandoff,
        guard: &mut ValidationCleanupGuard,
    ) -> ToolResult {
        let result = match recover_hidden_structured_job(
            &self.runner_registry,
            handoff.auth.as_ref(),
            &job_id,
        )
        .await
        {
            Ok(HiddenStructuredJobWait::Terminal { job, .. }) => {
                self.validation_terminal_result(job_id, adapter, &job.status, handoff)
                    .await
            }
            Ok(HiddenStructuredJobWait::Continued { .. }) => {
                unreachable!("recovery never fabricates a successful handoff observation")
            }
            Err(failure) => validation_handoff_failure_result(failure, &handoff),
        };
        guard.disarm_if_returnable(&result);
        result
    }

    /// Build a terminal structured validation result from a Job's final state.
    async fn validation_terminal_result(
        &self,
        job_id: String,
        adapter: &'static dyn ValidationEvidenceProfile,
        job_status: &str,
        handoff: ValidationHandoff,
    ) -> ToolResult {
        let log = self
            .runner_registry
            .hidden_job_log_for_auth(handoff.auth.as_ref(), &job_id, Some(200))
            .await;
        let (job, stdout, stderr, stdout_source_truncated, stderr_source_truncated) = match log {
            Ok((job, stdout, stderr, next_stdout_line, next_stderr_line)) => {
                let stdout = stdout.unwrap_or_default();
                let stderr = stderr.unwrap_or_default();
                let stdout_source_truncated = job.stdout_log_truncated
                    || job.stdout_retained_from_line.is_some_and(|line| line > 1)
                    || stdout.lines().count() < next_stdout_line.saturating_sub(1);
                let stderr_source_truncated = job.stderr_log_truncated
                    || job.stderr_retained_from_line.is_some_and(|line| line > 1)
                    || stderr.lines().count() < next_stderr_line.saturating_sub(1);
                (
                    Some(job),
                    stdout,
                    stderr,
                    stdout_source_truncated,
                    stderr_source_truncated,
                )
            }
            Err(_) => {
                let failure = StructuredJobHandoffFailure::unresolved(
                    &self.runner_registry,
                    handoff.auth.as_ref(),
                    &job_id,
                )
                .await;
                return validation_handoff_failure_result(failure, &handoff);
            }
        };
        let (stdout_tail, bounded_stdout_truncated) = bounded_tail(&stdout, CARGO_STDIO_TAIL_CHARS);
        let (stderr_tail, bounded_stderr_truncated) = bounded_tail(&stderr, CARGO_STDIO_TAIL_CHARS);
        let stdout_truncated = stdout_source_truncated || bounded_stdout_truncated;
        let stderr_truncated = stderr_source_truncated || bounded_stderr_truncated;
        let exit_code = job.as_ref().and_then(|job| job.exit_code);
        if handoff.execution_source == "project_validate"
            && job.as_ref().is_some_and(|job| {
                job.command_execution_state == Some(ShellCommandExecutionState::NotStarted)
            })
        {
            let stale = job
                .as_ref()
                .and_then(|job| job.error.as_deref())
                .is_some_and(|error| error.contains("validation_plan_stale"));
            self.runner_registry
                .remove_projected_hidden_terminal_job_record(&job_id)
                .await;
            return ToolResult::err_with_output(
                if stale {
                    "Project validation plan changed before execution; resolve project_validate again. No command started."
                } else {
                    "Runner rejected validation before execution; inspect project availability and Runner policy, then retry."
                },
                json!({"execution_source": "project_validate", "execution_state": "not_started", "command_started": false, "command_completed": false, "failure_kind": if stale { "validation_plan_stale" } else { "validation_unavailable" }, "terminal": true}),
            );
        }
        let lifecycle = RunnerJobLifecycle::from_wire(job_status).ok();
        let timed_out = lifecycle.is_some_and(RunnerJobLifecycle::is_timed_out);
        let process_passed =
            lifecycle == Some(RunnerJobLifecycle::Completed) && exit_code == Some(0);
        let mut payload = json!({
            "project": handoff.project,
            "command_summary": handoff.command_summary,
            "cwd": handoff.cwd,
            "shell": handoff.shell,
            "executor": handoff.executor,
            "execution_source": handoff.execution_source,
            "purpose": handoff.purpose,
            "execution_state": if timed_out { "timed_out" } else { "completed" },
            "exit_code": exit_code,
            "duration_ms": job.as_ref().and_then(|job| job.duration_ms),
            "stdout_tail": stdout_tail,
            "stderr_tail": stderr_tail,
            "stdout_lines": stdout.lines().count(),
            "stderr_lines": stderr.lines().count(),
            "stdout_truncated": stdout_truncated,
            "stderr_truncated": stderr_truncated,
            "passed": process_passed,
            "command_started": true,
            "command_completed": !timed_out,
            "promoted_to_job": false,
            "effective_timeout_secs": handoff.effective_timeout_secs,
            "sync_wait_secs": handoff.sync_wait_secs,
            "terminal": true,
        });
        if let Some(projection) = crate::tool_runtime::jobs::validation_job_projection_with_policy(
            Some(adapter.tool_identity()),
            Some(adapter.validation_kind()),
            job_status,
            exit_code.map(i64::from),
            &stdout_tail,
            &stderr_tail,
            stdout_truncated || stderr_truncated,
            job.as_ref()
                .and_then(|job| job.test_count_evidence.as_ref()),
            handoff.minimum_tests,
            handoff.require_tests,
            handoff.no_run,
        ) {
            apply_validation_projection_fields(&mut payload, &projection);
        }
        let validation_passed = payload
            .get("passed")
            .and_then(Value::as_bool)
            .unwrap_or(process_passed);
        if validation_passed {
            // Discard the hidden Job record so a fast validation never leaves a
            // redundant visible job in list_jobs.
            self.runner_registry
                .remove_projected_hidden_terminal_job_record(&job_id)
                .await;
            ToolResult::ok(payload)
        } else {
            let failure_kind = if timed_out {
                "timeout"
            } else {
                CARGO_VALIDATION_FAILURE_KIND
            };
            payload["failure_kind"] = json!(failure_kind);
            let error = if timed_out {
                command_timeout_message(handoff.effective_timeout_secs, &stdout_tail, &stderr_tail)
            } else if process_passed {
                terminal_test_count_assertion_failure_message(adapter.tool_identity(), &payload)
            } else {
                format!("structured validation command completed with validation failure; {VALIDATION_FAILURE_GUIDANCE}")
            };
            let result = ToolResult {
                success: false,
                output: payload,
                error: Some(error),
            };
            self.runner_registry
                .remove_projected_hidden_terminal_job_record(&job_id)
                .await;
            result
        }
    }

    /// Build a structured cargo result for a command that ran synchronously in
    /// this process (local path). Mirrors the previous terminal structure.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn build_cargo_result(
        &self,
        project: &str,
        command: &str,
        cwd: Option<&str>,
        config: &crate::projects::ProjectConfig,
        adapter: &'static dyn ValidationEvidenceProfile,
        output: ProjectCommandOutput,
        timeout_secs: u64,
        sync_wait_secs: u64,
        promoted_to_job: bool,
        source_stdout_truncated: bool,
        source_stderr_truncated: bool,
        minimum_tests: Option<u64>,
        require_tests: Option<bool>,
        no_run: Option<bool>,
    ) -> ToolResult {
        let execution_state = output.execution_state;
        let (stdout_tail, bounded_stdout_truncated) =
            bounded_tail(&output.stdout, CARGO_STDIO_TAIL_CHARS);
        let (stderr_tail, bounded_stderr_truncated) =
            bounded_tail(&output.stderr, CARGO_STDIO_TAIL_CHARS);
        let stdout_truncated = source_stdout_truncated || bounded_stdout_truncated;
        let stderr_truncated = source_stderr_truncated || bounded_stderr_truncated;
        let process_passed =
            execution_state == ShellCommandExecutionState::Completed && output.exit_code == Some(0);
        let validation_failed = is_cargo_validation_failure(&output, timeout_secs);
        let resolved_cwd = super::helpers::resolve_runner_cwd(config, cwd)
            .and_then(|path| super::helpers::project_relative_runner_cwd(config, &path))
            .unwrap_or_else(|_| ".".to_string());
        let shell = "configured";
        let executor = "agent";
        let purpose = execution_purpose_for_validation_kind(adapter.validation_kind()).as_str();
        let mut payload = json!({
            "project": project,
            "command_summary": crate::runner_http::command_preview(command),
            "cwd": resolved_cwd,
            "shell": shell,
            "executor": executor,
            "execution_source": adapter.tool_identity(),
            "purpose": purpose,
            "execution_state": command_execution_state_name(execution_state),
            "exit_code": output.exit_code,
            "duration_ms": output.duration_ms,
            "stdout_tail": stdout_tail,
            "stderr_tail": stderr_tail,
            "stdout_lines": output.stdout.lines().count(),
            "stderr_lines": output.stderr.lines().count(),
            "stdout_truncated": stdout_truncated,
            "stderr_truncated": stderr_truncated,
            "passed": process_passed,
            "command_started": execution_state != ShellCommandExecutionState::NotStarted,
            "command_completed": execution_state == ShellCommandExecutionState::Completed,
            "promoted_to_job": promoted_to_job,
            "effective_timeout_secs": timeout_secs,
            "sync_wait_secs": sync_wait_secs,
            "terminal": execution_state != ShellCommandExecutionState::OutcomeUnknown,
        });
        let terminal_status = match execution_state {
            ShellCommandExecutionState::NotStarted | ShellCommandExecutionState::OutcomeUnknown => {
                "lost"
            }
            ShellCommandExecutionState::TimedOut => "timeout",
            ShellCommandExecutionState::Completed if process_passed => "completed",
            ShellCommandExecutionState::Completed => "failed",
        };
        if let Some(projection) = crate::tool_runtime::jobs::validation_job_projection_with_policy(
            Some(adapter.tool_identity()),
            Some(adapter.validation_kind()),
            terminal_status,
            output.exit_code.map(i64::from),
            &stdout_tail,
            &stderr_tail,
            stdout_truncated || stderr_truncated,
            None,
            minimum_tests,
            require_tests,
            no_run,
        ) {
            apply_validation_projection_fields(&mut payload, &projection);
        }
        let validation_passed = payload
            .get("passed")
            .and_then(Value::as_bool)
            .unwrap_or(process_passed);
        match execution_state {
            ShellCommandExecutionState::Completed if validation_passed => ToolResult::ok(payload),
            ShellCommandExecutionState::NotStarted => {
                payload["failure_kind"] =
                    json!(cargo_prestart_failure_kind(output.error.as_deref()));
                ToolResult {
                    success: false,
                    output: payload,
                    error: Some(command_rejected_message(
                        output
                            .error
                            .as_deref()
                            .unwrap_or("the Runner rejected the structured validation command before process spawn"),
                        "correct the project, cwd, permissions, or Runner availability indicated by the rejection, then retry.",
                    )),
                }
            }
            ShellCommandExecutionState::OutcomeUnknown => {
                payload["failure_kind"] = json!("outcome_unknown");
                ToolResult {
                    success: false,
                    output: payload,
                    error: Some(command_outcome_unknown_message(
                        output.error.as_deref().unwrap_or(
                            "the executor did not return a trustworthy terminal validation result",
                        ),
                    )),
                }
            }
            ShellCommandExecutionState::TimedOut => {
                payload["failure_kind"] = json!("timeout");
                ToolResult {
                    success: false,
                    output: payload,
                    error: Some(command_timeout_message(
                        timeout_secs,
                        &stdout_tail,
                        &stderr_tail,
                    )),
                }
            }
            ShellCommandExecutionState::Completed => {
                payload["failure_kind"] = json!(if validation_failed || process_passed {
                    CARGO_VALIDATION_FAILURE_KIND
                } else {
                    "process_exit"
                });
                let error = if process_passed {
                    terminal_test_count_assertion_failure_message(adapter.tool_identity(), &payload)
                } else {
                    format!("structured validation command completed with validation failure; {VALIDATION_FAILURE_GUIDANCE}")
                };
                ToolResult {
                    success: false,
                    output: payload,
                    error: Some(error),
                }
            }
        }
    }
}

/// Structured validation request carried through the read-only tool path.
pub(super) struct ValidationRunRequest<'a> {
    pub(super) project: String,
    pub(super) cwd: Option<String>,
    pub(super) require_tests: Option<bool>,
    pub(super) minimum_tests: Option<u64>,
    pub(super) timeout_secs: Option<u64>,
    pub(super) sync_wait_secs: Option<u64>,
    pub(super) session_id: Option<String>,
    pub(super) ssh_resource: Option<&'a str>,
    pub(super) auth: Option<&'a AuthContext>,
}

/// Job handoff / terminal projection metadata for a promoted validation.
struct ValidationHandoff {
    execution_source: String,
    purpose: String,
    job_id: String,
    effective_timeout_secs: u64,
    sync_wait_secs: u64,
    project: String,
    cwd: String,
    shell: String,
    executor: String,
    command_summary: String,
    minimum_tests: Option<u64>,
    require_tests: Option<bool>,
    no_run: Option<bool>,
    auth: Option<webcodex_runner_registry::RunnerAccess>,
}

fn validation_handoff_failure_result(
    failure: StructuredJobHandoffFailure,
    handoff: &ValidationHandoff,
) -> ToolResult {
    let mut result = failure.into_tool_result(
        &handoff.project,
        StructuredExecutionBudget {
            effective_timeout_secs: handoff.effective_timeout_secs,
            sync_wait_secs: handoff.sync_wait_secs,
        },
    );
    result.output["project"] = json!(handoff.project);
    result.output["passed"] = json!(false);
    result
}

pub(super) fn apply_validation_projection_fields(payload: &mut Value, projection: &Value) {
    for field in [
        "passed",
        "warnings_count",
        "errors_count",
        "tests_detected",
        "tests_run_count",
        "tests_passed",
        "tests_failed",
        "zero_tests_run",
        "require_tests",
        "no_run",
        "test_count_assertion",
        "diagnostics",
    ] {
        if let Some(value) = projection.get(field) {
            if field != "passed" || value.is_boolean() {
                payload[field] = value.clone();
            }
        }
    }
}

/// Cancellation guard for a hidden structured validation Job during its
/// synchronous wait window.
///
/// If the MCP request is cancelled before the tool returns, Drop synchronously
/// records cleanup intent in the registry and then triggers asynchronous stop
/// delivery. The registry lifecycle retries delayed intents; active records are
/// retained for reconciliation until a terminal Runner update confirms cleanup.
/// Once the tool has produced a terminal result or public handoff, the guard is
/// disarmed.
struct ValidationCleanupGuard {
    clients: std::sync::Arc<crate::runner_http::RunnerRegistry>,
    job_id: String,
    auth: Option<webcodex_runner_registry::RunnerAccess>,
    armed: bool,
}

impl ValidationCleanupGuard {
    fn new(
        clients: std::sync::Arc<crate::runner_http::RunnerRegistry>,
        job_id: String,
        auth: Option<webcodex_runner_registry::RunnerAccess>,
    ) -> Self {
        Self {
            clients,
            job_id,
            auth,
            armed: true,
        }
    }

    fn disarm_if_returnable(&mut self, result: &ToolResult) {
        if result.output["terminal"] == true || result.output["promoted_to_job"] == true {
            self.disarm();
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ValidationCleanupGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // Register intent before attempting any asynchronous work. This is the
        // durable in-process safety boundary: if the immediate processor is
        // delayed or the runtime is closing, the periodic registry lifecycle
        // still sees and retries the cleanup without deleting the active record.
        let job_id = self.job_id.clone();
        let auth = self.auth.clone();
        let clients = self.clients.clone();
        clients.record_hidden_cleanup_intent(job_id, auth);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                clients.process_hidden_cleanup_intents().await;
            });
        }
    }
}

pub(super) fn validation_handoff_execution_state(
    status: &str,
    started: bool,
) -> (&'static str, bool) {
    let pending_status = matches!(
        RunnerJobLifecycle::from_wire(status),
        Ok(RunnerJobLifecycle::Queued
            | RunnerJobLifecycle::RunnerQueued
            | RunnerJobLifecycle::StartedLegacy)
    );
    let command_started = !pending_status || started;
    (
        if command_started { "running" } else { "queued" },
        command_started,
    )
}

#[cfg(test)]
mod validation_handoff_projection_tests {
    use super::validation_handoff_execution_state;

    #[test]
    fn fresh_started_evidence_prevents_false_queued_validation_handoff() {
        assert_eq!(
            validation_handoff_execution_state("agent_queued", false),
            ("queued", false)
        );
        assert_eq!(
            validation_handoff_execution_state("agent_queued", true),
            ("running", true)
        );
        assert_eq!(
            validation_handoff_execution_state("started", true),
            ("running", true)
        );
        assert_eq!(
            validation_handoff_execution_state("running", true),
            ("running", true)
        );
    }
}

#[cfg(test)]
mod structured_cargo_arg_parity_tests {
    use super::*;

    #[test]
    fn cargo_test_count_inputs_resolve_to_the_stricter_bounded_minimum() {
        assert_eq!(resolve_cargo_test_minimum(None, None, None).unwrap(), None);
        assert_eq!(
            resolve_cargo_test_minimum(Some(false), None, None).unwrap(),
            None
        );
        assert_eq!(
            resolve_cargo_test_minimum(Some(true), None, None).unwrap(),
            Some(1)
        );
        assert_eq!(
            resolve_cargo_test_minimum(Some(true), Some(6), None).unwrap(),
            Some(6)
        );
        assert_eq!(
            resolve_cargo_test_minimum(Some(false), Some(6), None).unwrap(),
            Some(6)
        );
        for result in [
            resolve_cargo_test_minimum(None, Some(0), None),
            resolve_cargo_test_minimum(
                None,
                Some(crate::runner_protocol::CARGO_TEST_MIN_TESTS_MAX + 1),
                None,
            ),
            resolve_cargo_test_minimum(Some(true), None, Some(true)),
            resolve_cargo_test_minimum(None, Some(6), Some(true)),
        ] {
            let rejection = result.expect_err("invalid assertion input must fail closed");
            assert!(!rejection.success);
            assert_eq!(rejection.output["execution_state"], "not_started");
            assert_eq!(rejection.output["command_started"], false);
            assert_eq!(rejection.output["failure_kind"], "invalid_arguments");
        }
    }

    /// One adapter-owned plan must preserve both the legacy command projection
    /// and the canonical Job argv. Exact expected values stay independent of
    /// the planner so a shared producer bug cannot make this test self-fulfilling.
    #[test]
    fn read_only_plan_preserves_exact_command_and_structured_argv() {
        for (tool_name, options, expected_command, expected_args) in [
            (
                "cargo_check",
                ValidationCommandOptions {
                    all_targets: Some(true),
                    all_features: Some(true),
                    no_default_features: Some(true),
                    features: Some("  serde  ".to_string()),
                    package: Some("  my-crate  ".to_string()),
                    ..ValidationCommandOptions::default()
                },
                "cargo check --all-targets --all-features --no-default-features --features 'serde' -p 'my-crate'",
                vec![
                    "check",
                    "--all-targets",
                    "--all-features",
                    "--no-default-features",
                    "--features",
                    "serde",
                    "-p",
                    "my-crate",
                ],
            ),
            (
                "cargo_test",
                ValidationCommandOptions {
                    filter: Some("  module::nested::test  ".to_string()),
                    lib: Some(true),
                    all_targets: Some(true),
                    all_features: Some(true),
                    no_default_features: Some(true),
                    features: Some("  a  b  ".to_string()),
                    package: Some("  my-crate  ".to_string()),
                    no_run: Some(true),
                    ..ValidationCommandOptions::default()
                },
                "cargo test 'module::nested::test' --lib --all-targets --all-features --no-default-features --features 'a  b' -p 'my-crate' --no-run",
                vec![
                    "test",
                    "module::nested::test",
                    "--lib",
                    "--all-targets",
                    "--all-features",
                    "--no-default-features",
                    "--features",
                    "a  b",
                    "-p",
                    "my-crate",
                    "--no-run",
                ],
            ),
        ] {
            let adapter = validation_adapter_for_tool(tool_name).unwrap();
            let plan = adapter
                .build_readonly_plan(options)
                .unwrap_or_else(|error| panic!("{tool_name} plan build: {error}"));
            assert_eq!(plan.compatibility_command, expected_command, "{tool_name}");
            assert_eq!(
                plan.structured_step.args,
                expected_args
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
                "{tool_name}"
            );
            assert!(plan.structured_step.is_canonical(), "{tool_name} step must be canonical");
        }
    }

    #[test]
    fn cargo_test_lib_false_and_omission_share_one_plan() {
        let adapter = validation_adapter_for_tool("cargo_test").unwrap();
        let omitted = adapter
            .build_readonly_plan(ValidationCommandOptions::default())
            .unwrap();
        let explicit_false = adapter
            .build_readonly_plan(ValidationCommandOptions {
                lib: Some(false),
                ..ValidationCommandOptions::default()
            })
            .unwrap();

        assert_eq!(omitted, explicit_false);
        assert_eq!(omitted.compatibility_command, "cargo test");
        assert_eq!(omitted.structured_step.args, vec!["test"]);
    }

    #[test]
    fn go_test_read_only_plan_preserves_package_projection() {
        let options = ValidationCommandOptions {
            go_packages: Some(vec![
                "./internal/control".to_string(),
                "./internal/node".to_string(),
            ]),
            ..ValidationCommandOptions::default()
        };
        let adapter = validation_adapter_for_tool("go_test").unwrap();
        let plan = adapter.build_readonly_plan(options).unwrap();
        assert_eq!(
            plan.compatibility_command,
            "go test -json './internal/control' './internal/node'"
        );
        assert_eq!(plan.structured_step.name, "test");
        assert_eq!(plan.structured_step.program, "go");
        assert_eq!(
            plan.structured_step.args,
            vec!["test", "-json", "./internal/control", "./internal/node"]
        );
        assert!(plan.structured_step.env.is_empty());
        assert!(plan.structured_step.is_structured_go_test_json());
        assert!(plan.structured_step.is_canonical());
    }

    #[test]
    fn invalid_cargo_values_fail_before_a_read_only_plan_exists() {
        for (tool_name, invalid) in [
            (
                "cargo_check",
                ValidationCommandOptions {
                    features: Some("--no-run".to_string()),
                    ..ValidationCommandOptions::default()
                },
            ),
            (
                "cargo_check",
                ValidationCommandOptions {
                    package: Some("--all-features".to_string()),
                    ..ValidationCommandOptions::default()
                },
            ),
            (
                "cargo_check",
                ValidationCommandOptions {
                    features: Some("line\nbreak".to_string()),
                    ..ValidationCommandOptions::default()
                },
            ),
            (
                "cargo_check",
                ValidationCommandOptions {
                    features: Some("a".repeat(crate::runner_protocol::CARGO_VALUE_MAX_BYTES + 1)),
                    ..ValidationCommandOptions::default()
                },
            ),
            (
                "cargo_test",
                ValidationCommandOptions {
                    filter: Some("--all-features".to_string()),
                    ..ValidationCommandOptions::default()
                },
            ),
            (
                "cargo_test",
                ValidationCommandOptions {
                    filter: Some("line\nbreak".to_string()),
                    ..ValidationCommandOptions::default()
                },
            ),
        ] {
            let adapter = validation_adapter_for_tool(tool_name).unwrap();
            assert!(
                adapter.build_readonly_plan(invalid.clone()).is_err(),
                "{tool_name} plan must reject {invalid:?}"
            );
            assert!(
                adapter.build_command(invalid.clone()).is_err(),
                "{tool_name} compatibility command must reject {invalid:?}"
            );
        }
    }

    #[test]
    fn whitespace_only_values_are_omitted_from_both_plan_projections() {
        for (tool_name, options, expected_command, expected_args) in [
            (
                "cargo_check",
                ValidationCommandOptions {
                    features: Some("   ".to_string()),
                    package: Some("   ".to_string()),
                    ..ValidationCommandOptions::default()
                },
                "cargo check --all-targets",
                vec!["check", "--all-targets"],
            ),
            (
                "cargo_test",
                ValidationCommandOptions {
                    filter: Some("   ".to_string()),
                    features: Some("   ".to_string()),
                    ..ValidationCommandOptions::default()
                },
                "cargo test",
                vec!["test"],
            ),
        ] {
            let adapter = validation_adapter_for_tool(tool_name).unwrap();
            let plan = adapter.build_readonly_plan(options).unwrap();
            assert_eq!(plan.compatibility_command, expected_command, "{tool_name}");
            assert_eq!(
                plan.structured_step.args,
                expected_args
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
                "{tool_name}"
            );
            assert!(
                plan.structured_step.is_canonical(),
                "{tool_name} step must be canonical"
            );
        }
    }
}

fn sanitize_project_validation_output(value: &mut Value) {
    match value {
        Value::String(text) => *text = webcodex_core::validation_bridge::sanitize_bridge_text(text),
        Value::Array(items) => items
            .iter_mut()
            .for_each(sanitize_project_validation_output),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(sanitize_project_validation_output),
        _ => {}
    }
}

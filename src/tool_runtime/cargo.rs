use super::helpers::{
    command_rejected_message, resolve_sync_timeout_secs, sync_timeout_out_of_range_result,
    DEFAULT_CARGO_FMT_TIMEOUT_SECS,
};
use super::tool_result::ToolResult;
pub(crate) use super::validation::count_rustc_diagnostics;
use super::validation::{
    annotate_cargo_fmt_effect, append_cargo_fmt_uncertainty_guidance,
    cargo_fmt_check_is_stable_diff, reject_structured_validation_ssh_resource,
    resolve_cargo_test_minimum, validate_cwd, ValidationRunRequest,
};
use super::validation_profile::{validation_adapter_for_tool, ValidationCommandOptions};
use super::ToolRuntime;
use crate::auth::AuthContext;
use crate::runner_protocol::ShellCommandExecutionState;
use serde_json::json;
use std::time::{Duration, Instant};
pub(crate) use webcodex_validation::parse_cargo_test_run_metadata;

impl ToolRuntime {
    #[cfg(test)]
    pub(crate) async fn cargo_fmt(
        &self,
        project: String,
        cwd: Option<String>,
        check: Option<bool>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        self.cargo_fmt_with_context_inner(project, cwd, check, timeout_secs, None, None, None, None)
            .await
    }

    /// Entry used by dispatch: carries the Session/execution-context and auth
    /// so a long validation can promote to a Job that inherits the original
    /// Session ownership.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn cargo_fmt_with_context(
        &self,
        project: String,
        cwd: Option<String>,
        check: Option<bool>,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        session_id: Option<String>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.cargo_fmt_with_context_inner(
            project,
            cwd,
            check,
            timeout_secs,
            sync_wait_secs,
            session_id,
            ssh_resource,
            auth,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn cargo_fmt_with_context_inner(
        &self,
        project: String,
        cwd: Option<String>,
        check: Option<bool>,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        session_id: Option<String>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let check = check.unwrap_or(false);
        // `sync_wait_secs` controls read-only Job handoff only. Ensure-format accepts
        // the field for caller-shape compatibility but intentionally ignores it:
        // mutation remains synchronous and `timeout_secs` remains the full budget.
        // Both read-only and mutating structured Cargo formatting reject named
        // SSH resources before selecting an execution path. In particular, the
        // mutating sync path must never fall back to the Runner project root.
        if let Some(result) = reject_structured_validation_ssh_resource(ssh_resource) {
            return result;
        }
        // Non-check `cargo_fmt` is an ensure-formatted operation. It first runs
        // the read-only rustfmt check, mutating only when that check proves a
        // stable formatting diff. The mutation stays synchronous and never
        // continues after this ToolResult returns.
        if !check {
            let timeout =
                match resolve_sync_timeout_secs(timeout_secs, DEFAULT_CARGO_FMT_TIMEOUT_SECS) {
                    Ok(timeout) => timeout,
                    Err(_) => {
                        return sync_timeout_out_of_range_result(
                            "cargo_fmt",
                            DEFAULT_CARGO_FMT_TIMEOUT_SECS,
                        )
                    }
                };
            let cwd = match validate_cwd(cwd) {
                Ok(cwd) => cwd,
                Err(e) => {
                    return ToolResult::err(command_rejected_message(
                        e,
                        "choose an existing project-relative cwd, then retry.",
                    ))
                }
            };
            let config = match self.resolve_project(&project).await {
                Ok(config) => config,
                Err(e) => {
                    return ToolResult::err(command_rejected_message(
                        e.to_message(),
                        "verify the project id/cwd and agent connectivity, then retry.",
                    ))
                }
            };
            let adapter = validation_adapter_for_tool("cargo_fmt")
                .expect("Rust validation profile must register cargo_fmt");
            let check_command = adapter
                .build_command(ValidationCommandOptions {
                    check: true,
                    ..ValidationCommandOptions::default()
                })
                .expect("cargo_fmt check command builder is infallible");
            let started = Instant::now();
            let check_output = match self
                .run_project_command_capture(&project, check_command.clone(), timeout, cwd.clone())
                .await
            {
                Ok(output) => output,
                Err(e) => {
                    return ToolResult::err(command_rejected_message(
                        e,
                        "verify the project id/cwd and agent connectivity, then retry.",
                    ))
                }
            };
            let already_formatted = check_output.execution_state
                == ShellCommandExecutionState::Completed
                && check_output.exit_code == Some(0);
            if already_formatted {
                let mut result = self
                    .build_cargo_result(
                        &project,
                        &check_command,
                        cwd.as_deref(),
                        &config,
                        adapter,
                        check_output,
                        timeout,
                        0,
                        false,
                        false,
                        false,
                        None,
                        None,
                        None,
                    )
                    .await;
                annotate_cargo_fmt_effect(&mut result, Some(false), Some(false));
                return result;
            }
            if !cargo_fmt_check_is_stable_diff(adapter, &check_output) {
                let mut result = self
                    .build_cargo_result(
                        &project,
                        &check_command,
                        cwd.as_deref(),
                        &config,
                        adapter,
                        check_output,
                        timeout,
                        0,
                        false,
                        false,
                        false,
                        None,
                        None,
                        None,
                    )
                    .await;
                annotate_cargo_fmt_effect(&mut result, Some(false), Some(false));
                if let Some(error) = result.error.take() {
                    result.error = Some(format!(
                        "{error} No source formatting was attempted because the precheck did not prove a stable rustfmt diff."
                    ));
                }
                return result;
            }

            let total_budget = Duration::from_secs(timeout);
            let elapsed = started.elapsed();
            let remaining_budget = total_budget.saturating_sub(elapsed);
            if remaining_budget < Duration::from_secs(1) {
                let mut result = self
                    .build_cargo_result(
                        &project,
                        &check_command,
                        cwd.as_deref(),
                        &config,
                        adapter,
                        check_output,
                        timeout,
                        0,
                        false,
                        false,
                        false,
                        None,
                        None,
                        None,
                    )
                    .await;
                annotate_cargo_fmt_effect(&mut result, Some(false), Some(false));
                result.output["failure_kind"] = json!("timeout");
                result.error = Some(
                    "cargo_fmt found formatting differences but exhausted its synchronous timeout before mutation; no source formatting was attempted."
                        .to_string(),
                );
                return result;
            }
            let remaining = remaining_budget.as_secs();
            let command = adapter
                .build_command(ValidationCommandOptions {
                    check: false,
                    ..ValidationCommandOptions::default()
                })
                .expect("cargo_fmt command builder is infallible");
            let mutation_output = match self
                .run_project_command_capture(&project, command.clone(), remaining, cwd.clone())
                .await
            {
                Ok(output) => output,
                Err(e) => {
                    return ToolResult::err(command_rejected_message(
                        e,
                        "the formatting precheck proved a diff but the mutation command could not be dispatched; inspect the worktree before retrying.",
                    ))
                }
            };
            let mutation_state = mutation_output.execution_state;
            let mutation_exit_code = mutation_output.exit_code;
            let mut result = self
                .build_cargo_result(
                    &project,
                    &command,
                    cwd.as_deref(),
                    &config,
                    adapter,
                    mutation_output,
                    remaining,
                    0,
                    false,
                    false,
                    false,
                    None,
                    None,
                    None,
                )
                .await;
            result.output["effective_timeout_secs"] = json!(timeout);
            result.output["duration_ms"] = json!(started.elapsed().as_millis() as u64);
            match (mutation_state, mutation_exit_code) {
                (ShellCommandExecutionState::Completed, Some(0)) => {
                    annotate_cargo_fmt_effect(&mut result, Some(true), Some(true));
                }
                (ShellCommandExecutionState::NotStarted, _) => {
                    annotate_cargo_fmt_effect(&mut result, Some(false), Some(false));
                }
                _ => {
                    annotate_cargo_fmt_effect(&mut result, None, None);
                    append_cargo_fmt_uncertainty_guidance(&mut result);
                }
            }
            result
        } else {
            self.run_readonly_validation(
                "cargo_fmt",
                ValidationRunRequest {
                    project,
                    cwd,
                    check: true,
                    filter: None,
                    lib: None,
                    all_targets: None,
                    all_features: None,
                    no_default_features: None,
                    features: None,
                    package: None,
                    cargo_packages: None,
                    no_run: None,
                    require_tests: None,
                    minimum_tests: None,
                    go_packages: None,
                    timeout_secs,
                    sync_wait_secs,
                    session_id,
                    ssh_resource,
                    auth,
                },
            )
            .await
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn cargo_check(
        &self,
        project: String,
        cwd: Option<String>,
        all_targets: Option<bool>,
        all_features: Option<bool>,
        no_default_features: Option<bool>,
        features: Option<String>,
        package: Option<String>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        let packages =
            match crate::runner_protocol::normalize_cargo_packages(package.as_deref(), None) {
                Ok(packages) => packages,
                Err(error) => return ToolResult::err(error),
            };
        self.cargo_check_with_context_inner(
            project,
            cwd,
            all_targets,
            all_features,
            no_default_features,
            features,
            packages,
            timeout_secs,
            None,
            None,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn cargo_check_with_context(
        &self,
        project: String,
        cwd: Option<String>,
        all_targets: Option<bool>,
        all_features: Option<bool>,
        no_default_features: Option<bool>,
        features: Option<String>,
        packages: Option<Vec<String>>,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        session_id: Option<String>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.cargo_check_with_context_inner(
            project,
            cwd,
            all_targets,
            all_features,
            no_default_features,
            features,
            packages,
            timeout_secs,
            sync_wait_secs,
            session_id,
            ssh_resource,
            auth,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn cargo_check_with_context_inner(
        &self,
        project: String,
        cwd: Option<String>,
        all_targets: Option<bool>,
        all_features: Option<bool>,
        no_default_features: Option<bool>,
        features: Option<String>,
        packages: Option<Vec<String>>,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        session_id: Option<String>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.run_readonly_validation(
            "cargo_check",
            ValidationRunRequest {
                project,
                cwd,
                check: false,
                filter: None,
                lib: None,
                all_targets,
                all_features,
                no_default_features,
                features,
                package: None,
                cargo_packages: packages,
                no_run: None,
                require_tests: None,
                minimum_tests: None,
                go_packages: None,
                timeout_secs,
                sync_wait_secs,
                session_id,
                ssh_resource,
                auth,
            },
        )
        .await
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn cargo_test(
        &self,
        project: String,
        cwd: Option<String>,
        filter: Option<String>,
        all_targets: Option<bool>,
        all_features: Option<bool>,
        no_default_features: Option<bool>,
        features: Option<String>,
        package: Option<String>,
        no_run: Option<bool>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        self.cargo_test_with_context_inner(
            project,
            cwd,
            filter,
            None,
            all_targets,
            all_features,
            no_default_features,
            features,
            package,
            no_run,
            None,
            None,
            timeout_secs,
            None,
            None,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn cargo_test_with_context(
        &self,
        project: String,
        cwd: Option<String>,
        filter: Option<String>,
        lib: Option<bool>,
        all_targets: Option<bool>,
        all_features: Option<bool>,
        no_default_features: Option<bool>,
        features: Option<String>,
        package: Option<String>,
        no_run: Option<bool>,
        require_tests: Option<bool>,
        min_tests: Option<u64>,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        session_id: Option<String>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.cargo_test_with_context_inner(
            project,
            cwd,
            filter,
            lib,
            all_targets,
            all_features,
            no_default_features,
            features,
            package,
            no_run,
            require_tests,
            min_tests,
            timeout_secs,
            sync_wait_secs,
            session_id,
            ssh_resource,
            auth,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn cargo_test_with_context_inner(
        &self,
        project: String,
        cwd: Option<String>,
        filter: Option<String>,
        lib: Option<bool>,
        all_targets: Option<bool>,
        all_features: Option<bool>,
        no_default_features: Option<bool>,
        features: Option<String>,
        package: Option<String>,
        no_run: Option<bool>,
        require_tests: Option<bool>,
        min_tests: Option<u64>,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        session_id: Option<String>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let minimum_tests = match resolve_cargo_test_minimum(require_tests, min_tests, no_run) {
            Ok(minimum) => minimum,
            Err(result) => return result,
        };
        self.run_readonly_validation(
            "cargo_test",
            ValidationRunRequest {
                project,
                cwd,
                check: false,
                filter,
                lib,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                cargo_packages: None,
                no_run,
                require_tests,
                minimum_tests,
                go_packages: None,
                timeout_secs,
                sync_wait_secs,
                session_id,
                ssh_resource,
                auth,
            },
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn go_test(
        &self,
        project: String,
        cwd: Option<String>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        self.go_test_with_context(project, cwd, None, timeout_secs, None, None, None, None)
            .await
    }

    pub(crate) async fn go_test_with_context(
        &self,
        project: String,
        cwd: Option<String>,
        packages: Option<Vec<String>>,
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        session_id: Option<String>,
        ssh_resource: Option<&str>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.run_readonly_validation(
            "go_test",
            ValidationRunRequest {
                project,
                cwd,
                check: false,
                filter: None,
                lib: None,
                all_targets: None,
                all_features: None,
                no_default_features: None,
                features: None,
                package: None,
                cargo_packages: None,
                no_run: None,
                require_tests: None,
                minimum_tests: None,
                go_packages: packages,
                timeout_secs,
                sync_wait_secs,
                session_id,
                ssh_resource,
                auth,
            },
        )
        .await
    }
}

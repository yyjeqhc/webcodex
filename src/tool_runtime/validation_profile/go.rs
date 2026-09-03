use super::{ValidationAdapter, ValidationCommandOptions, ValidationFailureEvidence};
use crate::tool_runtime::helpers::shell_escape_simple;
use crate::tool_runtime::validation_parser::{parse_go_test_diagnostics, ValidationDiagnostics};

struct GoTestValidationAdapter;

static GO_TEST_ADAPTER: GoTestValidationAdapter = GoTestValidationAdapter;

pub(super) fn validation_adapter(tool_identity: &str) -> Option<&'static dyn ValidationAdapter> {
    (tool_identity == "go_test").then_some(&GO_TEST_ADAPTER)
}

impl ValidationAdapter for GoTestValidationAdapter {
    fn validation_kind(&self) -> &'static str {
        "test"
    }

    fn tool_identity(&self) -> &'static str {
        "go_test"
    }

    fn build_command(&self, options: ValidationCommandOptions) -> Result<String, String> {
        if options.check
            || options.filter.is_some()
            || options.all_targets.is_some()
            || options.all_features.is_some()
            || options.no_default_features.is_some()
            || options.features.is_some()
            || options.package.is_some()
            || options.no_run.is_some()
        {
            return Err("go_test does not accept Cargo validation command options".to_string());
        }
        let Some(packages) = options.go_packages.as_deref() else {
            return Ok("go test -json ./...".to_string());
        };
        let packages = crate::runner_protocol::normalize_go_test_packages(Some(packages))
            .map_err(|reason| format!("packages {reason}"))?;
        let mut command = vec!["go".to_string(), "test".to_string(), "-json".to_string()];
        command.extend(packages.iter().map(|package| shell_escape_simple(package)));
        Ok(command.join(" "))
    }

    fn parse(
        &self,
        stdout_excerpt: &str,
        _stderr_excerpt: &str,
        truncated: bool,
    ) -> ValidationDiagnostics {
        parse_go_test_diagnostics(stdout_excerpt, truncated)
    }

    fn map_failure_kind(&self, evidence: ValidationFailureEvidence<'_>) -> &'static str {
        if evidence.success {
            return "unknown";
        }
        if matches!(
            evidence.reported_failure_kind,
            Some("timeout" | "timed_out" | "command_timeout")
        ) {
            return "timeout";
        }
        if evidence.diagnostics.is_some_and(|diagnostics| {
            diagnostics
                .test_summary
                .as_ref()
                .and_then(|summary| summary.failed)
                .is_some_and(|failed| failed > 0)
                || !diagnostics.failed_test_details.is_empty()
        }) {
            return "test_failure";
        }
        if evidence.exit_code.is_some_and(|exit_code| exit_code != 0)
            || matches!(
                evidence.reported_failure_kind,
                Some(
                    "command_exit_nonzero"
                        | "command_spawn_failed"
                        | "command_wait_failed"
                        | "command_output_failed"
                )
            )
        {
            return "process_exit";
        }
        "unknown"
    }

    fn reports_test_run_metadata(&self) -> bool {
        true
    }
}

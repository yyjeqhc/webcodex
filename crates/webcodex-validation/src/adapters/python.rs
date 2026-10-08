use super::{
    read_only_validation_plan, ReadOnlyValidationPlan, ValidationAdapter, ValidationCommandOptions,
    ValidationEvidenceProfile, ValidationFailureEvidence, ValidationPlanArg,
};
use webcodex_core::validation_evidence::{parse_pytest_diagnostics, ValidationDiagnostics};

struct PytestValidationAdapter;
static PYTEST_ADAPTER: PytestValidationAdapter = PytestValidationAdapter;
pub(super) fn test_adapter() -> &'static dyn ValidationAdapter {
    &PYTEST_ADAPTER
}

impl ValidationEvidenceProfile for PytestValidationAdapter {
    fn validation_kind(&self) -> &'static str {
        "test"
    }

    fn tool_identity(&self) -> &'static str {
        "python:pytest:test"
    }

    fn parse(&self, stdout: &str, _stderr: &str, truncated: bool) -> ValidationDiagnostics {
        parse_pytest_diagnostics(stdout, truncated)
    }

    fn reports_test_run_metadata(&self) -> bool {
        true
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
        if evidence.exit_code == Some(1) {
            "test_failure"
        } else {
            "process_exit"
        }
    }
}

impl ValidationAdapter for PytestValidationAdapter {
    fn build_readonly_plan(
        &self,
        options: ValidationCommandOptions,
    ) -> Result<ReadOnlyValidationPlan, String> {
        if options.check
            || options.lib.is_some()
            || options.all_targets.is_some()
            || options.all_features.is_some()
            || options.no_default_features.is_some()
            || options.features.is_some()
            || options.package.is_some()
            || options.cargo_packages.is_some()
            || options.all_packages
            || options.go_packages.is_some()
            || options.no_run.is_some()
            || options.dependency_mode.is_some()
        {
            return Err("pytest accepts only its bounded test filter".into());
        }
        let mut args = ["-m", "pytest", "--color=no", "-rA"]
            .into_iter()
            .map(ValidationPlanArg::Literal)
            .collect::<Vec<_>>();
        if let Some(filter) = options
            .filter
            .as_deref()
            .map(webcodex_core::runner_protocol::normalize_pytest_filter)
            .transpose()?
            .flatten()
        {
            args.push(ValidationPlanArg::Literal("-k"));
            args.push(ValidationPlanArg::Value(filter));
        }
        read_only_validation_plan("test", "python", args)
    }
}

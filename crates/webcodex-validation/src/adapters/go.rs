use super::{
    read_only_validation_plan, ReadOnlyValidationPlan, ValidationAdapter, ValidationCommandOptions,
    ValidationFailureEvidence, ValidationPlanArg,
};
use webcodex_core::runner_protocol::normalize_go_test_packages;
use webcodex_core::validation_evidence::{parse_go_test_diagnostics, ValidationDiagnostics};

struct GoTestValidationAdapter;

static GO_TEST_ADAPTER: GoTestValidationAdapter = GoTestValidationAdapter;

pub(super) fn validation_adapter(tool_identity: &str) -> Option<&'static dyn ValidationAdapter> {
    match tool_identity {
        "go_test" => Some(&GO_TEST_ADAPTER),
        "go_vet" => Some(&GO_VET_ADAPTER),
        _ => None,
    }
}

pub(super) fn test_adapter() -> &'static dyn ValidationAdapter {
    &GO_TEST_ADAPTER
}

impl ValidationAdapter for GoTestValidationAdapter {
    fn validation_kind(&self) -> &'static str {
        "test"
    }

    fn tool_identity(&self) -> &'static str {
        "go_test"
    }

    fn build_readonly_plan(
        &self,
        options: ValidationCommandOptions,
    ) -> Result<ReadOnlyValidationPlan, String> {
        if options.check
            || options.filter.is_some()
            || options.lib.is_some()
            || options.all_targets.is_some()
            || options.all_features.is_some()
            || options.no_default_features.is_some()
            || options.features.is_some()
            || options.package.is_some()
            || options.no_run.is_some()
        {
            return Err("go_test does not accept Cargo validation command options".to_string());
        }
        let explicit_packages = options.go_packages.is_some();
        let packages = normalize_go_test_packages(options.go_packages.as_deref())
            .map_err(|reason| format!("packages {reason}"))?;
        let mut args = vec![
            ValidationPlanArg::Literal("test"),
            ValidationPlanArg::Literal("-json"),
        ];
        if explicit_packages {
            args.extend(packages.into_iter().map(ValidationPlanArg::Value));
        } else {
            debug_assert_eq!(packages.as_slice(), ["./..."]);
            args.push(ValidationPlanArg::Literal("./..."));
        }
        read_only_validation_plan("test", "go", args)
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

struct GoVetValidationAdapter;
static GO_VET_ADAPTER: GoVetValidationAdapter = GoVetValidationAdapter;
impl ValidationAdapter for GoVetValidationAdapter {
    fn validation_kind(&self) -> &'static str {
        "check"
    }
    fn tool_identity(&self) -> &'static str {
        "go_vet"
    }
    fn build_readonly_plan(
        &self,
        _options: ValidationCommandOptions,
    ) -> Result<ReadOnlyValidationPlan, String> {
        read_only_validation_plan(
            "check",
            "go",
            vec![
                ValidationPlanArg::Literal("vet"),
                ValidationPlanArg::Literal("./..."),
            ],
        )
    }
    fn parse(&self, _stdout: &str, stderr: &str, truncated: bool) -> ValidationDiagnostics {
        webcodex_core::validation_evidence::parse_go_vet_diagnostics(stderr, truncated)
    }
    fn map_failure_kind(&self, evidence: ValidationFailureEvidence<'_>) -> &'static str {
        if evidence.success {
            "unknown"
        } else if matches!(
            evidence.reported_failure_kind,
            Some("timeout" | "timed_out" | "command_timeout")
        ) {
            "timeout"
        } else {
            "compile_error"
        }
    }
}

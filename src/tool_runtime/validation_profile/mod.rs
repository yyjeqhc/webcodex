use super::helpers::{
    DEFAULT_CARGO_CHECK_TIMEOUT_SECS, DEFAULT_CARGO_FMT_TIMEOUT_SECS,
    DEFAULT_CARGO_TEST_TIMEOUT_SECS,
};

pub(crate) use webcodex_validation::{
    validation_adapter_for_tool, CargoCheckOptions, CargoReadOnlyValidationOperation,
    CargoTestOptions, GoReadOnlyValidationOperation, GoTestOptions, ReadOnlyValidationOperation,
    ValidationAdapter, ValidationCommandOptions, ValidationFailureEvidence,
};

pub(crate) struct ValidationRuntimeProfile {
    pub default_timeout_secs: u64,
    pub force_agent_handoff: bool,
    pub invalid_argument_guidance: &'static str,
}

pub(crate) fn runtime_profile(operation: &ReadOnlyValidationOperation) -> ValidationRuntimeProfile {
    match operation {
        ReadOnlyValidationOperation::Cargo(CargoReadOnlyValidationOperation::FormatCheck) => {
            ValidationRuntimeProfile {
                default_timeout_secs: DEFAULT_CARGO_FMT_TIMEOUT_SECS,
                force_agent_handoff: false,
                invalid_argument_guidance: "fix the cargo argument format, then retry.",
            }
        }
        ReadOnlyValidationOperation::Cargo(CargoReadOnlyValidationOperation::Check(_)) => {
            ValidationRuntimeProfile {
                default_timeout_secs: DEFAULT_CARGO_CHECK_TIMEOUT_SECS,
                force_agent_handoff: false,
                invalid_argument_guidance: "fix the cargo argument format, then retry.",
            }
        }
        ReadOnlyValidationOperation::Cargo(CargoReadOnlyValidationOperation::Test(_)) => {
            ValidationRuntimeProfile {
                default_timeout_secs: DEFAULT_CARGO_TEST_TIMEOUT_SECS,
                force_agent_handoff: false,
                invalid_argument_guidance: "fix the cargo argument format, then retry.",
            }
        }
        ReadOnlyValidationOperation::Go(GoReadOnlyValidationOperation::Check(_)) => {
            ValidationRuntimeProfile {
                default_timeout_secs: DEFAULT_CARGO_CHECK_TIMEOUT_SECS,
                force_agent_handoff: false,
                invalid_argument_guidance: "fix the Go package pattern format, then retry.",
            }
        }
        ReadOnlyValidationOperation::Go(GoReadOnlyValidationOperation::Test(_)) => {
            ValidationRuntimeProfile {
                default_timeout_secs: DEFAULT_CARGO_TEST_TIMEOUT_SECS,
                force_agent_handoff: true,
                invalid_argument_guidance: "fix the Go package pattern format, then retry.",
            }
        }
    }
}

pub(crate) fn requires_multi_package_cargo_check(operation: &ReadOnlyValidationOperation) -> bool {
    match operation {
        ReadOnlyValidationOperation::Cargo(CargoReadOnlyValidationOperation::Check(options)) => {
            crate::runner_protocol::normalize_cargo_packages(
                options.package.as_deref(),
                options.packages.as_deref(),
            )
            .is_ok_and(|packages| packages.is_some_and(|packages| packages.len() > 1))
        }
        _ => false,
    }
}

//! Structured Cargo and Go validation adapters.

mod go;
mod rust;

use webcodex_core::runner_protocol::ShellJobValidationStep;
use webcodex_core::shell_quote::shell_escape_simple;
use webcodex_core::validation_evidence::ValidationDiagnostics;
use webcodex_core::validation_identity::ToolValidationIdentityKind;
use webcodex_core::workflow_session_contract::ExecutionPurpose;

#[derive(Debug, Clone, Default)]
pub struct ValidationCommandOptions {
    pub check: bool,
    pub filter: Option<String>,
    pub lib: Option<bool>,
    pub all_targets: Option<bool>,
    pub all_features: Option<bool>,
    pub no_default_features: Option<bool>,
    pub features: Option<String>,
    pub package: Option<String>,
    /// Canonical sorted, duplicate-free package scope for `cargo_check`.
    pub cargo_packages: Option<Vec<String>>,
    pub no_run: Option<bool>,
    /// First-class `go_test` package scope. Other validation adapters must
    /// reject this Go-specific option rather than silently ignoring it.
    pub go_packages: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CargoCheckOptions {
    pub all_targets: Option<bool>,
    pub all_features: Option<bool>,
    pub no_default_features: Option<bool>,
    pub features: Option<String>,
    pub package: Option<String>,
    pub packages: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CargoTestOptions {
    pub filter: Option<String>,
    pub lib: Option<bool>,
    pub all_targets: Option<bool>,
    pub all_features: Option<bool>,
    pub no_default_features: Option<bool>,
    pub features: Option<String>,
    pub package: Option<String>,
    pub no_run: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoTestOptions {
    pub packages: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoReadOnlyValidationOperation {
    FormatCheck,
    Check(CargoCheckOptions),
    Test(CargoTestOptions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoReadOnlyValidationOperation {
    Test(GoTestOptions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnlyValidationOperation {
    Cargo(CargoReadOnlyValidationOperation),
    Go(GoReadOnlyValidationOperation),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationCompatibilityProfile {
    pub tool_identity: &'static str,
    pub validation_identity: ToolValidationIdentityKind,
}

impl ReadOnlyValidationOperation {
    pub fn compatibility_profile(&self) -> ValidationCompatibilityProfile {
        match self {
            Self::Cargo(CargoReadOnlyValidationOperation::FormatCheck) => {
                ValidationCompatibilityProfile {
                    tool_identity: "cargo_fmt",
                    validation_identity: ToolValidationIdentityKind::CargoFmt,
                }
            }
            Self::Cargo(CargoReadOnlyValidationOperation::Check(_)) => {
                ValidationCompatibilityProfile {
                    tool_identity: "cargo_check",
                    validation_identity: ToolValidationIdentityKind::CargoCheck,
                }
            }
            Self::Cargo(CargoReadOnlyValidationOperation::Test(_)) => {
                ValidationCompatibilityProfile {
                    tool_identity: "cargo_test",
                    validation_identity: ToolValidationIdentityKind::CargoTest,
                }
            }
            Self::Go(GoReadOnlyValidationOperation::Test(_)) => ValidationCompatibilityProfile {
                tool_identity: "go_test",
                validation_identity: ToolValidationIdentityKind::GoTest,
            },
        }
    }

    pub fn adapter(&self) -> &'static dyn ValidationAdapter {
        match self {
            Self::Cargo(CargoReadOnlyValidationOperation::FormatCheck) => rust::format_adapter(),
            Self::Cargo(CargoReadOnlyValidationOperation::Check(_)) => rust::check_adapter(),
            Self::Cargo(CargoReadOnlyValidationOperation::Test(_)) => rust::test_adapter(),
            Self::Go(GoReadOnlyValidationOperation::Test(_)) => go::test_adapter(),
        }
    }

    pub fn build_readonly_plan(&self) -> Result<ReadOnlyValidationPlan, String> {
        match self {
            Self::Cargo(CargoReadOnlyValidationOperation::FormatCheck) => self
                .adapter()
                .build_readonly_plan(ValidationCommandOptions {
                    check: true,
                    ..ValidationCommandOptions::default()
                }),
            Self::Cargo(CargoReadOnlyValidationOperation::Check(options)) => self
                .adapter()
                .build_readonly_plan(ValidationCommandOptions::from(options.clone())),
            Self::Cargo(CargoReadOnlyValidationOperation::Test(options)) => self
                .adapter()
                .build_readonly_plan(ValidationCommandOptions::from(options.clone())),
            Self::Go(GoReadOnlyValidationOperation::Test(options)) => self
                .adapter()
                .build_readonly_plan(ValidationCommandOptions::from(options.clone())),
        }
    }
}

impl From<CargoCheckOptions> for ValidationCommandOptions {
    fn from(options: CargoCheckOptions) -> Self {
        Self {
            all_targets: options.all_targets,
            all_features: options.all_features,
            no_default_features: options.no_default_features,
            features: options.features,
            package: options.package,
            cargo_packages: options.packages,
            ..Self::default()
        }
    }
}

impl From<CargoTestOptions> for ValidationCommandOptions {
    fn from(options: CargoTestOptions) -> Self {
        Self {
            filter: options.filter,
            lib: options.lib,
            all_targets: options.all_targets,
            all_features: options.all_features,
            no_default_features: options.no_default_features,
            features: options.features,
            package: options.package,
            no_run: options.no_run,
            ..Self::default()
        }
    }
}

impl From<GoTestOptions> for ValidationCommandOptions {
    fn from(options: GoTestOptions) -> Self {
        Self {
            go_packages: options.packages,
            ..Self::default()
        }
    }
}

/// Adapter-owned, read-only validation plan.
///
/// `structured_step` is the canonical execution representation. The command
/// string is only the compatibility projection required by the existing
/// synchronous capture path; it is derived from the same typed arguments and
/// must never be parsed back into execution authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadOnlyValidationPlan {
    pub compatibility_command: String,
    pub structured_step: ShellJobValidationStep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ValidationPlanArg {
    Literal(&'static str),
    Value(String),
}

impl ValidationPlanArg {
    fn raw(&self) -> String {
        match self {
            Self::Literal(value) => (*value).to_string(),
            Self::Value(value) => value.clone(),
        }
    }

    fn rendered(&self) -> String {
        match self {
            Self::Literal(value) => (*value).to_string(),
            Self::Value(value) => shell_escape_simple(value),
        }
    }
}

pub(super) fn read_only_validation_plan(
    name: &'static str,
    program: &'static str,
    args: Vec<ValidationPlanArg>,
) -> Result<ReadOnlyValidationPlan, String> {
    let structured_step = ShellJobValidationStep {
        name: name.to_string(),
        program: program.to_string(),
        args: args.iter().map(ValidationPlanArg::raw).collect(),
        env: Vec::new(),
    };
    if !structured_step.is_canonical() {
        return Err("structured validation step is not canonical".to_string());
    }
    let compatibility_command = std::iter::once(program.to_string())
        .chain(args.iter().map(ValidationPlanArg::rendered))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(ReadOnlyValidationPlan {
        compatibility_command,
        structured_step,
    })
}

pub struct ValidationFailureEvidence<'a> {
    pub success: bool,
    pub reported_failure_kind: Option<&'a str>,
    pub exit_code: Option<i64>,
    pub diagnostics: Option<&'a ValidationDiagnostics>,
    pub stdout_excerpt: &'a str,
    pub stderr_excerpt: &'a str,
}

pub trait ValidationAdapter: Sync {
    fn validation_kind(&self) -> &'static str;

    fn tool_identity(&self) -> &'static str;

    fn build_readonly_plan(
        &self,
        options: ValidationCommandOptions,
    ) -> Result<ReadOnlyValidationPlan, String>;

    fn build_command(&self, options: ValidationCommandOptions) -> Result<String, String> {
        self.build_readonly_plan(options)
            .map(|plan| plan.compatibility_command)
    }

    fn parse(
        &self,
        stdout_excerpt: &str,
        stderr_excerpt: &str,
        truncated: bool,
    ) -> ValidationDiagnostics;

    fn map_failure_kind(&self, evidence: ValidationFailureEvidence<'_>) -> &'static str;

    fn reports_test_run_metadata(&self) -> bool {
        false
    }
}

/// Canonical mapping from structured validation classification to execution
/// evidence intent. Tool selection determines this purpose; callers do not.
pub fn execution_purpose_for_validation_kind(validation_kind: &str) -> ExecutionPurpose {
    match validation_kind {
        "test" => ExecutionPurpose::Test,
        "format" => ExecutionPurpose::Format,
        _ => ExecutionPurpose::Validation,
    }
}

pub fn validation_adapter_for_tool(tool_identity: &str) -> Option<&'static dyn ValidationAdapter> {
    rust::validation_adapters()
        .iter()
        .copied()
        .find(|adapter| adapter.tool_identity() == tool_identity)
        .or_else(|| go::validation_adapter(tool_identity))
}

//! Structured Cargo and Go validation adapters.

mod go;
mod rust;

use webcodex_core::project_validation::{ProjectDependencyMode, ProjectDependencyPolicy};
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
    pub dependency_mode: Option<ProjectDependencyMode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CargoCheckOptions {
    pub all_targets: Option<bool>,
    pub all_features: Option<bool>,
    pub no_default_features: Option<bool>,
    pub features: Option<String>,
    pub package: Option<String>,
    pub packages: Option<Vec<String>>,
    pub dependency_mode: Option<ProjectDependencyMode>,
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
    pub packages: Option<Vec<String>>,
    pub no_run: Option<bool>,
    pub dependency_mode: Option<ProjectDependencyMode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoCheckOptions {
    pub packages: Option<Vec<String>>,
    pub dependency_mode: Option<ProjectDependencyMode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoTestOptions {
    pub filter: Option<String>,
    pub packages: Option<Vec<String>>,
    pub dependency_mode: Option<ProjectDependencyMode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoReadOnlyValidationOperation {
    FormatCheck,
    Check(CargoCheckOptions),
    Test(CargoTestOptions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoReadOnlyValidationOperation {
    Check(GoCheckOptions),
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
    /// Apply a native test selector through the same operation that owns argv
    /// and target identity. Non-test operations must never silently ignore it.
    pub fn with_test_filter(mut self, filter: Option<&str>) -> Result<Self, &'static str> {
        let Some(filter) = filter else {
            return Ok(self);
        };
        match &mut self {
            Self::Cargo(CargoReadOnlyValidationOperation::Test(options)) => {
                options.filter = webcodex_core::runner_protocol::normalize_rust_test_filter(filter)
                    .map_err(|_| "test_filter_unsupported")?;
            }
            Self::Go(GoReadOnlyValidationOperation::Test(options)) => {
                options.filter = webcodex_core::runner_protocol::normalize_go_test_filter(filter)
                    .map_err(|_| "test_filter_unsupported")?;
            }
            _ => return Err("test_filter_unsupported"),
        }
        Ok(self)
    }

    pub fn with_dependency_policy(
        mut self,
        policy: Option<ProjectDependencyPolicy>,
    ) -> Result<Self, &'static str> {
        let Some(policy) = policy else {
            return Ok(self);
        };
        match &mut self {
            Self::Cargo(CargoReadOnlyValidationOperation::FormatCheck) => {
                return Err("dependency_policy_unsupported")
            }
            Self::Cargo(CargoReadOnlyValidationOperation::Check(options)) => {
                options.dependency_mode = Some(policy.mode);
            }
            Self::Cargo(CargoReadOnlyValidationOperation::Test(options)) => {
                options.dependency_mode = Some(policy.mode);
            }
            Self::Go(GoReadOnlyValidationOperation::Check(options)) => {
                options.dependency_mode = Some(policy.mode);
            }
            Self::Go(GoReadOnlyValidationOperation::Test(options)) => {
                options.dependency_mode = Some(policy.mode);
            }
        }
        Ok(self)
    }

    fn dependency_mode(&self) -> Option<ProjectDependencyMode> {
        match self {
            Self::Cargo(CargoReadOnlyValidationOperation::FormatCheck) => None,
            Self::Cargo(CargoReadOnlyValidationOperation::Check(options)) => {
                options.dependency_mode
            }
            Self::Cargo(CargoReadOnlyValidationOperation::Test(options)) => options.dependency_mode,
            Self::Go(GoReadOnlyValidationOperation::Check(options)) => options.dependency_mode,
            Self::Go(GoReadOnlyValidationOperation::Test(options)) => options.dependency_mode,
        }
    }

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
            Self::Go(GoReadOnlyValidationOperation::Check(_)) => ValidationCompatibilityProfile {
                tool_identity: "go_vet",
                validation_identity: ToolValidationIdentityKind::GoVet,
            },
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
            Self::Go(GoReadOnlyValidationOperation::Check(_)) => go::check_adapter(),
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
            Self::Go(GoReadOnlyValidationOperation::Check(options)) => self
                .adapter()
                .build_readonly_plan(ValidationCommandOptions::from(options.clone())),
            Self::Go(GoReadOnlyValidationOperation::Test(options)) => self
                .adapter()
                .build_readonly_plan(ValidationCommandOptions::from(options.clone())),
        }
    }

    /// Stable execution target identity for the semantic validation operation.
    /// Source state is deliberately excluded and fenced independently.
    pub fn validation_target_id(&self, cwd: Option<&str>) -> Option<String> {
        let profile = self.compatibility_profile();
        let arguments = match self {
            Self::Cargo(CargoReadOnlyValidationOperation::FormatCheck) => serde_json::json!({
                "cwd": cwd,
                "check": true,
            }),
            Self::Cargo(CargoReadOnlyValidationOperation::Check(options)) => serde_json::json!({
                "cwd": cwd,
                "all_targets": options.all_targets,
                "all_features": options.all_features,
                "no_default_features": options.no_default_features,
                "features": options.features.as_deref(),
                "package": options.package.as_deref(),
                "packages": options.packages.as_ref(),
            }),
            Self::Cargo(CargoReadOnlyValidationOperation::Test(options)) => serde_json::json!({
                "cwd": cwd,
                "filter": options.filter.as_deref(),
                "lib": options.lib,
                "all_targets": options.all_targets,
                "all_features": options.all_features,
                "no_default_features": options.no_default_features,
                "features": options.features.as_deref(),
                "package": options.package.as_deref(),
                "packages": options.packages.as_ref(),
                "no_run": options.no_run,
            }),
            Self::Go(GoReadOnlyValidationOperation::Check(options)) => serde_json::json!({
                "cwd": cwd,
                "packages": options.packages.as_ref(),
            }),
            Self::Go(GoReadOnlyValidationOperation::Test(options)) => serde_json::json!({
                "cwd": cwd,
                "packages": options.packages.as_ref(),
                "filter": options.filter.as_deref(),
            }),
        };
        let identity = webcodex_core::validation_identity::structured_validation_target_identity(
            profile.validation_identity,
            &arguments,
        )?;
        if self.dependency_mode() == Some(ProjectDependencyMode::Locked) {
            webcodex_core::validation_identity::contextualize_structured_validation_target_identity(
                &identity,
                webcodex_core::validation_identity::StructuredValidationExecutionContext::ProjectDependencyLockedV1,
            )
        } else {
            Some(identity)
        }
    }
}

/// Translate the portable project validation intent into the canonical
/// ecosystem-specific semantic operation. Project/root discovery remains
/// Runner-owned and outside this adapter boundary.
pub fn project_validation_operation(
    backend: &str,
    action: crate::SemanticCheck,
    packages: Option<Vec<String>>,
) -> Result<ReadOnlyValidationOperation, &'static str> {
    use crate::SemanticCheck::*;
    match (backend, action) {
        ("rust", Format) if packages.is_none() => Ok(ReadOnlyValidationOperation::Cargo(
            CargoReadOnlyValidationOperation::FormatCheck,
        )),
        ("rust", Format) => Err("validation_scope_unsupported"),
        ("rust", Check) => Ok(ReadOnlyValidationOperation::Cargo(
            CargoReadOnlyValidationOperation::Check(CargoCheckOptions {
                packages,
                ..Default::default()
            }),
        )),
        ("rust", Test) => Ok(ReadOnlyValidationOperation::Cargo(
            CargoReadOnlyValidationOperation::Test(CargoTestOptions {
                packages,
                ..Default::default()
            }),
        )),
        ("go", Format) => Err("validation_action_unsupported"),
        ("go", Check) => Ok(ReadOnlyValidationOperation::Go(
            GoReadOnlyValidationOperation::Check(GoCheckOptions {
                packages,
                ..Default::default()
            }),
        )),
        ("go", Test) => Ok(ReadOnlyValidationOperation::Go(
            GoReadOnlyValidationOperation::Test(GoTestOptions {
                packages,
                ..Default::default()
            }),
        )),
        _ => Err("validation_adapter_unavailable"),
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
            dependency_mode: options.dependency_mode,
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
            cargo_packages: options.packages,
            no_run: options.no_run,
            dependency_mode: options.dependency_mode,
            ..Self::default()
        }
    }
}

impl From<GoCheckOptions> for ValidationCommandOptions {
    fn from(options: GoCheckOptions) -> Self {
        Self {
            go_packages: options.packages,
            dependency_mode: options.dependency_mode,
            ..Self::default()
        }
    }
}

impl From<GoTestOptions> for ValidationCommandOptions {
    fn from(options: GoTestOptions) -> Self {
        Self {
            filter: options.filter,
            go_packages: options.packages,
            dependency_mode: options.dependency_mode,
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

/// Canonical project selection; direct tools remain compatibility entry points.
pub fn validation_adapter_for_recipe(
    backend: &str,
    action: crate::SemanticCheck,
) -> Option<&'static dyn ValidationAdapter> {
    use crate::SemanticCheck::*;
    validation_adapter_for_tool(match (backend, action) {
        ("rust", Format) => "cargo_fmt",
        ("rust", Check) => "cargo_check",
        ("rust", Test) => "cargo_test",
        ("go", Check) => "go_vet",
        ("go", Test) => "go_test",
        _ => return None,
    })
}

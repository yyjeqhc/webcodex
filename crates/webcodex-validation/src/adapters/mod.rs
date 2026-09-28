//! Structured Cargo and Go validation adapters.

mod go;
mod rust;

use webcodex_core::runner_protocol::ShellJobValidationStep;
use webcodex_core::shell_quote::shell_escape_simple;
use webcodex_core::validation_evidence::ValidationDiagnostics;
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

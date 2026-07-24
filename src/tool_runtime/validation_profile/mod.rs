//! Internal project recipes plus the existing Cargo evidence adapters.
//! This module never adds model-visible runtime tools.

mod recipe;
#[cfg(test)]
mod recipe_tests;
mod rust;

use super::validation_parser::ValidationDiagnostics;

pub(crate) use recipe::{resolve_validation_recipe, RecipeError, RecipeId, SemanticCheck};

#[derive(Debug, Default)]
pub(crate) struct ValidationCommandOptions {
    pub(crate) check: bool,
    pub(crate) filter: Option<String>,
    pub(crate) all_targets: Option<bool>,
    pub(crate) all_features: Option<bool>,
    pub(crate) no_default_features: Option<bool>,
    pub(crate) features: Option<String>,
    pub(crate) package: Option<String>,
    pub(crate) no_run: Option<bool>,
}

pub(crate) struct ValidationFailureEvidence<'a> {
    pub(crate) success: bool,
    pub(crate) reported_failure_kind: Option<&'a str>,
    pub(crate) exit_code: Option<i64>,
    pub(crate) diagnostics: Option<&'a ValidationDiagnostics>,
    pub(crate) stdout_excerpt: &'a str,
    pub(crate) stderr_excerpt: &'a str,
}

pub(crate) trait ValidationAdapter: Sync {
    fn validation_kind(&self) -> &'static str;

    fn tool_identity(&self) -> &'static str;

    fn build_command(&self, options: ValidationCommandOptions) -> Result<String, String>;

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

pub(crate) fn validation_adapter_for_tool(
    tool_identity: &str,
) -> Option<&'static dyn ValidationAdapter> {
    rust::validation_adapters()
        .iter()
        .copied()
        .find(|adapter| adapter.tool_identity() == tool_identity)
}

//! Structured Cargo and Go validation adapters.

mod go;
mod rust;

use webcodex_core::validation_evidence::ValidationDiagnostics;

#[derive(Debug, Clone, Default)]
pub struct ValidationCommandOptions {
    pub check: bool,
    pub filter: Option<String>,
    pub all_targets: Option<bool>,
    pub all_features: Option<bool>,
    pub no_default_features: Option<bool>,
    pub features: Option<String>,
    pub package: Option<String>,
    pub no_run: Option<bool>,
    /// First-class `go_test` package scope. Other validation adapters must
    /// reject this Go-specific option rather than silently ignoring it.
    pub go_packages: Option<Vec<String>>,
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

pub fn validation_adapter_for_tool(tool_identity: &str) -> Option<&'static dyn ValidationAdapter> {
    rust::validation_adapters()
        .iter()
        .copied()
        .find(|adapter| adapter.tool_identity() == tool_identity)
        .or_else(|| go::validation_adapter(tool_identity))
}

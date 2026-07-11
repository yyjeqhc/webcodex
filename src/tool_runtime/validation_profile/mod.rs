//! Internal validation profile and adapter registry.
//!
//! Profiles describe how a language is detected and which validation adapters
//! it supports. Adapters own the language/tool-specific command, parser, and
//! failure classification decisions. This registry is deliberately internal:
//! it does not add runtime tools or alter their schemas.

mod rust;

use std::path::Path;

use super::validation_parser::ValidationDiagnostics;

pub(crate) use rust::RustValidationProfile;

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

pub(crate) trait ValidationProfile: Sync {
    fn language(&self) -> &'static str;

    fn project_markers(&self) -> &'static [&'static str];

    fn supported_validation_kinds(&self) -> &'static [&'static str];

    fn adapters(&self) -> &'static [&'static dyn ValidationAdapter];

    #[allow(dead_code)]
    fn detects_project(&self, project_root: &Path) -> bool {
        self.project_markers()
            .iter()
            .any(|marker| project_root.join(marker).is_file())
    }
}

static RUST_PROFILE: RustValidationProfile = RustValidationProfile;
static REGISTERED_VALIDATION_PROFILES: [&dyn ValidationProfile; 1] = [&RUST_PROFILE];

pub(crate) fn registered_validation_profiles() -> &'static [&'static dyn ValidationProfile] {
    &REGISTERED_VALIDATION_PROFILES
}

pub(crate) fn validation_adapter_for_tool(
    tool_identity: &str,
) -> Option<&'static dyn ValidationAdapter> {
    for profile in registered_validation_profiles() {
        debug_assert!(!profile.language().is_empty());
        debug_assert!(!profile.project_markers().is_empty());
        debug_assert!(!profile.supported_validation_kinds().is_empty());
        for adapter in profile.adapters() {
            debug_assert!(
                profile
                    .supported_validation_kinds()
                    .contains(&adapter.validation_kind()),
                "validation profile adapter kind must be declared by its profile"
            );
            if adapter.tool_identity() == tool_identity {
                return Some(*adapter);
            }
        }
    }
    None
}

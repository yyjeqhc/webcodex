//! Shared declarative scope for portable project lifecycle operations.

use serde::{Deserialize, Serialize};

pub(crate) const PROJECT_OPERATION_SCOPE_MAX_PACKAGES: usize = 8;
pub(crate) const PROJECT_OPERATION_SCOPE_MAX_PACKAGE_BYTES: usize = 256;

/// Portable dependency-resolution policy for bounded project operations.
///
/// `Locked` means the adapter must not repair dependency selection by updating
/// the ecosystem's project-level resolution state. Network access is a separate
/// policy dimension and is intentionally not controlled here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectDependencyMode {
    Locked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectDependencyPolicy {
    pub mode: ProjectDependencyMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectOperationScope {
    /// Portable bounded package selectors. Adapters translate these semantic
    /// selectors into ecosystem-specific package mechanics.
    #[schemars(length(min = 1, max = 8))]
    #[schemars(inner(length(min = 1, max = 256)))]
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectOperationScopeError {
    PackageCount,
    InvalidPackage,
}

impl ProjectOperationScope {
    pub(crate) fn validate(&self) -> Result<(), ProjectOperationScopeError> {
        if self.packages.is_empty() || self.packages.len() > PROJECT_OPERATION_SCOPE_MAX_PACKAGES {
            return Err(ProjectOperationScopeError::PackageCount);
        }
        if self.packages.iter().any(|package| {
            package.is_empty()
                || package.len() > PROJECT_OPERATION_SCOPE_MAX_PACKAGE_BYTES
                || package.chars().any(char::is_control)
        }) {
            return Err(ProjectOperationScopeError::InvalidPackage);
        }
        Ok(())
    }
}

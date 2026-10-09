//! Declarative project validation planning and admission fence. No model argv.
use crate::project_operation::ProjectOperationScopeError;
use serde::{Deserialize, Serialize};

/// Portable project-operation scope retained under the project_validate API name.
pub use crate::project_operation::{
    ProjectDependencyMode, ProjectDependencyPolicy, ProjectOperationScope as ProjectValidationScope,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectValidationAction {
    FormatCheck,
    Check,
    Test,
}
impl ProjectValidationAction {
    pub fn kind(self) -> &'static str {
        match self {
            Self::FormatCheck => "format",
            Self::Check => "check",
            Self::Test => "test",
        }
    }
}
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProjectValidationAdapter {
    #[default]
    Auto,
    Rust,
    Go,
    Python,
    Node,
}

/// Test-only selection and evidence policy. Filtering changes the execution
/// target; count requirements are postconditions, not executable arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectValidationTestOptions {
    /// Rust: one libtest substring (not flags). Go: native -run regexp, including
    /// slash-separated subtest expressions; whitespace is significant for Go.
    /// Python: one native pytest -k expression, never command-line flags.
    /// Omission/empty selects the unfiltered default. At most 200 UTF-8 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 200))]
    pub filter: Option<String>,
    /// Defaults to true. false explicitly accepts proven zero tests only when
    /// no min_tests is requested. Missing/truncated count evidence is not zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_tests: Option<bool>,
    /// Proven executed-test minimum, independent from the process exit code.
    /// A supplied minimum still applies when require_tests=false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 1_000_000))]
    pub min_tests: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectValidationRequest {
    pub project_id: String,
    pub cwd: Option<String>,
    pub action: ProjectValidationAction,
    #[serde(default)]
    pub adapter: ProjectValidationAdapter,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ProjectValidationScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency_policy: Option<ProjectDependencyPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<ProjectValidationTestOptions>,
}
impl ProjectValidationRequest {
    /// Effective (require_tests, minimum_tests) retained with the same Job.
    /// Call validate before using this policy; non-test actions have neither.
    pub fn test_requirements(&self) -> (Option<bool>, Option<u64>) {
        if self.action != ProjectValidationAction::Test {
            return (None, None);
        }
        let required = self
            .test
            .as_ref()
            .and_then(|test| test.require_tests)
            .unwrap_or(true);
        let minimum = self
            .test
            .as_ref()
            .and_then(|test| test.min_tests)
            .or_else(|| required.then_some(1));
        (Some(required), minimum)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.project_id.is_empty()
            || self.project_id.len() > 200
            || self.project_id.chars().any(char::is_control)
        {
            return Err("invalid project validation project id".into());
        }
        validate_relative(self.cwd.as_deref().unwrap_or("."))?;
        if let Some(scope) = &self.scope {
            scope.validate().map_err(|error| match error {
                ProjectOperationScopeError::Selection => {
                    "project validation scope must select packages or all_packages=true".to_string()
                }
                ProjectOperationScopeError::PackageCount => {
                    "project validation packages must contain between 1 and 8 items".to_string()
                }
                ProjectOperationScopeError::InvalidPackage => {
                    "invalid project validation package scope".to_string()
                }
            })?;
        }
        if self.dependency_policy.is_some() && self.action == ProjectValidationAction::FormatCheck {
            return Err("dependency_policy requires project_validate action=check or test".into());
        }
        if let Some(test) = &self.test {
            if self.action != ProjectValidationAction::Test {
                return Err("test options require project_validate action=test".into());
            }
            if test.filter.as_ref().is_some_and(|filter| {
                filter.len() > crate::runner_protocol::RUST_TEST_FILTER_MAX_BYTES
                    || filter.chars().any(char::is_control)
            }) {
                return Err(
                    "test filter must be at most 200 UTF-8 bytes without control characters".into(),
                );
            }
            if test.min_tests.is_some_and(|minimum| {
                !(1..=crate::runner_protocol::CARGO_TEST_MIN_TESTS_MAX).contains(&minimum)
            }) {
                return Err("min_tests must be between 1 and 1000000".into());
            }
        }
        Ok(())
    }
}
pub fn validate_relative(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 1024
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.chars().any(char::is_control)
        || path.split('/').any(|p| p == "..")
    {
        return Err("validation cwd must be project-relative".into());
    }
    Ok(())
}
/// Retained internally with the exact durable Job plan. Digests fence planning
/// against admission; they are not workspace freshness evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectValidationProvenance {
    pub request: ProjectValidationRequest,
    pub backend: String,
    pub recipe_root: String,
    pub root_digest: String,
    pub manifest_digest: String,
    pub invocation_digest: String,
}
impl ProjectValidationProvenance {
    pub fn is_valid(&self) -> bool {
        self.request.validate().is_ok()
            && validate_relative(&self.recipe_root).is_ok()
            && matches!(self.backend.as_str(), "rust" | "go" | "python" | "node")
            && [
                &self.root_digest,
                &self.manifest_digest,
                &self.invocation_digest,
            ]
            .iter()
            .all(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectValidationPlan {
    pub provenance: ProjectValidationProvenance,
    pub adapter: String,
    pub step: crate::runner_protocol::ShellJobValidationStep,
    pub validation_target_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectValidationPlanningResult {
    Ready {
        plan: ProjectValidationPlan,
    },
    Unavailable {
        code: String,
        detected_backend: Option<String>,
    },
}

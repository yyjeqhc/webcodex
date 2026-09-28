//! Declarative project validation planning and admission fence. No model argv.
use serde::{Deserialize, Serialize};

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
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectValidationRequest {
    pub project_id: String,
    pub cwd: Option<String>,
    pub action: ProjectValidationAction,
    #[serde(default)]
    pub adapter: ProjectValidationAdapter,
}
impl ProjectValidationRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.project_id.is_empty()
            || self.project_id.len() > 200
            || self.project_id.chars().any(char::is_control)
        {
            return Err("invalid project validation project id".into());
        }
        validate_relative(self.cwd.as_deref().unwrap_or("."))
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
            && matches!(self.backend.as_str(), "rust" | "go")
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

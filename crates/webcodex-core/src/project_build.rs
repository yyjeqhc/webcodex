//! Declarative project build planning and admission provenance. No model argv.

use crate::runner_protocol::{normalize_cargo_packages, normalize_go_packages, ShellProcessArgv};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROJECT_BUILD_PROVENANCE_MAX_BYTES: usize = 8 * 1024;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProjectBuildAdapter {
    #[default]
    Auto,
    Rust,
    Go,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectBuildScope {
    /// Portable bounded package scope. Rust maps values to repeated Cargo -p
    /// selectors; Go maps values to project-relative package patterns.
    #[schemars(length(min = 1, max = 8))]
    #[schemars(inner(length(min = 1, max = 256)))]
    pub packages: Vec<String>,
}

impl ProjectBuildScope {
    fn validate(&self) -> Result<(), String> {
        if self.packages.is_empty() || self.packages.len() > 8 {
            return Err("project build packages must contain between 1 and 8 items".into());
        }
        if self.packages.iter().any(|package| {
            package.is_empty() || package.len() > 256 || package.chars().any(char::is_control)
        }) {
            return Err("invalid project build package scope".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBuildRequest {
    pub project_id: String,
    pub cwd: Option<String>,
    #[serde(default)]
    pub adapter: ProjectBuildAdapter,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ProjectBuildScope>,
}

impl ProjectBuildRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.project_id.is_empty()
            || self.project_id.len() > 200
            || self.project_id.chars().any(char::is_control)
        {
            return Err("invalid project build project id".into());
        }
        validate_relative(self.cwd.as_deref().unwrap_or("."))?;
        if let Some(scope) = &self.scope {
            scope.validate()?;
        }
        Ok(())
    }
}

fn validate_relative(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 1024
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| part == "..")
    {
        return Err("build cwd must be project-relative".into());
    }
    Ok(())
}

/// Retained internally with the exact durable Job plan. Digests fence planning
/// against admission; they are not a source-tree freshness proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBuildProvenance {
    pub request: ProjectBuildRequest,
    pub backend: String,
    pub recipe_root: String,
    pub root_digest: String,
    pub manifest_digest: String,
    pub invocation_digest: String,
}

impl ProjectBuildProvenance {
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
            .all(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBuildPlan {
    pub provenance: ProjectBuildProvenance,
    pub process: ShellProcessArgv,
}

impl ProjectBuildPlan {
    pub fn is_valid(&self) -> bool {
        self.provenance.is_valid()
            && canonical_project_build_process(&self.provenance.backend, &self.provenance.request)
                .is_ok_and(|process| process == self.process)
            && project_build_invocation_digest(&self.process) == self.provenance.invocation_digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectBuildPlanningResult {
    Ready {
        plan: ProjectBuildPlan,
    },
    Unavailable {
        code: String,
        detected_backend: Option<String>,
    },
}

pub fn canonical_project_build_process(
    backend: &str,
    request: &ProjectBuildRequest,
) -> Result<ShellProcessArgv, &'static str> {
    let packages = request
        .scope
        .as_ref()
        .map(|scope| scope.packages.as_slice());
    match backend {
        "rust" => {
            let packages = normalize_cargo_packages(None, packages)?;
            let mut args = vec!["build".to_string()];
            if let Some(packages) = packages {
                for package in packages {
                    args.push("-p".to_string());
                    args.push(package);
                }
            }
            Ok(ShellProcessArgv {
                executable: "cargo".to_string(),
                args,
            })
        }
        "go" => {
            let packages = normalize_go_packages(packages)?;
            let mut args = vec!["build".to_string()];
            args.extend(packages);
            Ok(ShellProcessArgv {
                executable: "go".to_string(),
                args,
            })
        }
        _ => Err("unsupported project build backend"),
    }
}

pub fn project_build_invocation_digest(process: &ShellProcessArgv) -> String {
    let bytes = serde_json::to_vec(process).expect("ShellProcessArgv serialization cannot fail");
    format!("{:x}", Sha256::digest(bytes))
}

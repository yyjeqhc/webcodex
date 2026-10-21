//! Declarative project build planning and admission provenance. No model argv.

use crate::project_operation::ProjectOperationScopeError;
use crate::runner_protocol::{normalize_cargo_packages, normalize_go_packages, ShellProcessArgv};
use serde::{Deserialize, Serialize};

pub use crate::project_validation::ProjectDependencyResolutionPolicy;

/// Portable project-operation scope retained under the project_build API name.
pub use crate::project_operation::{
    ProjectDependencyMode, ProjectDependencyPolicy, ProjectOperationScope as ProjectBuildScope,
};
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
    Node,
    Python,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency_policy: Option<ProjectDependencyPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency_resolution_policy: Option<ProjectDependencyResolutionPolicy>,
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
            scope.validate().map_err(|error| match error {
                ProjectOperationScopeError::Selection => {
                    "project build scope must select packages or all_packages=true".to_string()
                }
                ProjectOperationScopeError::PackageCount => {
                    "project build packages must contain between 1 and 8 items".to_string()
                }
                ProjectOperationScopeError::InvalidPackage => {
                    "invalid project build package scope".to_string()
                }
            })?;
        }
        if self.dependency_resolution_policy.is_some()
            && !matches!(
                self.adapter,
                ProjectBuildAdapter::Auto | ProjectBuildAdapter::Rust
            )
        {
            return Err("dependency_resolution_policy requires Rust project_build".into());
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
            && matches!(
                (self.request.adapter, self.backend.as_str()),
                (ProjectBuildAdapter::Auto, "rust" | "go" | "python" | "node")
                    | (ProjectBuildAdapter::Rust, "rust")
                    | (ProjectBuildAdapter::Go, "go")
                    | (ProjectBuildAdapter::Python, "python")
                    | (ProjectBuildAdapter::Node, "node")
            )
            && (self.request.dependency_resolution_policy.is_none() || self.backend == "rust")
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
            && canonical_project_build_process_with_manager(
                &self.provenance.backend,
                &self.provenance.request,
                &self.process.executable,
            )
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
    canonical_project_build_process_with_manager(backend, request, "npm")
}

/// Node manager is resolved from the manifest and bound by invocation digest and
/// Runner re-planning; this validator accepts only the closed script vocabulary.
pub fn canonical_project_build_process_with_manager(
    backend: &str,
    request: &ProjectBuildRequest,
    manager: &str,
) -> Result<ShellProcessArgv, &'static str> {
    if request.dependency_resolution_policy.is_some() && backend != "rust" {
        return Err("dependency_resolution_policy_unsupported");
    }
    let packages = request
        .scope
        .as_ref()
        .and_then(ProjectBuildScope::explicit_packages);
    let all_packages = request
        .scope
        .as_ref()
        .is_some_and(ProjectBuildScope::selects_all_packages);
    match backend {
        "rust" => {
            let packages = normalize_cargo_packages(None, packages)?;
            let mut args = vec!["build".to_string()];
            if request
                .dependency_policy
                .is_some_and(|policy| policy.mode == ProjectDependencyMode::Locked)
            {
                args.push("--locked".to_string());
            }
            if request.dependency_resolution_policy
                == Some(ProjectDependencyResolutionPolicy::Offline)
            {
                args.push("--offline".to_string());
            }
            if all_packages {
                args.push("--workspace".to_string());
            } else if let Some(packages) = packages {
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
        "python" => {
            if request.scope.is_some() {
                return Err("Python packaging does not support package scope");
            }
            Ok(ShellProcessArgv {
                executable: "python".into(),
                args: vec!["-m".into(), "build".into(), "--no-isolation".into()],
            })
        }
        "go" => {
            let packages = normalize_go_packages(if all_packages { None } else { packages })?;
            let mut args = vec!["build".to_string()];
            if request
                .dependency_policy
                .is_some_and(|policy| policy.mode == ProjectDependencyMode::Locked)
            {
                args.push("-mod=readonly".to_string());
            }
            args.extend(packages);
            Ok(ShellProcessArgv {
                executable: "go".to_string(),
                args,
            })
        }
        "node" if matches!(manager, "npm" | "pnpm" | "yarn" | "bun") => {
            if request.scope.is_some() || request.dependency_policy.is_some() {
                return Err("Node build scope/dependency policy unavailable");
            }
            Ok(ShellProcessArgv {
                executable: manager.into(),
                args: if manager == "yarn" {
                    vec!["run".into(), "build".into()]
                } else {
                    vec!["run".into(), "--silent".into(), "build".into()]
                },
            })
        }
        _ => Err("unsupported project build backend"),
    }
}

pub fn project_build_invocation_digest(process: &ShellProcessArgv) -> String {
    let bytes = serde_json::to_vec(process).expect("ShellProcessArgv serialization cannot fail");
    format!("{:x}", Sha256::digest(bytes))
}

/// A bounded observation of an existing artifact, never a correctness receipt.
/// These use the same path, byte count, MIME and digest facts as artifact reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBuildArtifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub mime_type: Option<String>,
}

pub const PROJECT_BUILD_ARTIFACTS_MAX: usize = 16;
pub fn validate_build_artifacts(artifacts: &[ProjectBuildArtifact]) -> bool {
    artifacts.len() <= PROJECT_BUILD_ARTIFACTS_MAX
        && artifacts
            .iter()
            .try_fold(0u64, |sum, artifact| sum.checked_add(artifact.bytes))
            .is_some_and(|total| total <= 256 * 1024 * 1024)
        && artifacts.iter().enumerate().all(|(index, artifact)| {
            validate_relative(&artifact.path).is_ok()
                && !crate::sensitive_paths::is_bulk_skipped_path(&artifact.path)
                && artifact.bytes <= 256 * 1024 * 1024
                && artifact.sha256.len() == 64
                && artifact
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                && artifact.mime_type.as_ref().is_none_or(|mime| {
                    !mime.is_empty() && mime.len() <= 128 && !mime.chars().any(char::is_control)
                })
                && !artifacts[..index]
                    .iter()
                    .any(|earlier| earlier.path == artifact.path)
        })
}

/// Artifact metadata is terminal historical observation, never active execution
/// authority. Reject injected metadata on unrelated or incomplete Jobs.
pub fn validate_build_artifact_report(
    source: &str,
    status: &str,
    exit_code: Option<i32>,
    state: Option<crate::runner_protocol::ShellCommandExecutionState>,
    artifacts: Option<&[ProjectBuildArtifact]>,
) -> Result<(), &'static str> {
    if let Some(artifacts) = artifacts {
        if source != "project_build"
            || status != "completed"
            || exit_code != Some(0)
            || state != Some(crate::runner_protocol::ShellCommandExecutionState::Completed)
            || !validate_build_artifacts(artifacts)
        {
            return Err("build_artifacts_invalid");
        }
    }
    Ok(())
}

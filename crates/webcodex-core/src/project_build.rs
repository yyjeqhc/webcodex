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

/// Closed target intent, translated by the Rust adapter rather than caller argv.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBuildTargets {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub library: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binaries: Vec<String>,
}

impl schemars::JsonSchema for ProjectBuildTargets {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ProjectBuildTargets".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type":"object", "additionalProperties":false,
            "properties": {
                "library":{"type":"boolean"},
                "binaries":{"type":"array","maxItems":8,"items":{"type":"string","minLength":1,"maxLength":256,"pattern":"^[A-Za-z0-9_][A-Za-z0-9_-]*$"}}
            },
            "anyOf":[
                {"required":["library"],"properties":{"library":{"const":true}}},
                {"required":["binaries"],"properties":{"binaries":{"minItems":1}}}
            ]
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectBuildArtifactPolicy {
    /// Observe selected outputs; this is not proof of correctness or creation.
    Observe,
}

fn build_name_valid(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        && !value.starts_with('-')
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<ProjectBuildTargets>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_policy: Option<ProjectBuildArtifactPolicy>,
}

impl ProjectBuildRequest {
    pub fn has_rust_options(&self) -> bool {
        self.profile.is_some() || self.targets.is_some() || self.artifact_policy.is_some()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.project_id.is_empty()
            || self.project_id.len() > 200
            || self.project_id.chars().any(char::is_control)
        {
            return Err("invalid project build project id".into());
        }
        validate_relative(self.cwd.as_deref().unwrap_or("."))?;
        if self.has_rust_options()
            && !matches!(
                self.adapter,
                ProjectBuildAdapter::Auto | ProjectBuildAdapter::Rust
            )
        {
            return Err(
                "build_options_unsupported: profile, targets and artifact_policy require Rust"
                    .into(),
            );
        }
        if self
            .profile
            .as_ref()
            .is_some_and(|name| !build_name_valid(name, 64))
        {
            return Err("invalid build profile".into());
        }
        if self.targets.as_ref().is_some_and(|targets| {
            (!targets.library && targets.binaries.is_empty())
                || targets.binaries.len() > 8
                || targets
                    .binaries
                    .iter()
                    .any(|name| !build_name_valid(name, 256))
        }) {
            return Err("build targets require a library or at most eight named binaries".into());
        }
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
                ProjectOperationScopeError::ExclusionRequiresAllPackages => {
                    "exclude_packages requires all_packages=true".to_string()
                }
                ProjectOperationScopeError::ExclusionCount => {
                    "exclude_packages must contain at most 8 items".to_string()
                }
                ProjectOperationScopeError::InvalidExclusion => {
                    "invalid excluded package name".to_string()
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
            && (!self.request.has_rust_options() || self.backend == "rust")
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
    if request.has_rust_options() && backend != "rust" {
        return Err("build_options_unsupported");
    }
    canonical_project_build_process_with_manager(backend, request, "npm")
}

/// Node manager is resolved from the manifest and bound by invocation digest and
/// Runner re-planning; this validator accepts only the closed script vocabulary.
pub fn canonical_project_build_process_with_manager(
    backend: &str,
    request: &ProjectBuildRequest,
    manager: &str,
) -> Result<ShellProcessArgv, &'static str> {
    if request.validate().is_err() {
        return Err("invalid_arguments");
    }
    if request.has_rust_options() && backend != "rust" {
        return Err("build_options_unsupported");
    }
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
    let excluded_packages = request
        .scope
        .as_ref()
        .map(ProjectBuildScope::normalized_exclude_packages)
        .transpose()?
        .unwrap_or_default();
    if !excluded_packages.is_empty() && backend != "rust" {
        return Err("workspace_exclusions_unsupported");
    }
    match backend {
        "rust" => {
            let packages = normalize_cargo_packages(None, packages)?;
            let mut args = vec!["build".to_string()];
            if let Some(profile) = &request.profile {
                args.extend(["--profile".into(), profile.clone()]);
            }
            if let Some(targets) = &request.targets {
                if targets.library {
                    args.push("--lib".into());
                }
                let mut binaries = targets.binaries.clone();
                binaries.sort_unstable();
                binaries.dedup();
                for binary in binaries {
                    args.extend(["--bin".into(), binary]);
                }
            }
            if request.artifact_policy.is_some() {
                args.push("--message-format=json-render-diagnostics".into());
            }
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
                for package in &excluded_packages {
                    args.push("--exclude".to_string());
                    args.push(package.clone());
                }
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
            if !excluded_packages.is_empty() {
                return Err("workspace_exclusions_unsupported");
            }
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

/// Finite native-build observations can name Cargo's generated `target/` tree.
/// This does not grant generic artifact read/export authority or bulk traversal.
pub fn build_artifact_path_valid(path: &str) -> bool {
    validate_relative(path).is_ok()
        && !crate::sensitive_paths::is_secret_path(path)
        && !path
            .split('/')
            .any(|component| component.eq_ignore_ascii_case("node_modules"))
}

/// Generic adapters retain the bulk-exclusion policy.
pub fn validate_build_artifacts(artifacts: &[ProjectBuildArtifact]) -> bool {
    validate_artifacts_with_path_policy(artifacts, |path| {
        validate_relative(path).is_ok() && !crate::sensitive_paths::is_bulk_skipped_path(path)
    })
}

/// Only exact Cargo output observations may include generated target/ paths.
pub fn validate_cargo_build_artifacts(artifacts: &[ProjectBuildArtifact]) -> bool {
    validate_artifacts_with_path_policy(artifacts, build_artifact_path_valid)
}

fn validate_artifacts_with_path_policy(
    artifacts: &[ProjectBuildArtifact],
    path_valid: impl Fn(&str) -> bool,
) -> bool {
    artifacts.len() <= PROJECT_BUILD_ARTIFACTS_MAX
        && artifacts
            .iter()
            .try_fold(0u64, |sum, artifact| sum.checked_add(artifact.bytes))
            .is_some_and(|total| total <= 256 * 1024 * 1024)
        && artifacts.iter().enumerate().all(|(index, artifact)| {
            path_valid(&artifact.path)
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
/// authority. Receipts retain no backend/request, so validate the union of
/// supported observations here; readers enforce their adapter-specific policy.
/// Reject injected metadata on unrelated or incomplete Jobs.
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
            || !validate_cargo_build_artifacts(artifacts)
        {
            return Err("build_artifacts_invalid");
        }
    }
    Ok(())
}

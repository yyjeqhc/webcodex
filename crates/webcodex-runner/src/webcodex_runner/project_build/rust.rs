//! Rust build output observation stays in the original process-owning Job.
use super::*;
use crate::webcodex_runner::config::ShellConfig;
use crate::webcodex_runner::output::ShellCommandResult;
use crate::webcodex_runner::shell::PreparedShellProfileCache;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use webcodex_core::project_build::{ProjectBuildArtifact, ProjectBuildScope, ProjectBuildTargets};
use webcodex_core::runner_protocol::ShellCommandExecutionState;

struct SelectedPackages {
    ids: BTreeSet<String>,
}

fn evidence_error(code: &'static str) -> String {
    code.into()
}

fn parsed_output(result: &ShellCommandResult) -> Result<&str, String> {
    if result.stdout_truncated || result.stderr_truncated {
        return Err(evidence_error("build_artifact_output_truncated"));
    }
    let text = result.result.stdout.as_deref().unwrap_or("");
    if text.contains('\u{fffd}') {
        return Err(evidence_error("build_artifact_output_invalid_utf8"));
    }
    Ok(text)
}

fn selected_packages(
    root: &Path,
    cwd: &Path,
    request: &ProjectBuildRequest,
    text: &str,
) -> Result<SelectedPackages, String> {
    let invalid = || evidence_error("build_artifact_metadata_invalid");
    let value: Value = serde_json::from_str(text).map_err(|_| invalid())?;
    let packages = value["packages"].as_array().ok_or_else(invalid)?;
    let members = value["workspace_members"].as_array().ok_or_else(invalid)?;
    if packages.len() > 256 || members.len() > 256 {
        return Err(invalid());
    }
    let member_ids = members
        .iter()
        .map(|id| id.as_str().map(str::to_owned).ok_or_else(invalid))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut names = BTreeMap::new();
    let mut local_ids = BTreeSet::new();
    let mut local_package = None;
    for package in packages {
        let id = package["id"].as_str().ok_or_else(invalid)?;
        if !member_ids.contains(id) {
            continue;
        }
        let name = package["name"].as_str().ok_or_else(invalid)?;
        let manifest = Path::new(package["manifest_path"].as_str().ok_or_else(invalid)?)
            .canonicalize()
            .map_err(|_| invalid())?;
        if !manifest.starts_with(root) || names.insert(name.to_owned(), id.to_owned()).is_some() {
            return Err(invalid());
        }
        local_ids.insert(id.to_owned());
        if let Some(version) = package["version"].as_str() {
            names.insert(format!("{name}@{version}"), id.to_owned());
        }
        names.insert(id.to_owned(), id.to_owned());
        if manifest
            == cwd
                .join("Cargo.toml")
                .canonicalize()
                .map_err(|_| invalid())?
        {
            local_package = Some(id.to_owned());
        }
    }
    if local_ids != member_ids {
        return Err(invalid());
    }
    let mut ids = if request
        .scope
        .as_ref()
        .is_some_and(ProjectBuildScope::selects_all_packages)
    {
        member_ids.clone()
    } else if let Some(packages) = request
        .scope
        .as_ref()
        .and_then(ProjectBuildScope::explicit_packages)
    {
        webcodex_core::runner_protocol::normalize_cargo_packages(None, Some(packages))
            .map_err(|_| invalid())?
            .ok_or_else(invalid)?
            .iter()
            .map(|name| names.get(name).cloned().ok_or_else(invalid))
            .collect::<Result<BTreeSet<_>, _>>()?
    } else if let Some(id) = local_package {
        BTreeSet::from([id])
    } else {
        value["workspace_default_members"]
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .map(|id| id.as_str().map(str::to_owned).ok_or_else(invalid))
            .collect::<Result<BTreeSet<_>, _>>()?
    };
    // Reuse admitted typed scope; an excluded member built as a dependency
    // does not become a selected output owner.
    if let Some(scope) = &request.scope {
        for name in scope.normalized_exclude_packages().map_err(|_| invalid())? {
            ids.remove(names.get(&name).ok_or_else(invalid)?);
        }
    }
    if ids.is_empty() || !ids.is_subset(&member_ids) {
        return Err(invalid());
    }
    Ok(SelectedPackages { ids })
}

fn target_selected(target: &Value, selected: Option<&ProjectBuildTargets>) -> bool {
    let Some(kinds) = target["kind"].as_array() else {
        return false;
    };
    let binary = kinds.iter().any(|kind| kind == "bin");
    let library = kinds.iter().any(|kind| {
        matches!(
            kind.as_str(),
            Some("lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro")
        )
    });
    match selected {
        None => binary || library,
        Some(selected) => {
            selected.library && library
                || binary
                    && target["name"]
                        .as_str()
                        .is_some_and(|name| selected.binaries.iter().any(|binary| binary == name))
        }
    }
}

fn artifact_paths(
    root: &Path,
    request: &ProjectBuildRequest,
    packages: &SelectedPackages,
    text: &str,
) -> Result<Vec<String>, String> {
    let invalid = || evidence_error("build_artifact_output_invalid");
    let mut finished = false;
    let mut paths = BTreeSet::new();
    let mut library_seen = false;
    let mut binary_names = BTreeSet::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        if finished {
            return Err(invalid());
        }
        let value: Value = serde_json::from_str(line).map_err(|_| invalid())?;
        match value["reason"].as_str().ok_or_else(invalid)? {
            "build-finished" => {
                if value["success"] != true {
                    return Err(invalid());
                }
                finished = true;
            }
            "compiler-artifact" => {
                let id = value["package_id"].as_str().ok_or_else(invalid)?;
                if !packages.ids.contains(id)
                    || !target_selected(&value["target"], request.targets.as_ref())
                {
                    continue;
                }
                // Cargo messages are observations, never permission to read a new root.
                let source = Path::new(value["target"]["src_path"].as_str().ok_or_else(invalid)?)
                    .canonicalize()
                    .map_err(|_| invalid())?;
                if !source.starts_with(root) {
                    return Err(invalid());
                }
                let kinds = value["target"]["kind"].as_array().ok_or_else(invalid)?;
                if kinds.iter().any(|kind| kind == "bin") {
                    binary_names.insert(
                        value["target"]["name"]
                            .as_str()
                            .ok_or_else(invalid)?
                            .to_owned(),
                    );
                } else {
                    library_seen = true;
                }
                let files = value["filenames"].as_array().ok_or_else(invalid)?;
                if files.is_empty()
                    || files.len() > webcodex_core::project_build::PROJECT_BUILD_ARTIFACTS_MAX
                {
                    return Err(invalid());
                }
                for file in files {
                    let file = Path::new(file.as_str().ok_or_else(invalid)?);
                    let canonical = file
                        .canonicalize()
                        .map_err(|_| evidence_error("build_artifacts_missing"))?;
                    let relative = canonical
                        .strip_prefix(root)
                        .map_err(|_| evidence_error("build_artifact_path_invalid"))?;
                    let relative = relative.to_str().ok_or_else(invalid)?;
                    #[cfg(windows)]
                    let relative = relative.replace('\\', "/");
                    #[cfg(not(windows))]
                    let relative = relative.to_owned();
                    if relative.is_empty() {
                        return Err(evidence_error("build_artifact_path_invalid"));
                    }
                    paths.insert(relative);
                    if paths.len() > webcodex_core::project_build::PROJECT_BUILD_ARTIFACTS_MAX {
                        return Err(evidence_error("build_artifacts_limit_exceeded"));
                    }
                }
            }
            "compiler-message" | "build-script-executed" => {}
            _ => return Err(invalid()),
        }
    }
    if !finished {
        return Err(invalid());
    }
    if paths.is_empty()
        || request.targets.as_ref().is_some_and(|targets| {
            targets.library && !library_seen
                || targets
                    .binaries
                    .iter()
                    .any(|name| !binary_names.contains(name))
        })
    {
        return Err(evidence_error("build_artifacts_missing"));
    }
    Ok(paths.into_iter().collect())
}

fn failure(code: String, started: Instant) -> ShellCommandResult {
    ShellCommandResult::not_started(CommandResult {
        exit_code: None,
        stdout: None,
        stderr: None,
        duration_ms: Some(started.elapsed().as_millis() as u64),
        error: Some(code),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_rust_build(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    registry: &Path,
    cache: &PreparedShellProfileCache,
    operation: &RunnerJobOperation,
    stop: &AtomicBool,
    on_started: &dyn Fn(),
) -> (ShellCommandResult, Option<Vec<ProjectBuildArtifact>>) {
    let started = Instant::now();
    let RunnerJobOperation::StartBuild(build) = operation else {
        unreachable!("Rust build operation");
    };
    let request = &build.provenance.request;
    let cwd = build.cwd.as_deref().expect("admitted build cwd");
    let deadline =
        started + Duration::from_secs(build.timeout_secs.min(policy.max_timeout_secs).max(1));
    let fence = || super::fence(policy, registry, operation);
    let mut packages = None;
    let mut root = None;
    if request.artifact_policy.is_some() {
        let project_root = match resolve_runner_project(registry, &request.project_id)
            .and_then(|root| validate_project_root(policy, &root))
        {
            Ok(root) => root,
            Err(_) => {
                return (
                    failure("build_artifact_project_unavailable".into(), started),
                    None,
                )
            }
        };
        let args = [
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ]
        .map(str::to_owned);
        let mut metadata = crate::webcodex_runner::shell::run_rust_build_phase(
            generation, policy, shell, registry, cache, cwd, &args, deadline, stop, None, &fence,
        );
        if metadata.execution_state != ShellCommandExecutionState::Completed
            || metadata.result.exit_code != Some(0)
            || metadata.result.error.is_some()
        {
            // This was metadata preparation, never a successful requested build.
            metadata
                .result
                .error
                .get_or_insert("build_artifact_metadata_failed".into());
            return (metadata, None);
        }
        let selected = parsed_output(&metadata)
            .and_then(|text| selected_packages(&project_root, Path::new(cwd), request, text));
        match selected {
            Ok(selected) => {
                let identity =
                    match crate::webcodex_runner::file_access::directory_identity(&project_root) {
                        Ok(identity) => identity,
                        Err(_) => {
                            return (
                                failure("build_artifact_project_unavailable".into(), started),
                                None,
                            )
                        }
                    };
                packages = Some(selected);
                root = Some((project_root, identity));
            }
            Err(error) => {
                metadata.result.error = Some(error);
                return (metadata, None);
            }
        }
    }
    let mut result = crate::webcodex_runner::shell::run_rust_build_phase(
        generation,
        policy,
        shell,
        registry,
        cache,
        cwd,
        &build.process.args,
        deadline,
        stop,
        Some(on_started),
        &fence,
    );
    if result.execution_state != ShellCommandExecutionState::Completed
        || result.result.exit_code != Some(0)
        || result.result.error.is_some()
        || stop.load(Ordering::SeqCst)
    {
        return (result, None);
    }
    let Some(packages) = packages else {
        return (result, None);
    };
    let (root, root_identity) = root.expect("metadata root");
    let check_read_owner = || {
        let current = resolve_runner_project(registry, &request.project_id)
            .and_then(|root| validate_project_root(policy, &root))
            .map_err(|_| evidence_error("build_artifact_project_unavailable"))?;
        if current != root
            || crate::webcodex_runner::file_access::directory_identity(&current)
                .map_err(|_| evidence_error("build_artifact_project_unavailable"))?
                != root_identity
        {
            return Err(evidence_error("build_artifact_project_changed"));
        }
        Ok(())
    };
    let artifacts = (|| {
        // Cargo may legitimately create/update Cargo.lock. Post-build reads
        // recheck Project ownership rather than misclassify its own effects as
        // stale pre-execution provenance.
        check_read_owner()?;
        let paths = artifact_paths(&root, request, &packages, parsed_output(&result)?)?;
        let mut artifacts = Vec::new();
        for path in paths {
            if stop.load(Ordering::SeqCst) {
                return Err(evidence_error("build_stopped"));
            }
            if Instant::now() >= deadline {
                return Err(evidence_error("build_timeout"));
            }
            artifacts.push(
                crate::webcodex_runner::artifacts::observe_cargo_build_artifact(
                    &root, &path, deadline, stop,
                )?,
            );
            if !webcodex_core::project_build::validate_cargo_build_artifacts(&artifacts) {
                return Err(evidence_error("build_artifacts_limit_exceeded"));
            }
        }
        if !webcodex_core::project_build::validate_cargo_build_artifacts(&artifacts) {
            return Err(evidence_error("build_artifacts_limit_exceeded"));
        }
        check_read_owner()?;
        Ok(artifacts)
    })();
    result.result.duration_ms = Some(started.elapsed().as_millis() as u64);
    match artifacts {
        Ok(artifacts) => (result, Some(artifacts)),
        Err(error) => {
            result.result.error = Some(error);
            (result, None)
        }
    }
}

#[cfg(test)]
#[path = "rust_tests.rs"]
mod tests;

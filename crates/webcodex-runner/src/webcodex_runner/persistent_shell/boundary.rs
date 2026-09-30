//! Raw-shell admission and current project/cwd/profile boundaries.

use super::*;

pub(super) fn validate_boundary(
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    client_id: &str,
    operation: &PersistentShellRequest,
) -> Result<RunnerProjectShellContext, (&'static str, String)> {
    if !policy.allow_raw_shell {
        return Err((
            "raw_shell_disabled",
            "persistent shells are disabled by the current Runner raw shell policy".to_string(),
        ));
    }
    validate_shell_config(shell).map_err(|message| ("persistent_shell_config_invalid", message))?;
    let prefix = format!("agent:{client_id}:");
    let project_id = operation
        .runtime_project_id
        .strip_prefix(&prefix)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            (
                "persistent_shell_project_mismatch",
                "runtime project does not belong to this Runner".to_string(),
            )
        })?;
    find_project_shell_context_by_id(project_registry_dir, project_id).ok_or_else(|| {
        (
            "persistent_shell_project_unavailable",
            "project is disabled, unregistered, or not executable by this Runner".to_string(),
        )
    })
}

pub(super) fn resolve_cwd(
    project: &RunnerProjectShellContext,
    requested: Option<&str>,
) -> Result<PathBuf, (&'static str, String)> {
    let root = PathBuf::from(&project.path)
        .canonicalize()
        .map_err(|error| {
            (
                "persistent_shell_project_unavailable",
                format!("failed to access project root: {error}"),
            )
        })?;
    let requested = requested
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let requested = if requested.is_absolute() {
        requested
    } else {
        root.join(requested)
    };
    let cwd = requested.canonicalize().map_err(|error| {
        (
            "persistent_shell_cwd_invalid",
            format!("failed to access persistent shell cwd: {error}"),
        )
    })?;
    if cwd != root && !cwd.starts_with(&root) {
        return Err((
            "persistent_shell_cwd_outside_project",
            "persistent shell cwd is outside the registered project root".to_string(),
        ));
    }
    Ok(cwd)
}

pub(super) fn validate_open_shell_boundary(
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project: &RunnerProjectShellContext,
    summary: &ShellSummary,
) -> Result<(), (&'static str, String)> {
    let cwd = resolve_cwd(project, Some(&summary.cwd.to_string_lossy()))?;
    cwd_allowed(policy, &cwd).map_err(|message| ("persistent_shell_cwd_denied", message))?;
    let (profile_name, _) = selected_profile(shell, project)?;
    if summary.profile != profile_name {
        return Err((
            "shell_reset_required",
            "the project shell profile changed; close and reopen the persistent shell".to_string(),
        ));
    }
    Ok(())
}

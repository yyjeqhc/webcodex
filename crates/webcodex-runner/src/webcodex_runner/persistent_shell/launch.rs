//! Local profile selection and platform-specific launch preparation.

use super::*;

pub(super) fn selected_profile<'a>(
    shell: &'a ShellConfig,
    project: &RunnerProjectShellContext,
) -> Result<(Option<String>, Option<&'a ShellProfileConfig>), (&'static str, String)> {
    let name = project
        .shell_profile
        .as_deref()
        .or(shell.default_profile.as_deref());
    match name {
        Some(name) => shell
            .profiles
            .get(name)
            .map(|profile| (Some(name.to_string()), Some(profile)))
            .ok_or_else(|| {
                (
                    "persistent_shell_profile_unavailable",
                    format!("shell profile '{name}' is not configured"),
                )
            }),
        None => Ok((None, None)),
    }
}

pub(super) fn build_launch(
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    client_id: &str,
    operation: &PersistentShellRequest,
    project: &RunnerProjectShellContext,
) -> Result<ShellLaunch, (&'static str, String)> {
    let cwd = resolve_cwd(project, operation.cwd.as_deref())?;
    cwd_allowed(policy, &cwd).map_err(|message| ("persistent_shell_cwd_denied", message))?;
    build_launch_at_cwd(
        shell,
        client_id,
        operation,
        project,
        cwd,
        policy.max_output_bytes,
    )
}

pub(super) fn build_launch_at_cwd(
    shell: &ShellConfig,
    client_id: &str,
    operation: &PersistentShellRequest,
    project: &RunnerProjectShellContext,
    cwd: PathBuf,
    max_output_bytes: usize,
) -> Result<ShellLaunch, (&'static str, String)> {
    let (profile_name, profile) = selected_profile(shell, project)?;
    let empty_profile = ShellProfileConfig::default();
    let env = base_shell_env(shell, profile.unwrap_or(&empty_profile))
        .map_err(|message| ("persistent_shell_environment_invalid", message))?;

    #[cfg(unix)]
    let (program, dialect, args, initialization) = {
        let explicit = operation.shell.as_deref();
        if explicit.is_some_and(|dialect| !matches!(dialect, "sh" | "bash")) {
            return Err((
                "persistent_shell_dialect_unsupported",
                "persistent shell must be 'sh' or 'bash'".to_string(),
            ));
        }
        let program = explicit
            .map(str::to_string)
            .or_else(|| profile.and_then(|profile| profile.program.clone()))
            .unwrap_or_else(|| shell.program.clone());
        let dialect = canonical_dialect(&program).ok_or_else(|| {
            (
                "persistent_shell_dialect_unsupported",
                "configured persistent shell program must resolve to sh or bash".to_string(),
            )
        })?;
        let initialization = match profile {
            Some(profile) => profile.init_script.clone(),
            None => shell
                .init_script
                .as_ref()
                .map(|path| format!(". {}", shell_quote(&path.to_string_lossy()))),
        };
        let args = if dialect == "bash" {
            vec!["--noprofile".to_string(), "--norc".to_string()]
        } else {
            Vec::new()
        };
        (program, dialect.to_string(), args, initialization)
    };

    #[cfg(windows)]
    let (program, dialect, args, initialization) = {
        if let Some(explicit) = operation.shell.as_deref() {
            return Err((
                "persistent_shell_dialect_unsupported",
                format!(
                    "Windows local persistent shell uses the configured PowerShell profile; explicit shell override '{explicit}' is unsupported"
                ),
            ));
        }
        let program = profile
            .and_then(|profile| profile.program.clone())
            .unwrap_or_else(|| shell.program.clone());
        let resolved_dialect = profile
            .and_then(|profile| profile.dialect)
            .or(shell.dialect)
            .or_else(|| dialect_for_program(&program))
            .unwrap_or_else(platform_default_dialect);
        if resolved_dialect != ShellDialect::PowerShell {
            return Err((
                "persistent_shell_dialect_unsupported",
                "Windows local persistent shell requires a configured PowerShell program/profile"
                    .to_string(),
            ));
        }
        let configured_args = profile
            .and_then(|profile| profile.args.clone())
            .unwrap_or_else(|| shell.args.clone());
        let args = windows_persistent_shell_prefix_args(&configured_args)?;
        let initialization = match profile {
            Some(profile) => profile.init_script.clone(),
            None => shell
                .init_script
                .as_ref()
                .map(|path| format!(". {}", shell_quote_powershell(&path.to_string_lossy()))),
        };
        (program, "powershell".to_string(), args, initialization)
    };

    #[cfg(not(any(unix, windows)))]
    return Err((
        "persistent_shell_unsupported",
        "local persistent shell is unsupported on this platform".to_string(),
    ));

    Ok(ShellLaunch {
        identity: ShellIdentity {
            shell_id: operation.shell_id.clone(),
            workflow_session_id: operation.workflow_session_id.clone(),
            runtime_project_id: operation.runtime_project_id.clone(),
            executor: EXECUTOR_AGENT.to_string(),
            client_id: Some(client_id.to_string()),
        },
        dialect,
        profile: profile_name,
        program,
        args,
        initial_cwd: cwd,
        env,
        initialization,
        max_output_bytes,
    })
}

#[cfg(windows)]
fn windows_persistent_shell_prefix_args(
    configured_args: &[String],
) -> Result<Vec<String>, (&'static str, String)> {
    let Some((command_flag, prefix)) = configured_args.split_last() else {
        return Err((
            "persistent_shell_config_invalid",
            "Windows PowerShell shell args must end with -Command".to_string(),
        ));
    };
    if !command_flag.eq_ignore_ascii_case("-Command") {
        return Err((
            "persistent_shell_config_invalid",
            "Windows PowerShell shell args must end with -Command so persistent-shell transport can replace the one-shot payload mode"
                .to_string(),
        ));
    }
    if prefix.iter().any(|arg| {
        matches!(
            arg.to_ascii_lowercase().as_str(),
            "-command" | "-encodedcommand" | "-file"
        )
    }) {
        return Err((
            "persistent_shell_config_invalid",
            "Windows PowerShell shell args contain a conflicting command/file payload switch"
                .to_string(),
        ));
    }
    Ok(prefix.to_vec())
}

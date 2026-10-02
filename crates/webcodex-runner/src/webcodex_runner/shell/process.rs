//! Structured process execution and detached launch preparation.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_process_with_profiles_and_execution_state(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> ShellCommandResult {
    run_process_with_profiles_and_execution_state_with_start_hook(
        generation,
        policy,
        shell,
        project_registry_dir,
        cache,
        cwd,
        executable,
        args,
        stdin,
        timeout_secs,
        stop_requested,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_detached_process_launch(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    executable: &str,
    args: &[String],
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> Result<PreparedDetachedProcessLaunch, String> {
    // Detached structured execution has the same local policy boundary as the
    // ordinary native-argv path. This helper performs preparation only; it never
    // spawns the requested payload.
    if !policy.allow_raw_shell {
        return Err("structured process execution is disabled by local Runner policy".to_string());
    }
    let cwd_path = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    cwd_allowed(policy, &cwd_path)?;
    let timeout_secs = timeout_secs.min(policy.max_timeout_secs).max(1);
    let profile = resolve_prepared_shell_profile(
        generation,
        shell,
        project_registry_dir,
        &cwd_path,
        cwd.is_some(),
        cache,
        stop_requested,
    )?;
    let resolved_program =
        resolve_process_program(shell, profile.as_deref(), executable, Some(&cwd_path))?;
    // Validate the same batch argv contract before a detached Job is accepted.
    let _ = structured_process_command(&resolved_program, args, Some(&cwd_path))?;
    let resolved_program = resolved_program.into_string().map_err(|_| {
        "structured process executable resolved to a non-UTF-8 native path".to_string()
    })?;
    let cwd = cwd_path
        .to_str()
        .ok_or_else(|| "structured process cwd resolved to a non-UTF-8 native path".to_string())?
        .to_string();
    let env = match profile.as_deref() {
        Some(profile) => profile.env_snapshot.clone(),
        None => base_shell_env(shell, &ShellProfileConfig::default())?,
    };
    let mut env = env.into_iter().collect::<Vec<_>>();
    env.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(PreparedDetachedProcessLaunch {
        process: ShellProcessArgv {
            executable: resolved_program,
            args: args.to_vec(),
        },
        cwd,
        env,
        timeout_secs,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_process_with_profiles_and_execution_state_with_start_hook(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    on_started: Option<&dyn Fn()>,
) -> ShellCommandResult {
    run_process_with_profiles_and_execution_state_with_internal_env_and_start_hook(
        generation,
        policy,
        shell,
        project_registry_dir,
        cache,
        cwd,
        executable,
        args,
        stdin,
        timeout_secs,
        stop_requested,
        &[],
        on_started,
    )
}

/// Apply Runner-owned semantic environment overrides after shell/profile
/// preparation and before native spawn. These values are internal invariants,
/// never caller/model-provided environment input.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_process_with_profiles_and_execution_state_with_internal_env_and_start_hook(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    env_overrides: &[(&str, &str)],
    on_started: Option<&dyn Fn()>,
) -> ShellCommandResult {
    // Structured execution intentionally receives the same policy treatment
    // as run_shell. Absence of shell syntax is not a permission bypass.
    if !policy.allow_raw_shell {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("structured process execution is disabled by local Runner policy".into()),
        });
    }
    let cwd_path = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    if let Err(error) = cwd_allowed(policy, &cwd_path) {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(error),
        });
    }
    let timeout_secs = timeout_secs.min(policy.max_timeout_secs).max(1);
    let start = Instant::now();
    let profile = match resolve_prepared_shell_profile(
        generation,
        shell,
        project_registry_dir,
        &cwd_path,
        cwd.is_some(),
        cache,
        stop_requested,
    ) {
        Ok(profile) => profile,
        Err(error) => {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(error),
            })
        }
    };
    let mut cmd = match configured_process_command(
        shell,
        profile.as_deref(),
        executable,
        args,
        Some(&cwd_path),
    ) {
        Ok(cmd) => cmd,
        Err(error) => {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(error),
            })
        }
    };
    for (key, value) in env_overrides {
        cmd.env(key, value);
    }
    execute_configured_command(
        policy,
        cmd,
        &cwd_path,
        stdin,
        timeout_secs,
        stop_requested,
        start,
        "failed to spawn structured process",
        on_started,
    )
}

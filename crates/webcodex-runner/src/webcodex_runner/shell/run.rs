//! Raw-shell admission and profile-aware entry points.

use super::*;

// Test-only wrapper for callers that do not need prepared shell profiles; the
// production request path uses `run_shell_with_profiles` directly.
#[cfg(test)]
pub(crate) fn run_shell(
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> CommandResult {
    run_shell_impl(
        policy,
        shell,
        None,
        cwd,
        command,
        None,
        false,
        stdin,
        timeout_secs,
        stop_requested,
    )
    .result
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_shell_with_profiles(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    command: &str,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> CommandResult {
    run_shell_with_profiles_and_execution_state(
        generation,
        policy,
        shell,
        project_registry_dir,
        cache,
        cwd,
        command,
        None,
        false,
        stdin,
        timeout_secs,
        stop_requested,
    )
    .result
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_shell_with_profiles_and_execution_state(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    command: &str,
    explicit_shell: Option<ExecutionShell>,
    login: bool,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> ShellCommandResult {
    run_shell_impl(
        policy,
        shell,
        Some((generation, project_registry_dir, cache)),
        cwd,
        command,
        explicit_shell,
        login,
        stdin,
        timeout_secs,
        stop_requested,
    )
}

pub(super) fn run_shell_impl(
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    profiles: Option<(u64, &Path, &PreparedShellProfileCache)>,
    cwd: Option<&str>,
    command: &str,
    explicit_shell: Option<ExecutionShell>,
    login: bool,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> ShellCommandResult {
    if login && explicit_shell != Some(ExecutionShell::Bash) {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("bash login mode requires shell=bash".to_string()),
        });
    }
    if !policy.allow_raw_shell {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("raw shell is disabled by local Runner policy".to_string()),
        });
    }
    let cwd_path = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    if let Err(e) = cwd_allowed(policy, &cwd_path) {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some(e),
        });
    }
    let timeout_secs = timeout_secs.min(policy.max_timeout_secs).max(1);
    let start = Instant::now();
    let mut prepared_profile_name = None;
    let cmd = match profiles {
        Some((generation, project_registry_dir, cache)) => match resolve_prepared_shell_profile(
            generation,
            shell,
            project_registry_dir,
            &cwd_path,
            cwd.is_some(),
            cache,
            stop_requested,
        ) {
            Ok(Some(profile)) => {
                let configured = match explicit_shell {
                    Some(selection) => configured_explicit_shell_command(
                        shell,
                        Some(&profile),
                        selection,
                        login,
                        command,
                    ),
                    None => configured_prepared_shell_command(&profile, command),
                };
                match configured {
                    Ok(cmd) => {
                        prepared_profile_name = Some(profile.profile_name.clone());
                        cmd
                    }
                    Err(e) => {
                        return ShellCommandResult::not_started(CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(format!(
                                "failed to configure shell profile '{}': {}",
                                profile.profile_name, e
                            )),
                        })
                    }
                }
            }
            Ok(None) => {
                let configured = match explicit_shell {
                    Some(selection) => {
                        configured_explicit_shell_command(shell, None, selection, login, command)
                    }
                    None => configured_shell_command(shell, command),
                };
                match configured {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        return ShellCommandResult::not_started(CommandResult {
                            exit_code: None,
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(start.elapsed().as_millis() as u64),
                            error: Some(e),
                        })
                    }
                }
            }
            Err(e) => {
                return ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(e),
                })
            }
        },
        None => {
            let configured = match explicit_shell {
                Some(selection) => {
                    configured_explicit_shell_command(shell, None, selection, login, command)
                }
                None => configured_shell_command(shell, command),
            };
            match configured {
                Ok(cmd) => cmd,
                Err(e) => {
                    return ShellCommandResult::not_started(CommandResult {
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        error: Some(e),
                    })
                }
            }
        }
    };
    let spawn_error_prefix = prepared_profile_name
        .as_deref()
        .map(|profile_name| format!("failed to spawn shell profile '{profile_name}'"))
        .unwrap_or_else(|| "failed to spawn command".to_string());
    execute_configured_command(
        policy,
        cmd,
        &cwd_path,
        stdin,
        timeout_secs,
        stop_requested,
        start,
        &spawn_error_prefix,
        None,
    )
}

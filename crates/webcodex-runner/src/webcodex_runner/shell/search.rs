//! Native Windows interpreter and bounded project search fallback.

use super::*;

#[cfg(windows)]
pub(super) fn is_windows_wsl_bash_launcher(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    normalized.ends_with("\\windows\\system32\\bash.exe")
        || normalized.ends_with("\\windows\\sysnative\\bash.exe")
        || normalized.ends_with("\\windows\\syswow64\\bash.exe")
        || normalized.ends_with("\\microsoft\\windowsapps\\bash.exe")
}

#[cfg(windows)]
pub(super) fn resolve_windows_internal_posix_interpreter(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
) -> Result<OsString, String> {
    let path = configured_process_path(shell, profile)?;

    // Keep generated Git/workspace programs on the same native Windows toolchain
    // as the `git.exe` visible to the Runner. Git for Windows normally exposes
    // `cmd\\git.exe` on PATH while its Bash lives in `bin\\bash.exe`; MSYS/Cygwin
    // layouts commonly keep both executables in the same `bin` directory.
    if let Some(crate::webcodex_runner::util::ResolvedProgram::Native(git)) =
        crate::webcodex_runner::util::resolve_program_in_path("git", &path)
    {
        let mut candidates = Vec::new();
        if let Some(parent) = git.parent() {
            let parent_name = parent
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if parent_name.eq_ignore_ascii_case("cmd") {
                if let Some(root) = parent.parent() {
                    candidates.push(root.join("bin").join("bash.exe"));
                    candidates.push(root.join("usr").join("bin").join("bash.exe"));
                }
            } else if parent_name.eq_ignore_ascii_case("bin") {
                candidates.push(parent.join("bash.exe"));
                if let Some(root) = parent.parent() {
                    candidates.push(root.join("usr").join("bin").join("bash.exe"));
                }
            }
        }
        for candidate in candidates {
            if is_windows_wsl_bash_launcher(&candidate) {
                continue;
            }
            if let Some(crate::webcodex_runner::util::ResolvedProgram::Native(resolved)) =
                crate::webcodex_runner::util::resolve_program_in_path(
                    &candidate.to_string_lossy(),
                    OsStr::new(""),
                )
            {
                return Ok(resolved.into_os_string());
            }
        }
    }

    // A standalone native Windows Bash remains valid for non-Git internal work,
    // but the Windows-provided `bash.exe` / app-execution alias is a WSL launcher.
    // Entering WSL changes cwd/Git/environment semantics and is therefore not a
    // valid runtime for Runner-generated programs over a native Windows project.
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join("bash.exe");
        if is_windows_wsl_bash_launcher(&candidate) {
            continue;
        }
        if let Some(crate::webcodex_runner::util::ResolvedProgram::Native(resolved)) =
            crate::webcodex_runner::util::resolve_program_in_path(
                &candidate.to_string_lossy(),
                OsStr::new(""),
            )
        {
            return Ok(resolved.into_os_string());
        }
    }

    Err(
        "interpreter_unavailable: native Windows Bash interpreter is unavailable; WSL bash launchers are not valid for Runner-generated internal programs; command was not started"
            .to_string(),
    )
}

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeSearchSpec {
    pub(super) path: String,
    pub(super) args: Vec<String>,
    pub(super) line_limit: usize,
}

#[cfg(windows)]
pub(super) fn native_search_spec(payload: &str) -> Option<NativeSearchSpec> {
    use webcodex_core::project_search::{
        project_text_search_line_budget, project_text_search_rg_options,
    };
    let payload: serde_json::Value = serde_json::from_str(payload).ok()?;
    let object = payload.as_object()?;
    let path = object.get("path")?.as_str()?.trim();
    if path.is_empty()
        || path.contains('\0')
        || Path::new(path).components().any(|component| {
            matches!(
                component,
                std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
                    | std::path::Component::ParentDir
            )
        })
    {
        return None;
    }
    let globs = |field: &str| -> Option<Vec<&str>> {
        match object.get(field) {
            None | Some(serde_json::Value::Null) => Some(Vec::new()),
            Some(value) => value
                .as_array()?
                .iter()
                .map(|value| {
                    let glob = value.as_str()?;
                    (!glob.is_empty() && !glob.contains('\0')).then_some(glob)
                })
                .collect(),
        }
    };
    let include = globs("include_globs")?;
    let exclude = globs("exclude_globs")?;

    let pattern = object.get("pattern")?.as_str()?;
    let limit = usize::try_from(object.get("limit")?.as_u64()?).ok()?.max(1);
    let before = usize::try_from(object.get("context_before")?.as_u64()?).ok()?;
    let after = usize::try_from(object.get("context_after")?.as_u64()?).ok()?;
    let result_mode = object.get("result_mode")?.as_str()?;

    let mut args = project_text_search_rg_options(
        result_mode,
        before,
        after,
        &include,
        &exclude,
        str::to_string,
    )?;
    args.extend(["--path-separator".to_string(), "/".to_string()]);
    args.extend([
        "-e".to_string(),
        pattern.to_string(),
        "--".to_string(),
        path.to_string(),
    ]);
    Some(NativeSearchSpec {
        path: path.to_string(),
        args,
        line_limit: project_text_search_line_budget(limit, result_mode == "matches", before, after),
    })
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_windows_native_search_with_profiles(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    payload: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> Option<ShellCommandResult> {
    use webcodex_core::project_search::PROJECT_TEXT_SEARCH_OUTPUT_BYTE_BUDGET;
    const MARKER: &str =
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n";
    // Native execution has the same policy gate as generated internal POSIX
    // programs. Small transport budgets keep the existing fallback behavior.
    if !policy.allow_raw_shell
        || policy.max_output_bytes < MARKER.len() + PROJECT_TEXT_SEARCH_OUTPUT_BYTE_BUDGET + 4
    {
        return None;
    }
    let spec = native_search_spec(payload?)?;
    let timeout_secs = timeout_secs.min(policy.max_timeout_secs).max(1);
    let cwd_path = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    if cwd_allowed(policy, &cwd_path).is_err() {
        return None;
    }
    let canonical_cwd = canonicalize_existing(&cwd_path).ok()?;
    let canonical_target = canonicalize_existing(&cwd_path.join(&spec.path)).ok()?;
    if !(canonical_target.is_file() || canonical_target.is_dir())
        || !webcodex_runner_config::paths::path_is_within(&canonical_target, &canonical_cwd)
    {
        return None;
    }

    let profile = resolve_prepared_shell_profile(
        generation,
        shell,
        project_registry_dir,
        &cwd_path,
        cwd.is_some(),
        cache,
        stop_requested,
    )
    .ok()?;
    let path = configured_process_path(shell, profile.as_deref()).ok()?;
    let resolved = crate::webcodex_runner::util::resolve_program_in_path("rg", &path)
        .or_else(|| crate::webcodex_runner::util::resolve_program_in_path("rg.exe", &path))?;
    let crate::webcodex_runner::util::ResolvedProgram::Native(program) = resolved else {
        return None;
    };

    let mut command = Command::new(program);
    command.args(&spec.args);
    match profile.as_deref() {
        Some(profile) => apply_env_snapshot(&mut command, &profile.env_snapshot),
        None => {
            if apply_shell_environment(&mut command, shell).is_err() {
                return None;
            }
        }
    }
    let start = Instant::now();
    let mut result = super::execution::execute_configured_command_with_prefix(
        policy,
        command,
        &cwd_path,
        None,
        timeout_secs,
        stop_requested,
        start,
        "failed to spawn native ripgrep search",
        None,
        Some(super::drains::StdoutPrefixLimit {
            lines: spec.line_limit,
            bytes: PROJECT_TEXT_SEARCH_OUTPUT_BYTE_BUDGET + 1,
        }),
    );
    if result.execution_state == ShellCommandExecutionState::NotStarted {
        return None;
    }
    if result.stdout_truncated {
        return None;
    }
    if result.execution_state == ShellCommandExecutionState::Completed {
        let stdout = result.result.stdout.take().unwrap_or_default();
        if MARKER.len().saturating_add(stdout.len()) > policy.max_output_bytes {
            return None;
        }
        result.result.stdout = Some(format!("{MARKER}{stdout}"));
    }
    Some(result)
}

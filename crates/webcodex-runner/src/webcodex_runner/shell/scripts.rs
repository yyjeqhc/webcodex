//! Script interpreter selection, temporary payloads, and execution.

use super::*;

pub(super) fn configured_script_interpreter(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    language: ShellScriptLanguage,
) -> Result<OsString, String> {
    let configured_program = profile
        .map(|profile| profile.program.as_str())
        .unwrap_or(shell.program.as_str());
    let configured_basename = Path::new(configured_program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(configured_program)
        .to_ascii_lowercase();
    let configured_matches = match language {
        ShellScriptLanguage::Sh => matches!(configured_basename.as_str(), "sh" | "sh.exe"),
        ShellScriptLanguage::Bash => {
            matches!(configured_basename.as_str(), "bash" | "bash.exe")
        }
        ShellScriptLanguage::Powershell if cfg!(windows) => matches!(
            configured_basename.as_str(),
            "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
        ),
        ShellScriptLanguage::Powershell => {
            matches!(configured_basename.as_str(), "pwsh" | "pwsh.exe")
        }
        ShellScriptLanguage::Python => {
            matches!(
                configured_basename.as_str(),
                "python3" | "python3.exe" | "python" | "python.exe"
            )
        }
        ShellScriptLanguage::Javascript | ShellScriptLanguage::Typescript => {
            matches!(configured_basename.as_str(), "node" | "node.exe")
        }
    };
    let mut candidates = Vec::new();
    if configured_matches {
        candidates.push(configured_program.to_string());
    }
    match language {
        ShellScriptLanguage::Sh => candidates.push("sh".to_string()),
        ShellScriptLanguage::Bash => candidates.push("bash".to_string()),
        ShellScriptLanguage::Powershell if cfg!(windows) => {
            candidates.push("pwsh".to_string());
            candidates.push("powershell".to_string());
        }
        ShellScriptLanguage::Powershell => candidates.push("pwsh".to_string()),
        ShellScriptLanguage::Python => {
            if cfg!(windows) {
                candidates.extend(["python".to_string(), "python3".to_string()]);
            } else {
                candidates.extend(["python3".to_string(), "python".to_string()]);
            }
        }
        ShellScriptLanguage::Javascript | ShellScriptLanguage::Typescript => {
            candidates.push("node".to_string())
        }
    }
    candidates.dedup_by(|left, right| {
        if cfg!(windows) {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    });
    let path = configured_process_path(shell, profile)?;
    for candidate in candidates {
        if let Some(crate::webcodex_runner::util::ResolvedProgram::Native(path)) =
            crate::webcodex_runner::util::resolve_program_in_path(&candidate, &path)
        {
            #[cfg(windows)]
            if language == ShellScriptLanguage::Python
                && std::fs::symlink_metadata(&path)
                    .ok()
                    .is_none_or(|metadata| {
                        crate::webcodex_runner::configured_skills::metadata_is_link_like(&metadata)
                    })
            {
                // Windows App Execution Aliases are link-like launch stubs, not
                // a proven Python interpreter. Keep interpreter admission exact.
                continue;
            }
            return Ok(path.into_os_string());
        }
    }
    let interpreter_name = match language {
        ShellScriptLanguage::Javascript => "JavaScript/Node",
        ShellScriptLanguage::Typescript => "TypeScript/Node",
        _ => language.as_str(),
    };
    Err(format!(
        "interpreter_unavailable: {interpreter_name} interpreter is unavailable; command was not started"
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NodeVersion {
    pub(super) major: u64,
    pub(super) minor: u64,
    pub(super) patch: u64,
}

pub(super) fn parse_node_version(output: &[u8]) -> Option<NodeVersion> {
    let version = std::str::from_utf8(output).ok()?.trim();
    let version = version.strip_prefix('v').unwrap_or(version);
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.split('-').next()?.parse().ok()?;
    Some(NodeVersion {
        major,
        minor,
        patch,
    })
}

pub(super) fn typescript_node_prefix_args(version: NodeVersion) -> Result<Vec<OsString>, String> {
    if version.major < 22 || (version.major == 22 && version.minor < 6) {
        return Err(format!(
            "interpreter_unavailable: TypeScript requires Node.js 22.6.0 or newer with native type stripping; found Node.js v{}.{}.{}; command was not started",
            version.major, version.minor, version.patch
        ));
    }
    let needs_enable_flag =
        (version.major == 22 && version.minor < 18) || (version.major == 23 && version.minor < 6);
    Ok(if needs_enable_flag {
        vec![OsString::from("--experimental-strip-types")]
    } else {
        Vec::new()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScriptRuntimePlan {
    pub(super) program: OsString,
    pub(super) prefix_args: Vec<OsString>,
}

pub(super) fn fixed_script_prefix_args(language: ShellScriptLanguage) -> Vec<OsString> {
    if language != ShellScriptLanguage::Powershell {
        return Vec::new();
    }
    let mut args = vec![
        OsString::from("-NoProfile"),
        OsString::from("-NonInteractive"),
    ];
    if cfg!(windows) {
        // Match the Runner's existing Windows PowerShell policy: a process-scoped
        // bypass keeps Runner-owned temporary .ps1 files executable under the
        // stock Restricted machine policy.
        args.extend([OsString::from("-ExecutionPolicy"), OsString::from("Bypass")]);
    }
    args.push(OsString::from("-File"));
    args
}

pub(super) fn apply_script_environment(
    command: &mut Command,
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
) -> Result<(), String> {
    match profile {
        Some(profile) => {
            apply_env_snapshot(command, &profile.env_snapshot);
            Ok(())
        }
        None => apply_shell_environment(command, shell),
    }
}

pub(super) fn typescript_node_probe_error(error: String) -> String {
    if error.contains("stopped during runner shutdown") {
        "TypeScript runtime probe stopped during runner shutdown; command was not started"
            .to_string()
    } else {
        "interpreter_unavailable: unable to verify Node.js native TypeScript support; command was not started"
            .to_string()
    }
}

pub(super) fn configured_script_runtime_plan(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    language: ShellScriptLanguage,
    cwd: &Path,
    stop_requested: Option<&AtomicBool>,
) -> Result<ScriptRuntimePlan, String> {
    let program = configured_script_interpreter(shell, profile, language)?;
    let mut prefix_args = fixed_script_prefix_args(language);
    if language == ShellScriptLanguage::Typescript {
        let mut probe = Command::new(&program);
        // This Runner-owned probe has no input contract. In particular, never
        // inherit the Runner's parent-liveness stdin or consume its input.
        probe.arg("--version").current_dir(cwd).stdin(Stdio::null());
        apply_script_environment(&mut probe, shell, profile)?;
        let probe_result =
            run_prepare_command(probe, TYPESCRIPT_NODE_VERSION_PROBE_TIMEOUT, stop_requested);
        let (status, stdout, _stderr) = probe_result.map_err(typescript_node_probe_error)?;
        if !status.success() {
            return Err(
                "interpreter_unavailable: unable to verify Node.js native TypeScript support; command was not started"
                    .to_string(),
            );
        }
        let version = parse_node_version(&stdout).ok_or_else(|| {
            "interpreter_unavailable: Node.js returned an unrecognized version while checking native TypeScript support; command was not started"
                .to_string()
        })?;
        prefix_args.extend(typescript_node_prefix_args(version)?);
    }
    Ok(ScriptRuntimePlan {
        program,
        prefix_args,
    })
}

pub(super) fn build_script_command(
    plan: &ScriptRuntimePlan,
    script_path: &Path,
    args: &[String],
) -> Command {
    let mut command = Command::new(&plan.program);
    command.args(&plan.prefix_args).arg(script_path).args(args);
    command
}

pub(super) fn script_setup_error(action: &str, error: &std::io::Error) -> String {
    format!(
        "script_setup_failed: failed to {action} Runner-owned temporary script file ({:?}); command was not started",
        error.kind()
    )
}

pub(super) fn create_temporary_script(
    payload: &ShellScriptPayload,
) -> Result<(tempfile::TempPath, PathBuf, PathBuf), String> {
    let mut builder = tempfile::Builder::new();
    builder
        .prefix("webcodex-script-")
        .suffix(payload.language.file_extension());
    let mut file = builder
        .tempfile()
        .map_err(|error| script_setup_error("create", &error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| script_setup_error("secure", &error))?;
    }
    if payload.language == ShellScriptLanguage::Powershell {
        // Windows PowerShell 5.1 needs a UTF-8 BOM to preserve arbitrary
        // Unicode in script files. pwsh also accepts it. The BOM is encoding
        // metadata, so a leading param(...) block remains the first script
        // construct and no behavioral preamble is injected.
        file.write_all(&[0xEF, 0xBB, 0xBF])
            .map_err(|error| script_setup_error("write", &error))?;
    }
    file.write_all(payload.script.as_bytes())
        .and_then(|_| file.flush())
        .map_err(|error| script_setup_error("write", &error))?;
    let original_path = file.path().to_path_buf();
    // Avoid `canonicalize` here: on Windows it commonly adds a `\\?\` prefix
    // that Windows PowerShell 5.1 does not reliably accept for `-File`.
    let absolute_path = if file.path().is_absolute() {
        file.path().to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(file.path()))
            .map_err(|error| script_setup_error("resolve", &error))?
    };
    Ok((file.into_temp_path(), original_path, absolute_path))
}

pub(super) fn redact_temporary_script_path(result: &mut ShellCommandResult, paths: &[&Path]) {
    for value in [
        result.result.stdout.as_mut(),
        result.result.stderr.as_mut(),
        result.result.error.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        for path in paths {
            let rendered = path.to_string_lossy();
            if !rendered.is_empty() {
                *value = value.replace(rendered.as_ref(), "<temporary-script>");
                let alternate = if rendered.contains('\\') {
                    rendered.replace('\\', "/")
                } else {
                    rendered.replace('/', "\\")
                };
                if alternate != rendered {
                    *value = value.replace(&alternate, "<temporary-script>");
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_script_with_profiles_and_execution_state(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    payload: &ShellScriptPayload,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
) -> ShellCommandResult {
    run_script_with_profiles_and_execution_state_with_start_hook(
        generation,
        policy,
        shell,
        project_registry_dir,
        cache,
        cwd,
        payload,
        stdin,
        timeout_secs,
        stop_requested,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_script_with_profiles_and_execution_state_with_start_hook(
    generation: u64,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    payload: &ShellScriptPayload,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    on_started: Option<&dyn Fn()>,
) -> ShellCommandResult {
    // Typed script execution is consequential and receives the same Runner
    // policy treatment as raw shell and structured native processes.
    if !policy.allow_raw_shell {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("structured script execution is disabled by local Runner policy".into()),
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
    // Resolve the semantic runtime plan before creating the payload file. Missing
    // interpreters and unsupported TypeScript Node versions are therefore
    // definite pre-start rejections with no user-script side effect. TypeScript
    // performs only a bounded Runner-owned `node --version` capability probe;
    // the user's script body is still spawned exactly once.
    let runtime_plan = match configured_script_runtime_plan(
        shell,
        profile.as_deref(),
        payload.language,
        &cwd_path,
        stop_requested,
    ) {
        Ok(plan) => plan,
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
    let (temporary_path, original_path, absolute_path) = match create_temporary_script(payload) {
        Ok(temporary) => temporary,
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
    let mut command = build_script_command(&runtime_plan, &absolute_path, &payload.args);
    if let Err(error) = apply_script_environment(&mut command, shell, profile.as_deref()) {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(error),
        });
    }
    let mut result = execute_configured_command(
        policy,
        command,
        &cwd_path,
        stdin,
        timeout_secs,
        stop_requested,
        start,
        "failed to spawn script interpreter",
        on_started,
    );
    redact_temporary_script_path(&mut result, &[original_path.as_path(), &absolute_path]);
    if let Err(error) = temporary_path.close() {
        // Cleanup is infrastructure after the child result is already known.
        // Never rewrite completed/timed_out/outcome_unknown lifecycle truth.
        tracing::warn!(
            language = payload.language.as_str(),
            error_kind = ?error.kind(),
            "failed to remove Runner-owned temporary script file"
        );
    }
    result
}

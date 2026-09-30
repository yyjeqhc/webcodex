//! Shell/process command construction and executable resolution.

use super::*;

/// POSIX sh single-quote escaping.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// PowerShell single-quote escaping (an embedded single quote is doubled).
/// PowerShell's single-quoted strings are literal, so spaces, backslashes,
/// double quotes, `$`, Unicode, and `C:\...` Windows paths need no further
/// escaping; only `'` does.
pub(crate) fn shell_quote_powershell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Deterministic UTF-8 setup for redirected PowerShell output. Bounded to the
/// child process: when stdout is redirected, .NET only caches these encodings
/// instead of calling SetConsoleOutputCP, so the parent Runner console state
/// is never mutated. PowerShell 5.1 otherwise writes through the console code
/// page (OEM), which would corrupt Unicode output and the env snapshot.
pub(super) const POWERSHELL_UTF8_PREAMBLE: &str = concat!(
    "try { $OutputEncoding = [Console]::InputEncoding = ",
    "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false) } catch { }",
);

/// Wrap `command` so the PowerShell process exits with a meaningful status:
/// the shell's own `exit N` statements pass through, a failing trailing native
/// command is reported through `$LASTEXITCODE`, a failing PowerShell statement
/// returns 1, and a successful trailing PowerShell statement returns 0 even if
/// an earlier native command left a stale non-zero `$LASTEXITCODE` behind.
/// PowerShell 5.1 does not propagate these statuses consistently on its own,
/// so inspect `$?` immediately after the requested command and exit explicitly.
pub(super) fn powershell_command_text(command: &str) -> String {
    format!(
        "{POWERSHELL_UTF8_PREAMBLE}\n\
         $LASTEXITCODE = 0\n\
         {command}\n\
         if (-not $?) {{ if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; exit 1 }}\n\
         exit 0"
    )
}

/// Dot-source a shell init script, then run the command only after the init
/// script reported success. Native-command failure inside the init script
/// blocks the command (like POSIX `. <path> && (...)`); a terminating
/// PowerShell error aborts the whole script, which also blocks the command.
///
/// `$?` is inspected immediately after dot-sourcing so a failed dot-source
/// operation (for example, an unavailable script) blocks the command. Native
/// failure status is preserved through `$LASTEXITCODE`; ordinary PowerShell
/// non-terminating errors retain PowerShell's own dot-source semantics.
pub(super) fn powershell_init_command_text(init_script: &Path, command: &str) -> String {
    format!(
        "{POWERSHELL_UTF8_PREAMBLE}\n\
         . {}\n\
         if (-not $?) {{ if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; exit 1 }}\n\
         $LASTEXITCODE = 0\n\
         {command}\n\
         if (-not $?) {{ if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; exit 1 }}\n\
         exit 0",
        shell_quote_powershell(&init_script.to_string_lossy()),
    )
}

pub(super) fn shell_command_text(
    shell: &ShellConfig,
    dialect: ShellDialect,
    command: &str,
) -> String {
    match (dialect, shell.init_script.as_ref()) {
        (ShellDialect::Posix, Some(path)) => format!(
            ". {} && (\n{}\n)",
            shell_quote(&path.to_string_lossy()),
            command
        ),
        (ShellDialect::Posix, None) => command.to_string(),
        (ShellDialect::PowerShell, Some(path)) => powershell_init_command_text(path, command),
        (ShellDialect::PowerShell, None) => powershell_command_text(command),
    }
}

/// Command text for an already-prepared profile execution. POSIX shells get
/// the raw command (the last statement's status is the shell status); the
/// PowerShell wrapper adds the explicit exit-status propagation.
pub(super) fn prepared_shell_command_text(dialect: ShellDialect, command: &str) -> String {
    match dialect {
        ShellDialect::Posix => command.to_string(),
        ShellDialect::PowerShell => powershell_command_text(command),
    }
}

pub(super) fn configured_shell_command(
    shell: &ShellConfig,
    command: &str,
) -> Result<Command, String> {
    validate_shell_config(shell)?;
    let dialect = resolve_dialect(&shell.program, shell.dialect);
    let program = resolved_shell_program(&shell.program);
    let mut cmd = Command::new(program);
    for arg in &shell.args {
        cmd.arg(arg);
    }
    cmd.arg(shell_command_text(shell, dialect, command));
    // The shell execution path owns its process tree through ManagedChild; do
    // not add a process-group pre_exec here. ManagedChild creates the private
    // process group (Unix) / Job Object (Windows) at spawn time.
    apply_shell_environment(&mut cmd, shell)?;
    Ok(cmd)
}

pub(super) fn configured_prepared_shell_command(
    profile: &PreparedShellProfile,
    command: &str,
) -> Result<Command, String> {
    let mut cmd = Command::new(&profile.program);
    for arg in &profile.args {
        cmd.arg(arg);
    }
    cmd.arg(prepared_shell_command_text(profile.dialect, command));
    // The shell execution path owns its process tree through ManagedChild; do
    // not add a process-group pre_exec here. ManagedChild creates the private
    // process group (Unix) / Job Object (Windows) at spawn time.
    apply_env_snapshot(&mut cmd, &profile.env_snapshot);
    Ok(cmd)
}

/// Build one raw-shell command using the caller-selected semantic POSIX shell
/// directly rather than feeding a POSIX wrapper to the Runner's configured
/// shell. Prepared profiles still contribute their materialized environment.
pub(crate) fn configured_explicit_shell_command(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    selection: ExecutionShell,
    login: bool,
    command: &str,
) -> Result<Command, String> {
    if login && selection != ExecutionShell::Bash {
        return Err("bash login mode requires shell=bash".to_string());
    }
    let language = match selection {
        ExecutionShell::Sh => ShellScriptLanguage::Sh,
        ExecutionShell::Bash => ShellScriptLanguage::Bash,
    };
    let program = configured_script_interpreter(shell, profile, language)?;
    let mut cmd = Command::new(program);
    cmd.arg(if login { "-lc" } else { "-c" }).arg(command);
    match profile {
        Some(profile) => apply_env_snapshot(&mut cmd, &profile.env_snapshot),
        None => apply_shell_environment(&mut cmd, shell)?,
    }
    Ok(cmd)
}

pub(crate) fn configured_shell_job_command(
    shell: &ShellConfig,
    command: &str,
) -> Result<Command, String> {
    validate_shell_config(shell)?;
    let dialect = resolve_dialect(&shell.program, shell.dialect);
    let program = resolved_shell_program(&shell.program);
    let mut cmd = Command::new(program);
    for arg in &shell.args {
        cmd.arg(arg);
    }
    cmd.arg(shell_command_text(shell, dialect, command));
    // JobManager owns this process tree through ManagedChild; do not add
    // the legacy setsid pre_exec here. ManagedChild creates the private group.
    apply_shell_environment(&mut cmd, shell)?;
    Ok(cmd)
}

pub(crate) fn configured_prepared_shell_job_command(
    profile: &PreparedShellProfile,
    command: &str,
) -> Result<Command, String> {
    let mut cmd = Command::new(&profile.program);
    for arg in &profile.args {
        cmd.arg(arg);
    }
    cmd.arg(prepared_shell_command_text(profile.dialect, command));
    // JobManager owns this process tree through ManagedChild; do not add
    // the legacy setsid pre_exec here. ManagedChild creates the private group.
    apply_env_snapshot(&mut cmd, &profile.env_snapshot);
    Ok(cmd)
}

pub(crate) fn configured_validation_job_command(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    program: &str,
    args: &[String],
    cwd: &Path,
) -> Result<Command, String> {
    if profile.is_none() {
        validate_shell_config(shell)?;
    }
    configured_process_command(shell, profile, program, args, Some(cwd))
}

pub(super) fn configured_process_command(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
) -> Result<Command, String> {
    let resolved_program = resolve_process_program(shell, profile, program, cwd)?;
    let mut cmd = structured_process_command(&resolved_program, args, cwd)?;
    // ManagedChild (or JobManager for structured validation) owns this process
    // tree. Native argv stays literal; batch conversion belongs to the helper.
    match profile {
        Some(profile) => apply_env_snapshot(&mut cmd, &profile.env_snapshot),
        None => apply_shell_environment(&mut cmd, shell)?,
    }
    Ok(cmd)
}

pub(super) fn resolve_process_program(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    program: &str,
    cwd: Option<&Path>,
) -> Result<OsString, String> {
    #[cfg(windows)]
    {
        let path = configured_process_path(shell, profile)?;
        let program_path = Path::new(program);
        let resolved_input = if !program_path.is_absolute() && program_path.components().count() > 1
        {
            cwd.map(|cwd| cwd.join(program_path))
                .unwrap_or_else(|| program_path.to_path_buf())
                .to_string_lossy()
                .into_owned()
        } else {
            program.to_string()
        };
        match crate::webcodex_runner::util::resolve_program_in_path(&resolved_input, &path) {
            Some(crate::webcodex_runner::util::ResolvedProgram::Native(path)) => Ok(path.into_os_string()),
            Some(crate::webcodex_runner::util::ResolvedProgram::Batch(path)) => Ok(path.into_os_string()),
            None => Err(format!(
                "structured process executable is unavailable or has an unsupported Windows extension: {program}"
            )),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (shell, profile, cwd);
        Ok(OsString::from(program))
    }
}

pub(super) fn configured_process_path(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
) -> Result<OsString, String> {
    if let Some(profile) = profile {
        return Ok(env_lookup(&profile.env_snapshot, "PATH")
            .map(OsString::from)
            .unwrap_or_default());
    }
    if shell.environment_mode == ShellEnvironmentMode::Isolated {
        let env = base_shell_env(shell, &ShellProfileConfig::default())?;
        return Ok(env_lookup(&env, "PATH")
            .map(OsString::from)
            .unwrap_or_default());
    }
    if let Some(configured) = env_lookup(&shell.env, "PATH") {
        return Ok(OsString::from(configured));
    }
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    if shell.path_prepend.is_empty() {
        return Ok(inherited);
    }
    let mut paths = shell.path_prepend.clone();
    paths.extend(std::env::split_paths(&inherited));
    std::env::join_paths(paths)
        .map_err(|error| format!("failed to build process PATH from shell.path_prepend: {error}"))
}

pub(crate) fn explicit_shell_available(shell: &ShellConfig, language: ShellScriptLanguage) -> bool {
    matches!(
        language,
        ShellScriptLanguage::Sh | ShellScriptLanguage::Bash
    ) && configured_script_interpreter(shell, None, language).is_ok()
}

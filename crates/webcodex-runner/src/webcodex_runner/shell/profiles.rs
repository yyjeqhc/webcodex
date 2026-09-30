//! Profile initialization, environment snapshots, and generation cache.

use super::commands::POWERSHELL_UTF8_PREAMBLE;
use super::*;

pub(super) fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub(super) fn parse_env_payload(
    payload: &[u8],
    profile_name: &str,
) -> Result<HashMap<String, String>, String> {
    let mut env = HashMap::new();
    for entry in payload.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let Some(eq) = entry.iter().position(|byte| *byte == b'=') else {
            return Err(format!(
                "failed to parse env snapshot for profile '{}': entry missing '='",
                profile_name
            ));
        };
        let key = std::str::from_utf8(&entry[..eq]).map_err(|_| {
            format!(
                "failed to parse env snapshot for profile '{}': key is not UTF-8",
                profile_name
            )
        })?;
        if key.is_empty() {
            return Err(format!(
                "failed to parse env snapshot for profile '{}': empty env key",
                profile_name
            ));
        }
        let value = std::str::from_utf8(&entry[eq + 1..]).map_err(|_| {
            format!(
                "failed to parse env snapshot for profile '{}': value is not UTF-8",
                profile_name
            )
        })?;
        if should_inherit_env_key(key) {
            env.insert(key.to_string(), value.to_string());
        }
    }
    Ok(env)
}

/// POSIX profile-prepare script: `set -e`, the configured init snippet, a
/// marker line, then a NUL-delimited `env -0` dump. Unchanged from the legacy
/// Unix behavior.
pub(super) fn posix_profile_prepare_script(init_script: &str, marker: &str) -> String {
    format!(
        "set -e\n{}\nprintf '\\n{}\\n'\nenv -0\n",
        init_script, marker
    )
}

/// PowerShell profile-prepare script. The UTF-8 preamble makes the env dump
/// deterministic Unicode; `$ErrorActionPreference = 'Stop'` mirrors `set -e`
/// for cmdlet errors (a terminating error in the snippet aborts preparation
/// and reports a non-zero exit instead of producing a truncated snapshot); the
/// trailing `$LASTEXITCODE` truthiness check mirrors it for the last native
/// command (`$LASTEXITCODE` is `$null` after a pure-PowerShell snippet, which
/// is falsy). The marker is a host-formatted line, then each environment
/// entry is written as `NAME=VALUE\0` straight through `[Console]::Out` — no
/// human-formatted table, no line wrapping, values may contain `=`, spaces,
/// quotes, newlines, or any shell metacharacter, and entries are unambiguous
/// because the value can never contain NUL. `Get-ChildItem Env:` reflects the
/// process block after the init snippet ran.
pub(super) fn powershell_profile_prepare_script(init_script: &str, marker: &str) -> String {
    format!(
        "{POWERSHELL_UTF8_PREAMBLE}\n\
         $ErrorActionPreference = 'Stop'\n\
         try {{\n\
         {init_script}\n\
         if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}\n\
         }} catch {{\n\
         [Console]::Error.WriteLine($_)\n\
         exit 1\n\
         }}\n\
         Write-Output '{marker}'\n\
         Get-ChildItem Env: | ForEach-Object {{ \
         [Console]::Out.Write($_.Name + '=' + $_.Value + [string][char]0) }}\n\
         [Console]::Out.Flush()"
    )
}

pub(super) fn capture_profile_env_snapshot(
    profile_name: &str,
    profile: &ShellProfileConfig,
    program: &str,
    args: &[String],
    dialect: ShellDialect,
    prepare_cwd: &Path,
    initial_env: HashMap<String, String>,
    stop_requested: Option<&AtomicBool>,
) -> Result<HashMap<String, String>, String> {
    let Some(init_script) = profile.init_script.as_deref() else {
        return Ok(initial_env);
    };
    let marker = format!("__WEBCODEX_ENV_START_{}__", uuid::Uuid::new_v4().simple());
    let prepare_script = match dialect {
        ShellDialect::Posix => posix_profile_prepare_script(init_script, &marker),
        ShellDialect::PowerShell => powershell_profile_prepare_script(init_script, &marker),
    };
    let mut cmd = Command::new(program);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg(prepare_script).current_dir(prepare_cwd).env_clear();
    // `run_prepare_command` owns this process tree through ManagedChild; do
    // not add a process-group pre_exec here. ManagedChild creates the private
    // process group (Unix) / Job Object (Windows) at spawn time.
    for (key, value) in initial_env {
        cmd.env(key, value);
    }
    let (status, stdout, stderr) = run_prepare_command(
        cmd,
        Duration::from_secs(SHELL_PROFILE_PREPARE_TIMEOUT_SECS),
        stop_requested,
    )
    .map_err(|e| {
        format!(
            "failed to prepare shell profile '{}' at {}: {}",
            profile_name,
            prepare_cwd.display(),
            e
        )
    })?;
    if !status.success() {
        return Err(format!(
            "failed to prepare shell profile '{}' at {}: exit code {}; stderr tail: {}",
            profile_name,
            prepare_cwd.display(),
            status.code().unwrap_or(-1),
            stderr_tail(&stderr)
        ));
    }
    let marker_pos = find_bytes(&stdout, marker.as_bytes()).ok_or_else(|| {
        format!(
            "failed to prepare shell profile '{}' at {}: env marker not found",
            profile_name,
            prepare_cwd.display()
        )
    })?;
    let mut payload_start = marker_pos + marker.len();
    while stdout
        .get(payload_start)
        .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
    {
        payload_start += 1;
    }
    let mut snapshot = parse_env_payload(&stdout[payload_start..], profile_name)?;
    // The init snippet may have exported a sensitive variable in any case;
    // Windows filters case-insensitively.
    remove_sensitive_env(&mut snapshot);
    Ok(snapshot)
}

impl PreparedShellProfileCache {
    /// Number of currently prepared snapshots. Used only for the sanitized
    /// observability summary; never exposes snapshot contents.
    pub(crate) fn len(&self) -> usize {
        self.profiles.lock().unwrap().len()
    }

    pub(super) fn get_or_prepare(
        &self,
        generation: u64,
        shell: &ShellConfig,
        profile_name: &str,
        project_key: String,
        prepare_cwd: &Path,
        stop_requested: Option<&AtomicBool>,
    ) -> Result<Arc<PreparedShellProfile>, String> {
        let key = PreparedShellProfileKey {
            generation,
            project_key,
            profile_name: profile_name.to_string(),
        };
        let profiles = self.profiles.lock().unwrap();
        if let Some(prepared) = profiles.get(&key).cloned() {
            return Ok(prepared);
        }
        drop(profiles);
        let profile = shell.profiles.get(profile_name).ok_or_else(|| {
            format!(
                "shell profile '{}' is not configured for project/cwd {}",
                profile_name,
                prepare_cwd.display()
            )
        })?;
        let program = resolved_shell_program(
            &profile
                .program
                .clone()
                .unwrap_or_else(|| shell.program.clone()),
        );
        let args = profile.args.clone().unwrap_or_else(|| shell.args.clone());
        // The profile inherits the parent shell dialect unless it (or the
        // parent) explicitly configures one; the prepare script and every
        // later command in this profile use the same resolved dialect.
        let dialect = resolve_dialect(&program, profile.dialect.or(shell.dialect));
        let initial_env = base_shell_env(shell, profile)?;
        let env_snapshot = capture_profile_env_snapshot(
            profile_name,
            profile,
            &program,
            &args,
            dialect,
            prepare_cwd,
            initial_env,
            stop_requested,
        )?;
        let prepared = Arc::new(PreparedShellProfile {
            profile_name: profile_name.to_string(),
            program,
            args,
            dialect,
            env_snapshot,
        });
        let mut profiles = self.profiles.lock().unwrap();
        if let Some(cached) = profiles.get(&key).cloned() {
            return Ok(cached);
        }
        if profiles.keys().any(|cached| cached.generation > generation) {
            return Ok(prepared);
        }
        profiles.retain(|cached, _| cached.generation == generation);
        profiles.insert(key, prepared.clone());
        Ok(prepared)
    }
}

impl PreparedExecutionEnvironment {
    pub(crate) fn prepare(
        generation: u64,
        shell: &ShellConfig,
        explicit_profile: Option<&str>,
        prepare_cwd: &Path,
        cache: &PreparedShellProfileCache,
        stop_requested: Option<&AtomicBool>,
    ) -> Result<Self, String> {
        let profile_name = explicit_profile.or(shell.default_profile.as_deref());
        let env_snapshot = match profile_name {
            Some(profile_name) => cache
                .get_or_prepare(
                    generation,
                    shell,
                    profile_name,
                    format!(
                        "plugin:{}",
                        prepare_cwd
                            .canonicalize()
                            .unwrap_or_else(|_| prepare_cwd.to_path_buf())
                            .to_string_lossy()
                    ),
                    prepare_cwd,
                    stop_requested,
                )?
                .env_snapshot
                .clone(),
            None => base_shell_env(shell, &ShellProfileConfig::default())?,
        };
        Ok(Self { env_snapshot })
    }

    pub(crate) fn native_command(
        &self,
        program: &str,
        args: &[String],
        cwd: &Path,
    ) -> Result<Command, String> {
        let requested = {
            let path = Path::new(program);
            if !path.is_absolute() && path.components().count() > 1 {
                cwd.join(path).to_string_lossy().into_owned()
            } else {
                program.to_string()
            }
        };
        let path = env_lookup(&self.env_snapshot, "PATH")
            .map(OsString::from)
            .unwrap_or_default();
        let resolved = crate::webcodex_runner::util::resolve_program_in_path(&requested, &path)
            .ok_or_else(|| {
                format!("plugin executable is unavailable in prepared PATH: {program}")
            })?;
        #[cfg(windows)]
        let native = match resolved {
            crate::webcodex_runner::util::ResolvedProgram::Native(path) => path,
            crate::webcodex_runner::util::ResolvedProgram::Batch(_) => {
                return Err(
                    "unsupported_executable_type: native Tool Plugins cannot launch Windows .cmd/.bat files; configure a native runtime executable instead"
                        .to_string(),
                )
            }
        };
        #[cfg(not(windows))]
        let native = match resolved {
            crate::webcodex_runner::util::ResolvedProgram::Native(path) => path,
        };
        let mut command = Command::new(native);
        command.args(args);
        command.current_dir(cwd);
        apply_env_snapshot(&mut command, &self.env_snapshot);
        Ok(command)
    }
}

pub(super) fn shell_profile_project_key(project_id: Option<&str>, path: &Path) -> String {
    let path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();
    match project_id {
        Some(id) => format!("project:{}:{}", id, path),
        None => format!("cwd:{}", path),
    }
}

pub(crate) fn resolve_prepared_shell_profile(
    generation: u64,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cwd_path: &Path,
    request_has_cwd: bool,
    cache: &PreparedShellProfileCache,
    stop_requested: Option<&AtomicBool>,
) -> Result<Option<Arc<PreparedShellProfile>>, String> {
    let project = request_has_cwd
        .then(|| find_project_shell_context(project_registry_dir, cwd_path))
        .flatten();
    let profile_name = project
        .as_ref()
        .and_then(|project| project.shell_profile.as_deref())
        .or(shell.default_profile.as_deref());
    let Some(profile_name) = profile_name else {
        return Ok(None);
    };
    let prepare_cwd = project
        .as_ref()
        .map(|project| PathBuf::from(&project.path))
        .unwrap_or_else(|| cwd_path.to_path_buf());
    if let Some(project) = &project {
        if project.shell_profile.as_deref() == Some(profile_name)
            && !shell.profiles.contains_key(profile_name)
        {
            return Err(format!(
                "project '{}' shell_profile '{}' does not match any shell.profiles entry",
                project.id, profile_name
            ));
        }
    }
    let project_key = shell_profile_project_key(
        project.as_ref().map(|project| project.id.as_str()),
        &prepare_cwd,
    );
    cache
        .get_or_prepare(
            generation,
            shell,
            profile_name,
            project_key,
            &prepare_cwd,
            stop_requested,
        )
        .map(Some)
}

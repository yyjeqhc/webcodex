//! Current shell environment policy and sensitive-key filtering.

use super::*;

/// Resolve the effective shell dialect: an explicit config value wins, then a
/// known shell program basename, then the platform default. Profiles pass
/// `profile.dialect.or(shell.dialect)` as the explicit value so they inherit
/// the parent shell dialect unless designed otherwise.
pub(super) fn resolve_dialect(program: &str, explicit: Option<ShellDialect>) -> ShellDialect {
    explicit
        .or_else(|| dialect_for_program(program))
        .unwrap_or_else(platform_default_dialect)
}

pub(super) const SENSITIVE_ENV_KEYS: [&str; 5] = [
    "WEBCODEX_TOKEN",
    "WEBCODEX_PAT",
    "WEBCODEX_AGENT_TOKEN",
    "WEBCODEX_USER_TOKEN",
    "AUTHORIZATION",
];

/// Compare environment variable names with the host platform's semantics.
/// Windows names are case-insensitive; Unix names remain case-sensitive.
pub(crate) fn env_keys_equal(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

/// Sensitive environment keys must never reach child processes. Windows
/// environment names are case-insensitive, so mixed-case spellings such as
/// `WebCodex_Token` and `WebCodex_Pat` must be filtered too; Unix stays case-sensitive.
pub(crate) fn is_sensitive_env_key(key: &str) -> bool {
    SENSITIVE_ENV_KEYS
        .iter()
        .any(|sensitive| env_keys_equal(sensitive, key))
}

pub(super) fn should_inherit_env_key(key: &str) -> bool {
    // Windows command processors may add drive-current-directory pseudo
    // entries such as `=E:=E:\\git\\webcodex` to the native environment
    // block. They are not ordinary environment variables and cannot be
    // reconstructed through `Command::env`; detached execution carries its
    // working directory explicitly, so dropping them preserves the intended
    // child environment without weakening launch-envelope validation.
    !is_sensitive_env_key(key)
        && !key
            .to_ascii_uppercase()
            .starts_with(webcodex_runner_config::DESKTOP_MCP_ENV_PREFIX)
        && !(cfg!(windows) && key.starts_with('='))
}

/// Case-insensitive lookup on Windows (where environment names are
/// case-insensitive), exact match on Unix.
pub(super) fn env_lookup<'a>(env: &'a HashMap<String, String>, key: &str) -> Option<&'a String> {
    if cfg!(windows) {
        env.iter()
            .find(|(candidate, _)| env_keys_equal(candidate, key))
            .map(|(_, value)| value)
    } else {
        env.get(key)
    }
}

/// Insert `key=value`, replacing any existing entry that names the same
/// environment variable. On Windows the replacement is case-insensitive so a
/// snapshot never carries both `Path` and `PATH` (which would make the final
/// child environment depend on HashMap iteration order); on Unix it is exact.
pub(super) fn env_insert(env: &mut HashMap<String, String>, key: &str, value: String) {
    if cfg!(windows) {
        env.retain(|candidate, _| !env_keys_equal(candidate, key));
    }
    env.insert(key.to_string(), value);
}

/// Remove every sensitive environment key from `env`, case-insensitively on
/// Windows (a profile could configure `webcodetoken = ...`).
pub(super) fn remove_sensitive_env(env: &mut HashMap<String, String>) {
    let sensitive: Vec<String> = env
        .keys()
        .filter(|key| is_sensitive_env_key(key))
        .cloned()
        .collect();
    for key in sensitive {
        env.remove(&key);
    }
}

pub(super) fn apply_shell_environment(
    cmd: &mut Command,
    shell: &ShellConfig,
) -> Result<(), String> {
    if shell.environment_mode == ShellEnvironmentMode::Isolated {
        let env = base_shell_env(shell, &ShellProfileConfig::default())?;
        apply_env_snapshot(cmd, &env);
        return Ok(());
    }
    // Rust's Windows env handling is case-insensitive (like the OS itself), so
    // removing the canonical spellings also removes mixed-case variants such
    // as `WebCodex_Token`.
    for key in SENSITIVE_ENV_KEYS {
        cmd.env_remove(key);
    }
    if !shell.path_prepend.is_empty() {
        let mut paths = shell.path_prepend.clone();
        if let Some(current) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&current));
        }
        let joined = std::env::join_paths(paths)
            .map_err(|e| format!("failed to build shell PATH from shell.path_prepend: {}", e))?;
        cmd.env("PATH", joined);
    }
    for (key, value) in &shell.env {
        if !is_sensitive_env_key(key) {
            cmd.env(key, value);
        }
    }
    Ok(())
}

pub(super) fn apply_env_snapshot(cmd: &mut Command, env_snapshot: &HashMap<String, String>) {
    cmd.env_clear();
    for (key, value) in env_snapshot {
        cmd.env(key, value);
    }
}

/// On Windows, resolve a bare shell program name through the platform rules
/// so an extensionless POSIX shim shadowing the real executable is never
/// selected (CreateProcess would fail with error 193). Path-qualified values
/// are used verbatim; an unresolvable bare name falls back to the configured
/// value so the spawn surfaces the real error.
pub(super) fn resolved_shell_program(program: &str) -> String {
    #[cfg(windows)]
    {
        let path = Path::new(program);
        if path.components().count() <= 1 && !path.is_absolute() {
            if let Some(resolved) = crate::webcodex_runner::util::resolve_program_in_path(
                program,
                std::env::var_os("PATH")
                    .as_deref()
                    .unwrap_or(OsStr::new("")),
            ) {
                return resolved.path().to_string_lossy().into_owned();
            }
        }
    }
    #[cfg(not(windows))]
    let _ = program;
    program.to_string()
}

pub(crate) fn base_shell_env(
    shell: &ShellConfig,
    profile: &ShellProfileConfig,
) -> Result<HashMap<String, String>, String> {
    let mut env: HashMap<String, String> = match shell.environment_mode {
        ShellEnvironmentMode::Inherit => std::env::vars_os()
            .filter_map(|(key, value)| {
                let key = key.into_string().ok()?;
                if !should_inherit_env_key(&key) {
                    return None;
                }
                let value = value.into_string().ok()?;
                Some((key, value))
            })
            .collect(),
        ShellEnvironmentMode::Isolated => {
            let mut env = HashMap::new();
            #[cfg(not(windows))]
            env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
            #[cfg(windows)]
            if let Ok(root) = std::env::var("SystemRoot") {
                let path = Path::new(&root).join("System32");
                env.insert("PATH".to_string(), path.to_string_lossy().into_owned());
                env.insert("SystemRoot".to_string(), root);
            }
            env
        }
    };
    if !shell.path_prepend.is_empty() {
        let mut paths = shell.path_prepend.clone();
        // The inherited Windows PATH may be spelled `Path`; lookup must be
        // case-insensitive or the prepended entries would replace it instead
        // of extending it.
        if let Some(current) = env_lookup(&env, "PATH") {
            paths.extend(std::env::split_paths(current));
        }
        let joined = std::env::join_paths(paths)
            .map_err(|e| format!("failed to build shell PATH from shell.path_prepend: {}", e))?;
        env_insert(&mut env, "PATH", joined.to_string_lossy().to_string());
    }
    for (key, value) in &shell.env {
        env_insert(&mut env, key, value.clone());
    }
    for (key, value) in &profile.env {
        env_insert(&mut env, key, value.clone());
    }
    // A profile could configure a sensitive name in any case; Windows filters
    // case-insensitively.
    remove_sensitive_env(&mut env);
    Ok(env)
}

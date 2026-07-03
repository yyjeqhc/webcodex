use crate::agent_init::{
    effective_allowed_roots, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_TIMEOUT_SECS,
    DEFAULT_POLL_INTERVAL_MS, TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC,
    TRANSPORT_WEBSOCKET,
};
use crate::shell_protocol::ShellClientCapabilities;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_PATH: &str = "/etc/webcodex/agent.toml";
pub(crate) const CLIENT_PROFILE_ERROR: &str =
    "--profile must be a safe path component using only ASCII letters, digits, '.', '_' or '-'";
pub(crate) const DEFAULT_MAX_CONCURRENT_JOBS: usize = 2;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AgentConfig {
    pub(crate) server_url: String,
    pub(crate) token: String,
    pub(crate) client_id: String,
    #[serde(default)]
    pub(crate) display_name: Option<String>,
    #[serde(default)]
    pub(crate) owner: Option<String>,
    #[serde(default)]
    pub(crate) hostname: Option<String>,
    #[serde(default)]
    pub(crate) projects_dir: Option<PathBuf>,
    #[serde(default = "default_poll_interval_ms")]
    pub(crate) poll_interval_ms: u64,
    #[serde(default)]
    pub(crate) capabilities: Option<ShellClientCapabilities>,
    #[serde(default)]
    pub(crate) max_concurrent_jobs: Option<usize>,
    #[serde(default)]
    pub(crate) policy: AgentPolicy,
    /// Transport selection: `"websocket"` (default), `"polling"`, `"quic"`,
    /// or explicit `"auto"` fallback mode.
    #[serde(default)]
    pub(crate) transport: Option<String>,
    /// Experimental custom QUIC agent transport config. Used by strict
    /// `transport = "quic"` and by explicit `transport = "auto"`.
    #[serde(default)]
    pub(crate) quic: Option<QuicClientConfig>,
    #[serde(default)]
    pub(crate) shell: ShellConfig,
}

/// Agent-side QUIC transport configuration (`[quic]` in `agent.toml`). All
/// fields are required when `transport = "quic"`; `run_quic_agent` validates
/// them before connecting. The token is NOT stored here — it stays in the
/// top-level `token` field and is carried in the `Register` envelope's
/// `auth_token` field, mirroring the `Authorization: Bearer` header used by
/// the websocket/polling paths.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QuicClientConfig {
    /// `host:port` of the server's QUIC listener (e.g. `host:8443`).
    pub(crate) server_addr: String,
    /// TLS SNI / server name to verify the certificate against. Must match the
    /// cert's SAN (typically the domain name).
    pub(crate) server_name: String,
    /// ALPN protocol; must match the server's `WEBCODEX_QUIC_ALPN`.
    #[serde(default = "default_quic_alpn")]
    pub(crate) alpn: String,
    /// Connection timeout in seconds.
    #[serde(default = "default_quic_connect_timeout_secs")]
    pub(crate) connect_timeout_secs: u64,
    /// QUIC keepalive interval in seconds.
    #[serde(default = "default_quic_keepalive_interval_secs")]
    pub(crate) keepalive_interval_secs: u64,
}

pub(crate) fn default_quic_alpn() -> String {
    "webcodex-agent/1".to_string()
}
pub(crate) fn default_quic_connect_timeout_secs() -> u64 {
    10
}
pub(crate) fn default_quic_keepalive_interval_secs() -> u64 {
    20
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AgentPolicy {
    #[serde(default = "default_true")]
    pub(crate) allow_raw_shell: bool,
    #[serde(default = "default_true")]
    pub(crate) allow_cwd_anywhere: bool,
    #[serde(default)]
    pub(crate) allowed_roots: Vec<PathBuf>,
    #[serde(default = "default_max_timeout_secs")]
    pub(crate) max_timeout_secs: u64,
    #[serde(default = "default_max_output_bytes")]
    pub(crate) max_output_bytes: usize,
}

impl Default for AgentPolicy {
    fn default() -> Self {
        Self {
            allow_raw_shell: true,
            allow_cwd_anywhere: true,
            allowed_roots: Vec::new(),
            max_timeout_secs: DEFAULT_MAX_TIMEOUT_SECS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct ShellConfig {
    #[serde(default)]
    pub(crate) default_profile: Option<String>,
    #[serde(default)]
    pub(crate) profiles: BTreeMap<String, ShellProfileConfig>,
    #[serde(default = "default_shell_program")]
    pub(crate) program: String,
    #[serde(default = "default_shell_args")]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) path_prepend: Vec<PathBuf>,
    #[serde(default)]
    pub(crate) env: HashMap<String, String>,
    #[serde(default)]
    pub(crate) init_script: Option<PathBuf>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            default_profile: None,
            profiles: BTreeMap::new(),
            program: default_shell_program(),
            args: default_shell_args(),
            path_prepend: Vec::new(),
            env: HashMap::new(),
            init_script: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct ShellProfileConfig {
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) program: Option<String>,
    #[serde(default)]
    pub(crate) args: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) init_script: Option<String>,
}

fn default_shell_program() -> String {
    "sh".to_string()
}

fn default_shell_args() -> Vec<String> {
    vec!["-c".to_string()]
}

pub(crate) fn default_true() -> bool {
    true
}

fn default_poll_interval_ms() -> u64 {
    DEFAULT_POLL_INTERVAL_MS
}

fn default_max_timeout_secs() -> u64 {
    DEFAULT_MAX_TIMEOUT_SECS
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}

pub(crate) fn max_concurrent_jobs(cfg: &AgentConfig) -> usize {
    cfg.max_concurrent_jobs
        .unwrap_or(DEFAULT_MAX_CONCURRENT_JOBS)
        .max(1)
}

fn default_client_base_dir() -> PathBuf {
    if is_effective_root() {
        PathBuf::from("/etc/webcodex")
    } else {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".config/webcodex")
    }
}

#[cfg(unix)]
fn is_effective_root() -> bool {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                let mut parts = rest.split_whitespace();
                let _real = parts.next();
                if let Some(effective) = parts.next() {
                    return effective == "0";
                }
            }
        }
    }
    std::env::var("USER").is_ok_and(|u| u == "root")
}

#[cfg(not(unix))]
fn is_effective_root() -> bool {
    false
}

pub(crate) fn validate_client_profile(profile: &str) -> Result<String, String> {
    let trimmed = profile.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.len() > 80
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || !trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(CLIENT_PROFILE_ERROR.to_string());
    }
    Ok(trimmed.to_string())
}

pub(crate) fn client_profile_agent_config(profile: &str) -> PathBuf {
    default_client_base_dir()
        .join("clients")
        .join(profile)
        .join("agent.toml")
}

pub(crate) fn default_config_path() -> PathBuf {
    let home_path = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/webcodex/agent.toml"));
    let system_path = PathBuf::from(DEFAULT_CONFIG_PATH);
    for path in [home_path.clone(), Some(system_path.clone())]
        .into_iter()
        .flatten()
    {
        if path.exists() {
            return path;
        }
    }
    home_path
        .or_else(|| Some(system_path))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH))
}

fn validate_env_key(key: &str) -> bool {
    !key.is_empty()
        && !key.contains('=')
        && !key.contains('\0')
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(crate) fn validate_shell_profile_name(context: &str, name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("{} cannot be empty", context));
    }
    if name.contains("..") {
        return Err(format!("{} cannot contain '..'", context));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!("{} cannot contain slash or backslash", context));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
    {
        return Err(format!(
            "{} may only contain ASCII letters, digits, '_', '-', and '.'",
            context
        ));
    }
    Ok(())
}

fn validate_shell_profile_config(name: &str, profile: &ShellProfileConfig) -> Result<(), String> {
    if profile
        .program
        .as_ref()
        .is_some_and(|program| program.trim().is_empty())
    {
        return Err(format!("shell.profiles.{}.program cannot be empty", name));
    }
    if let Some(args) = &profile.args {
        if args.is_empty() {
            return Err(format!(
                "shell.profiles.{}.args must include the command flag, for example [\"-c\"]",
                name
            ));
        }
        if args.iter().any(|arg| arg.trim().is_empty()) {
            return Err(format!(
                "shell.profiles.{}.args cannot contain empty values",
                name
            ));
        }
    }
    for key in profile.env.keys() {
        if !validate_env_key(key) {
            return Err(format!(
                "shell.profiles.{}.env contains invalid key '{}'",
                name, key
            ));
        }
    }
    if profile
        .init_script
        .as_ref()
        .is_some_and(|script| script.trim().is_empty())
    {
        return Err(format!(
            "shell.profiles.{}.init_script cannot be empty",
            name
        ));
    }
    Ok(())
}

pub(crate) fn validate_shell_config(shell: &ShellConfig) -> Result<(), String> {
    if let Some(default_profile) = &shell.default_profile {
        validate_shell_profile_name("shell.default_profile", default_profile)?;
        if !shell.profiles.contains_key(default_profile) {
            return Err(format!(
                "shell.default_profile '{}' does not match any shell.profiles entry",
                default_profile
            ));
        }
    }
    for (name, profile) in &shell.profiles {
        validate_shell_profile_name("shell profile name", name)?;
        validate_shell_profile_config(name, profile)?;
    }
    if shell.program.trim().is_empty() {
        return Err("shell.program cannot be empty".to_string());
    }
    if shell.args.is_empty() {
        return Err("shell.args must include the command flag, for example [\"-c\"]".to_string());
    }
    if shell.args.iter().any(|arg| arg.trim().is_empty()) {
        return Err("shell.args cannot contain empty values".to_string());
    }
    if shell
        .path_prepend
        .iter()
        .any(|path| path.as_os_str().is_empty())
    {
        return Err("shell.path_prepend cannot contain empty paths".to_string());
    }
    for key in shell.env.keys() {
        if !validate_env_key(key) {
            return Err(format!("shell.env contains invalid key '{}'", key));
        }
    }
    if shell
        .init_script
        .as_ref()
        .is_some_and(|path| path.as_os_str().is_empty())
    {
        return Err("shell.init_script cannot be empty".to_string());
    }
    Ok(())
}

fn validate_optional_toml_string(
    table: &toml::map::Map<String, toml::Value>,
    field: &str,
    path: &str,
) -> Result<(), String> {
    if table
        .get(field)
        .is_some_and(|value| !matches!(value, toml::Value::String(_)))
    {
        return Err(format!("{} must be a string", path));
    }
    Ok(())
}

fn validate_shell_profile_toml_shape(content: &str) -> Result<(), String> {
    let value: toml::Value = toml::from_str(content)
        .map_err(|e| format!("failed to parse config TOML syntax: {}", e))?;
    let Some(shell) = value.get("shell") else {
        return Ok(());
    };
    let Some(shell) = shell.as_table() else {
        return Err("shell must be a table".to_string());
    };
    validate_optional_toml_string(shell, "default_profile", "shell.default_profile")?;
    let Some(profiles) = shell.get("profiles") else {
        return Ok(());
    };
    let Some(profiles) = profiles.as_table() else {
        return Err("shell.profiles must be a table".to_string());
    };
    for (name, profile) in profiles {
        let Some(profile) = profile.as_table() else {
            return Err(format!("shell.profiles.{} must be a table", name));
        };
        validate_optional_toml_string(
            profile,
            "description",
            &format!("shell.profiles.{}.description", name),
        )?;
        validate_optional_toml_string(
            profile,
            "program",
            &format!("shell.profiles.{}.program", name),
        )?;
        validate_optional_toml_string(
            profile,
            "init_script",
            &format!("shell.profiles.{}.init_script", name),
        )?;
        if let Some(args) = profile.get("args") {
            let Some(args) = args.as_array() else {
                return Err(format!(
                    "shell.profiles.{}.args must be a string array",
                    name
                ));
            };
            if args
                .iter()
                .any(|arg| !matches!(arg, toml::Value::String(_)))
            {
                return Err(format!(
                    "shell.profiles.{}.args must be a string array",
                    name
                ));
            }
        }
        if let Some(env) = profile.get("env") {
            let Some(env) = env.as_table() else {
                return Err(format!("shell.profiles.{}.env must be a string map", name));
            };
            if env
                .values()
                .any(|value| !matches!(value, toml::Value::String(_)))
            {
                return Err(format!("shell.profiles.{}.env must be a string map", name));
            }
        }
    }
    Ok(())
}

pub(crate) fn load_config(path: &Path) -> Result<AgentConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read config {}: {}", path.display(), e))?;
    validate_shell_profile_toml_shape(&content)
        .map_err(|e| format!("failed to parse config {}: {}", path.display(), e))?;
    let mut cfg: AgentConfig = toml::from_str(&content)
        .map_err(|e| format!("failed to parse config {}: {}", path.display(), e))?;
    if cfg.server_url.trim().is_empty() {
        return Err("server_url cannot be empty".to_string());
    }
    if cfg.client_id.trim().is_empty() {
        return Err("client_id cannot be empty".to_string());
    }
    if cfg.poll_interval_ms == 0 {
        return Err("poll_interval_ms must be > 0".to_string());
    }
    if let Some(transport) = cfg.transport.as_deref().map(str::trim) {
        if !transport.is_empty()
            && !matches!(
                transport,
                TRANSPORT_WEBSOCKET | TRANSPORT_POLLING | TRANSPORT_QUIC | TRANSPORT_AUTO
            )
        {
            return Err("transport must be websocket, polling, quic, or auto".to_string());
        }
    }
    // When allowed_roots is missing/empty, default to [$HOME] so a
    // minimal agent.toml without an explicit policy.allowed_roots still works
    // predictably. If HOME is unavailable and allow_cwd_anywhere is false,
    // surface a clear configuration error. Explicit allowed_roots is preserved
    // as-is and overrides the HOME default.
    let effective =
        effective_allowed_roots(&cfg.policy.allowed_roots, cfg.policy.allow_cwd_anywhere)?;
    cfg.policy.allowed_roots = effective;
    validate_shell_config(&cfg.shell)?;
    Ok(cfg)
}

pub(crate) fn hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

fn default_projects_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/webcodex/projects.d")
}

pub(crate) fn projects_dir(cfg: &AgentConfig) -> PathBuf {
    cfg.projects_dir
        .clone()
        .unwrap_or_else(default_projects_dir)
}

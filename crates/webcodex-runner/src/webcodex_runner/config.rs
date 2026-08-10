use super::external_tools::ExternalToolRouter;
use super::shutdown::lock_unpoison;
use crate::agent_init::{
    effective_allowed_roots, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_TIMEOUT_SECS,
    DEFAULT_POLL_INTERVAL_MS, TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC,
    TRANSPORT_WEBSOCKET,
};
use crate::shell_protocol::{
    AgentConfigReloadStatus, ShellClientCapabilities, JOB_INVENTORY_MAX_ACTIVE_JOBS,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};

const DEFAULT_CONFIG_PATH: &str = "/etc/webcodex/agent.toml";
pub(crate) const CLIENT_PROFILE_ERROR: &str =
    "--profile must be a safe path component using only ASCII letters, digits, '.', '_' or '-'";
pub(crate) const DEFAULT_MAX_CONCURRENT_JOBS: usize = 4;
pub(crate) const DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_SECS: u64 = 5;

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
    /// Optional Runner-owned root for managed temporary projects. When absent,
    /// temporary project creation is disabled; ordinary project registration is
    /// unchanged.
    #[serde(default)]
    pub(crate) temporary_projects_root: Option<PathBuf>,
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
    /// WebSocket connect timeout in seconds. This bounds foreground auto
    /// fallback latency when WebSocket is blocked or unreachable.
    #[serde(default = "default_websocket_connect_timeout_secs")]
    pub(crate) websocket_connect_timeout_secs: u64,
    /// Experimental custom QUIC agent transport config. Used by strict
    /// `transport = "quic"` and by explicit `transport = "auto"`.
    #[serde(default)]
    pub(crate) quic: Option<QuicClientConfig>,
    #[serde(default)]
    pub(crate) shell: ShellConfig,
    /// Named remote SSH execution resources. Credentials remain entirely in
    /// the Runner host's OpenSSH configuration, keys, or ssh-agent.
    #[serde(default)]
    pub(crate) ssh: SshConfig,
    #[serde(default)]
    pub(crate) tool_providers: ToolProvidersConfig,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolProviderStrategy {
    #[default]
    Native,
    ClaudeCode,
    ClaudeCodeThenNative,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct ToolProvidersConfig {
    #[serde(default)]
    pub(crate) strategy: ToolProviderStrategy,
    #[serde(default)]
    pub(crate) claude_code: ClaudeCodeMcpConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct ClaudeCodeMcpConfig {
    pub(crate) enabled: bool,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) mapping: HashMap<String, String>,
    pub(crate) timeout_secs: u64,
}

impl Default for ClaudeCodeMcpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "claude".to_string(),
            args: vec!["mcp".to_string(), "serve".to_string()],
            mapping: HashMap::new(),
            timeout_secs: 30,
        }
    }
}

/// Agent-side QUIC transport configuration (`[quic]` in `agent.toml`). All
/// fields are required when `transport = "quic"`; `run_quic_agent` validates
/// them before connecting. The token is NOT stored here — it stays in the
/// top-level `token` field and is carried in the `Register` envelope's
/// `auth_token` field, mirroring the `Authorization: Bearer` header used by
/// the websocket/polling paths.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
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
    crate::shell_protocol::AGENT_QUIC_ALPN_V1.to_string()
}
pub(crate) fn default_quic_connect_timeout_secs() -> u64 {
    10
}
pub(crate) fn default_quic_keepalive_interval_secs() -> u64 {
    20
}
pub(crate) fn default_websocket_connect_timeout_secs() -> u64 {
    DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_SECS
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct AgentPolicy {
    #[serde(default = "default_true")]
    pub(crate) allow_raw_shell: bool,
    /// Fail closed: an `agent.toml` that omits `[policy]` must not disable the
    /// filesystem boundary. When false, `allowed_roots` (defaulted to `$HOME`
    /// by `effective_allowed_roots`) is the outer bound for every file op.
    #[serde(default)]
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
            allow_cwd_anywhere: false,
            allowed_roots: Vec::new(),
            max_timeout_secs: DEFAULT_MAX_TIMEOUT_SECS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

/// Shell grammar dialect used for quoting, init-script sourcing, profile
/// preparation, and environment snapshot serialization. The Runner never
/// guesses the grammar of an arbitrary custom executable: an explicit
/// `shell.dialect` / `shell.profiles.<name>.dialect` value wins, otherwise a
/// known shell basename is mapped, otherwise the platform default applies.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub(crate) enum ShellDialect {
    /// `sh`-compatible syntax: `. <path> && (...)`, single-quote escaping,
    /// `set -e`, `printf`, `env -0`.
    #[serde(rename = "posix")]
    Posix,
    /// Windows PowerShell syntax: dot-sourcing, `''` single-quote escaping,
    /// `[Console]::Out` env serialization.
    #[serde(rename = "powershell")]
    PowerShell,
}

/// Known-shell basename mapping used when no explicit dialect is configured.
/// Never guesses for arbitrary custom executables; unknown names return `None`
/// and callers fall back to the platform default shell dialect.
pub(crate) fn dialect_for_program(program: &str) -> Option<ShellDialect> {
    match Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
    {
        "sh" | "bash" => Some(ShellDialect::Posix),
        "powershell" | "powershell.exe" | "pwsh" => Some(ShellDialect::PowerShell),
        _ => None,
    }
}

pub(crate) fn platform_default_dialect() -> ShellDialect {
    if cfg!(windows) {
        ShellDialect::PowerShell
    } else {
        ShellDialect::Posix
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
    /// Explicit shell dialect (`"posix"` or `"powershell"`). When omitted the
    /// dialect is resolved from the program basename for known shells
    /// (sh/bash -> posix, powershell/pwsh -> powershell) and otherwise defaults
    /// to the platform shell (posix on Unix, powershell on Windows).
    #[serde(default)]
    pub(crate) dialect: Option<ShellDialect>,
    #[serde(default)]
    pub(crate) path_prepend: Vec<PathBuf>,
    #[serde(default)]
    pub(crate) env: HashMap<String, String>,
    #[serde(default)]
    pub(crate) init_script: Option<PathBuf>,
    /// Maximum number of live command-oriented persistent shells owned by
    /// this Runner process.
    #[serde(default = "default_max_persistent_shells")]
    pub(crate) max_persistent_shells: usize,
    /// Idle shells are reclaimed after this many seconds. Commands in flight
    /// are never interrupted by the idle collector.
    #[serde(default = "default_persistent_shell_idle_timeout_secs")]
    pub(crate) persistent_shell_idle_timeout_secs: u64,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            default_profile: None,
            profiles: BTreeMap::new(),
            program: default_shell_program(),
            args: default_shell_args(),
            dialect: None,
            path_prepend: Vec::new(),
            env: HashMap::new(),
            init_script: None,
            max_persistent_shells: default_max_persistent_shells(),
            persistent_shell_idle_timeout_secs: default_persistent_shell_idle_timeout_secs(),
        }
    }
}

/// Runner-local named SSH resources (`[ssh.resources.<name>]`).
///
/// Only a Host/Host-alias and an optional default remote cwd are retained
/// here. Authentication material is intentionally not modeled or serialized
/// through the WebCodex protocol.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct SshConfig {
    #[serde(default)]
    pub(crate) resources: BTreeMap<String, SshResourceConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct SshResourceConfig {
    /// Passed to the Runner host's `ssh` command, so an OpenSSH `Host` alias
    /// in the normal user/system config works without copying its details.
    pub(crate) host: String,
    #[serde(default)]
    pub(crate) default_cwd: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct ShellProfileConfig {
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) program: Option<String>,
    #[serde(default)]
    pub(crate) args: Option<Vec<String>>,
    /// Explicit dialect override for this profile. When omitted the profile
    /// inherits `shell.dialect`, then the known-basename mapping, then the
    /// platform default.
    #[serde(default)]
    pub(crate) dialect: Option<ShellDialect>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) init_script: Option<String>,
}

pub(crate) struct HotAgentConfig {
    pub(crate) generation: u64,
    pub(crate) policy: AgentPolicy,
    pub(crate) shell: ShellConfig,
    pub(crate) ssh: SshConfig,
    pub(crate) external_tools: Arc<ExternalToolRouter>,
    reload_status: Mutex<AgentConfigReloadStatus>,
}

impl HotAgentConfig {
    fn new(generation: u64, cfg: &AgentConfig, status: AgentConfigReloadStatus) -> Self {
        Self {
            generation,
            policy: cfg.policy.clone(),
            shell: cfg.shell.clone(),
            ssh: cfg.ssh.clone(),
            external_tools: Arc::new(ExternalToolRouter::new(&cfg.tool_providers)),
            reload_status: Mutex::new(status),
        }
    }

    pub(crate) fn reload_status(&self) -> AgentConfigReloadStatus {
        self.reload_status.lock().unwrap().clone()
    }
}

pub(crate) struct ReloadableAgentConfig {
    startup: AgentConfig,
    path: PathBuf,
    current: RwLock<Arc<HotAgentConfig>>,
    external_routers: Mutex<Vec<Weak<ExternalToolRouter>>>,
    stopping: AtomicBool,
}

impl ReloadableAgentConfig {
    pub(crate) fn new(startup: AgentConfig, path: PathBuf) -> Self {
        let mut status = AgentConfigReloadStatus::default();
        if !cfg!(unix) {
            status.last_reload_result = "unsupported".to_string();
            status.last_reload_error_code = Some("reload_unsupported".to_string());
        }
        let current = Arc::new(HotAgentConfig::new(1, &startup, status));
        let external_routers = vec![Arc::downgrade(&current.external_tools)];
        Self {
            startup,
            path,
            current: RwLock::new(current),
            external_routers: Mutex::new(external_routers),
            stopping: AtomicBool::new(false),
        }
    }

    pub(crate) fn snapshot(&self) -> Arc<HotAgentConfig> {
        Arc::clone(&self.current.read().unwrap())
    }

    pub(crate) fn with_active(&self, f: impl FnOnce(&HotAgentConfig)) {
        f(&self.current.read().unwrap());
    }

    pub(crate) fn begin_shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
    }

    pub(crate) fn shutdown_flag(&self) -> &AtomicBool {
        &self.stopping
    }

    /// Startup-owned managed temporary-project root. Like `projects_dir`, a
    /// changed value is reported as restart-required so one running Runner
    /// cannot silently switch its project-registration boundary.
    pub(crate) fn temporary_projects_root(&self) -> Option<&Path> {
        self.startup.temporary_projects_root.as_deref()
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    pub(crate) fn external_routers(&self) -> Vec<Arc<ExternalToolRouter>> {
        let mut routers = lock_unpoison(&self.external_routers);
        let live = routers.iter().filter_map(Weak::upgrade).collect::<Vec<_>>();
        routers.retain(|router| router.strong_count() > 0);
        live
    }

    pub(crate) fn reload(&self) -> AgentConfigReloadStatus {
        if self.is_stopping() {
            return self.snapshot().reload_status();
        }
        let candidate = match load_config(&self.path) {
            Ok(candidate) => candidate,
            Err(error) => {
                let code = reload_error_code(&error);
                let active = self.snapshot();
                let status = {
                    let mut status = active.reload_status.lock().unwrap();
                    status.last_reload_result = "failure".to_string();
                    status.last_reload_error_code = Some(code.to_string());
                    status.clone()
                };
                active.external_tools.configuration_status_changed();
                eprintln!("webcodex-runner config reload failed: {code}");
                return status;
            }
        };
        let active = self.snapshot();
        let generation = active.generation.saturating_add(1);
        let restart_required_fields = restart_required_fields(&self.startup, &candidate);
        let status = AgentConfigReloadStatus {
            generation,
            last_reload_result: if restart_required_fields.is_empty() {
                "success"
            } else {
                "partial"
            }
            .to_string(),
            last_reload_error_code: None,
            restart_required: !restart_required_fields.is_empty(),
            restart_required_fields,
        };
        let next = Arc::new(HotAgentConfig::new(generation, &candidate, status.clone()));
        {
            let mut routers = lock_unpoison(&self.external_routers);
            routers.retain(|router| router.strong_count() > 0);
            routers.push(Arc::downgrade(&next.external_tools));
        }
        let mut current = self.current.write().unwrap();
        if self.is_stopping() {
            return current.reload_status();
        }
        *current = next;
        eprintln!(
            "webcodex-runner config reload {}",
            status.last_reload_result
        );
        status
    }
}

fn reload_error_code(error: &str) -> &'static str {
    if error.starts_with("failed to read config") {
        "config_read_failed"
    } else if error.starts_with("failed to parse config") {
        "config_parse_failed"
    } else if error.starts_with("tool_providers.") {
        "provider_config_invalid"
    } else {
        "config_validation_failed"
    }
}

pub(crate) fn restart_required_fields(
    startup: &AgentConfig,
    candidate: &AgentConfig,
) -> Vec<String> {
    macro_rules! classify {
        ($($field:ident),+ $(,)?) => {{
            let AgentConfig {
                policy: _, shell: _, ssh: _, tool_providers: _, $($field: _),+
            } = candidate;
            [$((stringify!($field), startup.$field != candidate.$field)),+]
                .into_iter()
                .filter_map(|(name, changed)| changed.then(|| name.to_string()))
                .collect()
        }};
    }
    classify!(
        capabilities,
        client_id,
        display_name,
        hostname,
        max_concurrent_jobs,
        owner,
        poll_interval_ms,
        projects_dir,
        temporary_projects_root,
        quic,
        server_url,
        token,
        transport,
        websocket_connect_timeout_secs,
    )
}

/// Windows default shell: native PowerShell (no sh/Git Bash/WSL required).
/// `-NoProfile` skips the user's interactive profile, `-NonInteractive` never
/// prompts, `-ExecutionPolicy Bypass` is process-scoped and lets configured
/// init/profile scripts dot-source `.ps1` files even under the stock
/// Restricted machine policy, and `-Command` accepts the full script text as a
/// single argument. stdout/stderr are captured through the Runner pipes and the
/// script text appends an explicit `exit $LASTEXITCODE`.
#[cfg(windows)]
fn default_shell_program() -> String {
    "powershell.exe".to_string()
}

#[cfg(windows)]
fn default_shell_args() -> Vec<String> {
    vec![
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-Command".to_string(),
    ]
}

#[cfg(not(windows))]
fn default_shell_program() -> String {
    "sh".to_string()
}

#[cfg(not(windows))]
fn default_shell_args() -> Vec<String> {
    vec!["-c".to_string()]
}

pub(crate) fn default_max_persistent_shells() -> usize {
    8
}

pub(crate) fn default_persistent_shell_idle_timeout_secs() -> u64 {
    30 * 60
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
        .clamp(1, JOB_INVENTORY_MAX_ACTIVE_JOBS)
}

fn default_client_base_dir() -> Result<PathBuf, String> {
    webcodex_agent_config::paths::default_client_config_base_dir()
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

pub(crate) fn client_profile_agent_config(profile: &str) -> Result<PathBuf, String> {
    Ok(default_client_base_dir()?
        .join("clients")
        .join(profile)
        .join("agent.toml"))
}

pub(crate) fn default_config_path() -> Result<PathBuf, String> {
    let user_path = default_client_base_dir()?.join("agent.toml");
    let system_path = PathBuf::from(DEFAULT_CONFIG_PATH);
    for path in [user_path.clone(), system_path.clone()] {
        if path.exists() {
            return Ok(path);
        }
    }
    Ok(user_path)
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

fn validate_ssh_resource_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 80 {
        return Err("ssh resource name must contain 1..=80 characters".to_string());
    }
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(format!(
            "ssh.resources.{} is not a safe resource name",
            name
        ));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
    {
        return Err(format!(
            "ssh resource name '{}' may only contain ASCII letters, digits, '_', '-', and '.'",
            name
        ));
    }
    Ok(())
}

fn validate_ssh_config(ssh: &mut SshConfig) -> Result<(), String> {
    for (name, resource) in &mut ssh.resources {
        validate_ssh_resource_name(name)?;
        resource.host = resource.host.trim().to_string();
        if resource.host.is_empty()
            || resource.host.starts_with('-')
            || resource.host.len() > 512
            || resource.host.chars().any(char::is_control)
        {
            return Err(format!(
                "ssh.resources.{}.host must be a non-empty safe host name",
                name
            ));
        }
        if let Some(default_cwd) = resource.default_cwd.as_mut() {
            *default_cwd = default_cwd.trim().to_string();
            if default_cwd.is_empty()
                || default_cwd.len() > 4096
                || default_cwd.chars().any(char::is_control)
            {
                return Err(format!(
                    "ssh.resources.{}.default_cwd must be a non-empty remote path without control characters",
                    name
                ));
            }
        }
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
    if !(1..=64).contains(&shell.max_persistent_shells) {
        return Err("shell.max_persistent_shells must be between 1 and 64".to_string());
    }
    if !(1..=86_400).contains(&shell.persistent_shell_idle_timeout_secs) {
        return Err(
            "shell.persistent_shell_idle_timeout_secs must be between 1 and 86400".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn validate_quic_config(quic: &QuicClientConfig) -> Result<(), String> {
    if quic.server_addr.trim().is_empty() {
        return Err("[quic] server_addr is required for transport=quic".to_string());
    }
    if quic.server_name.trim().is_empty() {
        return Err("[quic] server_name is required for transport=quic".to_string());
    }
    if quic.alpn.trim().is_empty() {
        return Err("[quic] alpn cannot be empty".to_string());
    }
    if quic.connect_timeout_secs == 0 {
        return Err("[quic] connect_timeout_secs must be > 0".to_string());
    }
    if quic.keepalive_interval_secs == 0 {
        return Err("[quic] keepalive_interval_secs must be > 0".to_string());
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
    validate_optional_toml_string(shell, "dialect", "shell.dialect")?;
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
            "dialect",
            &format!("shell.profiles.{}.dialect", name),
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
    if cfg.websocket_connect_timeout_secs == 0 {
        return Err("websocket_connect_timeout_secs must be > 0".to_string());
    }
    if let Some(root) = cfg.temporary_projects_root.as_ref() {
        if root.as_os_str().is_empty() || !root.is_absolute() {
            return Err("temporary_projects_root must be a non-empty absolute path".to_string());
        }
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
    // Materialize the default projects dir at load time so request-time code
    // never re-derives it and can never fall back to a relative path. A
    // minimal agent.toml without an explicit projects_dir gets the shared
    // per-user config base (`$HOME/.config/webcodex/projects.d` on Unix,
    // `%APPDATA%\webcodex\projects.d` on Windows).
    if cfg.projects_dir.is_none() {
        cfg.projects_dir = Some(default_projects_dir()?);
    }
    validate_shell_config(&cfg.shell)?;
    validate_ssh_config(&mut cfg.ssh)?;
    if let Some(quic) = &cfg.quic {
        validate_quic_config(quic)?;
    } else if cfg.transport.as_deref().map(str::trim) == Some(TRANSPORT_QUIC) {
        return Err("transport=quic requires a [quic] section in agent.toml".to_string());
    }
    if cfg.tool_providers.claude_code.enabled {
        if cfg.tool_providers.claude_code.command.trim().is_empty() {
            return Err("tool_providers.claude_code.command cannot be empty".to_string());
        }
        if cfg.tool_providers.claude_code.timeout_secs == 0 {
            return Err("tool_providers.claude_code.timeout_secs must be > 0".to_string());
        }
    }
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

fn default_projects_dir() -> Result<PathBuf, String> {
    Ok(default_client_base_dir()?.join("projects.d"))
}

pub(crate) fn projects_dir(cfg: &AgentConfig) -> Result<PathBuf, String> {
    match &cfg.projects_dir {
        Some(projects_dir) => Ok(projects_dir.clone()),
        None => default_projects_dir(),
    }
}

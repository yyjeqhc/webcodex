use super::coding_agent::CodingAgentManager;
use super::external_tools::ExternalToolRouter;
use super::mcp_gateway::McpGatewayManager;
use super::shutdown::lock_unpoison;
use crate::runner_config::{
    effective_allowed_roots, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_TIMEOUT_SECS,
    DEFAULT_POLL_INTERVAL_MS, TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC,
    TRANSPORT_WEBSOCKET,
};
use crate::shell_protocol::{
    AgentConfigReloadStatus, AgentHostContext, ShellClientCapabilities,
    JOB_INVENTORY_MAX_ACTIVE_JOBS,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};

const DEFAULT_SYSTEM_CONFIG_DIR: &str = "/etc/webcodex";
pub(crate) const CLIENT_PROFILE_ERROR: &str =
    "--profile must be a safe path component using only ASCII letters, digits, '.', '_' or '-'";
pub(crate) const DEFAULT_MAX_CONCURRENT_JOBS: usize = 4;
pub(crate) const DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RunnerConfig {
    pub(crate) server_url: String,
    pub(crate) token: String,
    pub(crate) client_id: String,
    #[serde(default)]
    pub(crate) display_name: Option<String>,
    #[serde(default)]
    pub(crate) owner: Option<String>,
    #[serde(default)]
    pub(crate) hostname: Option<String>,
    /// Stable, bounded planning context for this host. This is registration
    /// metadata only and never changes Runner authority or capability.
    #[serde(default)]
    pub(crate) host_context: Option<AgentHostContext>,
    #[serde(default)]
    pub(crate) project_registry_dir: Option<PathBuf>,
    /// Legacy config spelling retained only for load-time compatibility. A
    /// loaded config is normalized into `project_registry_dir` and clears this
    /// field so runtime comparisons operate on one effective registry path.
    #[serde(default, rename = "projects_dir")]
    pub(crate) legacy_projects_dir: Option<PathBuf>,
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
    pub(crate) policy: RunnerPolicy,
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
    /// Static Runner-owned stdio MCP providers exposed through WebCodex's
    /// built-in MCP gateway. The public config section is `[mcp]`.
    #[serde(default, rename = "mcp")]
    pub(crate) mcp_gateway: McpGatewayConfig,
    /// Startup/restart-owned ACP coding-agent providers. This is independent
    /// from MCP tool providers and never accepts caller-controlled executable/env.
    #[serde(default)]
    pub(crate) acp: AcpConfig,
}

const ACP_MAX_ENV_MAPPINGS: usize = 64;
const ACP_MAX_ENV_NAME_BYTES: usize = 256;
const ACP_MAX_ARGS: usize = 64;
const ACP_MAX_ARG_BYTES: usize = 4096;
const ACP_MAX_ARGS_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct AcpConfig {
    #[serde(default = "default_acp_max_concurrent_runs")]
    pub(crate) max_concurrent_runs: usize,
    #[serde(default = "default_acp_permission_timeout_secs")]
    pub(crate) permission_timeout_secs: u64,
    #[serde(default)]
    pub(crate) agents: Vec<AcpAgentConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct AcpAgentConfig {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) executable: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    /// Explicit provider-env-key -> Runner-process-env-key mapping. The child
    /// environment is cleared before these mappings are injected.
    #[serde(default)]
    pub(crate) env_from_env: BTreeMap<String, String>,
    /// Remote callers may override only live ACP config options whose ids are
    /// explicitly named here. The live advertised option still validates value.
    #[serde(default)]
    pub(crate) allowed_config_options: Vec<String>,
}

fn default_acp_max_concurrent_runs() -> usize {
    1
}
fn default_acp_permission_timeout_secs() -> u64 {
    5
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            max_concurrent_runs: default_acp_max_concurrent_runs(),
            permission_timeout_secs: default_acp_permission_timeout_secs(),
            agents: Vec::new(),
        }
    }
}

const MCP_GATEWAY_MAX_ENV_MAPPINGS: usize = 64;
const MCP_GATEWAY_MAX_ENV_NAME_BYTES: usize = 256;
pub(crate) const MCP_GATEWAY_MAX_CWD_BYTES: usize = 4_096;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct McpGatewayConfig {
    #[serde(default = "default_mcp_gateway_request_timeout_secs")]
    pub(crate) request_timeout_secs: u64,
    #[serde(default)]
    pub(crate) providers: Vec<McpGatewayProviderConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct McpGatewayProviderConfig {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) executable: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    /// Optional host-local working directory used exactly as `Command::current_dir`.
    #[serde(default)]
    pub(crate) cwd: Option<String>,
    /// Explicit provider-env-key -> Runner-process-env-key mapping. Values are
    /// resolved only immediately before first spawn and are never advertised.
    #[serde(default)]
    pub(crate) env_from_env: BTreeMap<String, String>,
    /// Optional per-provider request deadline. When absent, inherit
    /// `mcp.request_timeout_secs`.
    #[serde(default)]
    pub(crate) timeout_secs: Option<u64>,
}

fn default_mcp_gateway_request_timeout_secs() -> u64 {
    30
}

impl Default for McpGatewayConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: default_mcp_gateway_request_timeout_secs(),
            providers: Vec::new(),
        }
    }
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

/// Runner-side QUIC transport configuration (`[quic]` in the Runner config). All
/// fields are required when `transport = "quic"`; `run_quic_runner` validates
/// them before connecting. The token is NOT stored here — it stays in the
/// top-level `RunnerConfig.token`. QUIC encodes that credential only in its v1
/// transport-specific first-register frame; it never enters `AgentEnvelope`.
/// WebSocket and polling continue to use `Authorization: Bearer`.
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

/// Quinn's Server/Client default idle timeout is 30 seconds. Keep the
/// operator-configured transport keepalive below it with explicit scheduling
/// slack so current and rolling-upgrade peers do not time out first.
pub(crate) const MAX_QUIC_KEEPALIVE_INTERVAL_SECS: u64 = 25;

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
pub(crate) struct RunnerPolicy {
    #[serde(default = "default_true")]
    pub(crate) allow_raw_shell: bool,
    /// Fail closed: a Runner config that omits `[policy]` must not disable the
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

impl Default for RunnerPolicy {
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
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => Some(ShellDialect::PowerShell),
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

pub(crate) struct HotRunnerConfig {
    pub(crate) generation: u64,
    pub(crate) policy: RunnerPolicy,
    pub(crate) shell: ShellConfig,
    pub(crate) ssh: SshConfig,
    pub(crate) external_tools: Arc<ExternalToolRouter>,
    reload_status: Mutex<AgentConfigReloadStatus>,
}

impl HotRunnerConfig {
    fn new(generation: u64, cfg: &RunnerConfig, status: AgentConfigReloadStatus) -> Self {
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

pub(crate) struct ReloadableRunnerConfig {
    startup: RunnerConfig,
    mcp_gateway: Arc<McpGatewayManager>,
    coding_agents: Option<Arc<CodingAgentManager>>,
    /// Config file path used by `reload()`. Config reload is a Unix feature
    /// (Windows marks reload as unsupported and never stores the path), but
    /// the reload logic is exercised by cross-platform tests.
    #[cfg(any(unix, test))]
    path: PathBuf,
    current: RwLock<Arc<HotRunnerConfig>>,
    external_routers: Mutex<Vec<Weak<ExternalToolRouter>>>,
    stopping: AtomicBool,
}

impl ReloadableRunnerConfig {
    pub(crate) fn new(startup: RunnerConfig, path: PathBuf) -> Self {
        let mut status = AgentConfigReloadStatus::default();
        if !cfg!(unix) {
            status.last_reload_result = "unsupported".to_string();
            status.last_reload_error_code = Some("reload_unsupported".to_string());
        }
        #[cfg(not(any(unix, test)))]
        let _ = &path;
        let current = Arc::new(HotRunnerConfig::new(1, &startup, status));
        let external_routers = vec![Arc::downgrade(&current.external_tools)];
        let coding_agents = if startup.acp.agents.is_empty() {
            None
        } else {
            match CodingAgentManager::new(&startup.acp, &startup.client_id, &startup.server_url) {
                Ok(manager) => Some(manager),
                Err(error) => {
                    tracing::error!(error = %error, "ACP coding-agent manager unavailable; ACP execution disabled fail closed");
                    None
                }
            }
        };
        Self {
            mcp_gateway: Arc::new(McpGatewayManager::new(&startup.mcp_gateway)),
            coding_agents,
            startup,
            #[cfg(any(unix, test))]
            path,
            current: RwLock::new(current),
            external_routers: Mutex::new(external_routers),
            stopping: AtomicBool::new(false),
        }
    }

    pub(crate) fn snapshot(&self) -> Arc<HotRunnerConfig> {
        Arc::clone(&self.current.read().unwrap())
    }

    pub(crate) fn with_active(&self, f: impl FnOnce(&HotRunnerConfig)) {
        f(&self.current.read().unwrap());
    }

    pub(crate) fn begin_shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.mcp_gateway.shutdown();
        if let Some(manager) = &self.coding_agents {
            manager.stop_accepting();
        }
    }

    pub(crate) fn shutdown_flag(&self) -> &AtomicBool {
        &self.stopping
    }

    pub(crate) fn mcp_gateway(&self) -> &McpGatewayManager {
        &self.mcp_gateway
    }

    pub(crate) fn coding_agents(&self) -> Option<&Arc<CodingAgentManager>> {
        self.coding_agents.as_ref()
    }

    pub(crate) fn client_id(&self) -> &str {
        &self.startup.client_id
    }

    pub(crate) fn server_url(&self) -> &str {
        &self.startup.server_url
    }

    /// Startup-owned managed temporary-project root. Like `project_registry_dir`, a
    /// changed value is reported as restart-required so one running Runner
    /// cannot silently switch its project-registration boundary.
    pub(crate) fn temporary_projects_root(&self) -> Option<&Path> {
        self.startup.temporary_projects_root.as_deref()
    }

    #[cfg(any(unix, test))]
    pub(crate) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    pub(crate) fn external_routers(&self) -> Vec<Arc<ExternalToolRouter>> {
        let mut routers = lock_unpoison(&self.external_routers);
        let live = routers.iter().filter_map(Weak::upgrade).collect::<Vec<_>>();
        routers.retain(|router| router.strong_count() > 0);
        live
    }

    #[cfg(any(unix, test))]
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
        let next = Arc::new(HotRunnerConfig::new(generation, &candidate, status.clone()));
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

#[cfg(any(unix, test))]
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

#[cfg(any(unix, test))]
pub(crate) fn restart_required_fields(
    startup: &RunnerConfig,
    candidate: &RunnerConfig,
) -> Vec<String> {
    macro_rules! classify {
        ($($field:ident),+ $(,)?) => {{
            let RunnerConfig {
                policy: _, shell: _, ssh: _, tool_providers: _, legacy_projects_dir: _, $($field: _),+
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
        host_context,
        max_concurrent_jobs,
        acp,
        mcp_gateway,
        owner,
        poll_interval_ms,
        project_registry_dir,
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

pub(crate) fn max_concurrent_jobs(cfg: &RunnerConfig) -> usize {
    cfg.max_concurrent_jobs
        .unwrap_or(DEFAULT_MAX_CONCURRENT_JOBS)
        .clamp(1, JOB_INVENTORY_MAX_ACTIVE_JOBS)
}

fn default_client_base_dir() -> Result<PathBuf, String> {
    webcodex_runner_config::paths::default_client_config_base_dir()
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

pub(crate) fn client_profile_runner_config(profile: &str) -> Result<PathBuf, String> {
    webcodex_runner_config::paths::resolve_runner_config_path(
        &default_client_base_dir()?.join("clients").join(profile),
    )
}

pub(crate) fn default_config_path() -> Result<PathBuf, String> {
    let user_dir = default_client_base_dir()?;
    if let Some(path) = webcodex_runner_config::paths::existing_runner_config_path(&user_dir)? {
        return Ok(path);
    }
    #[cfg(not(windows))]
    {
        let system_dir = PathBuf::from(DEFAULT_SYSTEM_CONFIG_DIR);
        if system_dir != user_dir {
            if let Some(path) =
                webcodex_runner_config::paths::existing_runner_config_path(&system_dir)?
            {
                return Ok(path);
            }
        }
    }
    Ok(user_dir.join(webcodex_runner_config::paths::RUNNER_CONFIG_FILE))
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
    if quic.keepalive_interval_secs > MAX_QUIC_KEEPALIVE_INTERVAL_SECS {
        return Err(format!(
            "[quic] keepalive_interval_secs must be <= {MAX_QUIC_KEEPALIVE_INTERVAL_SECS}"
        ));
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

pub(crate) fn load_config(path: &Path) -> Result<RunnerConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read config {}: {}", path.display(), e))?;
    validate_shell_profile_toml_shape(&content)
        .map_err(|e| format!("failed to parse config {}: {}", path.display(), e))?;
    let mut cfg: RunnerConfig = toml::from_str(&content)
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
    if let Some(host_context) = cfg.host_context.take() {
        cfg.host_context = Some(host_context.normalized()?);
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
    // minimal Runner config without an explicit policy.allowed_roots still works
    // predictably. If HOME is unavailable and allow_cwd_anywhere is false,
    // surface a clear configuration error. Explicit allowed_roots is preserved
    // as-is and overrides the HOME default.
    let effective =
        effective_allowed_roots(&cfg.policy.allowed_roots, cfg.policy.allow_cwd_anywhere)?;
    cfg.policy.allowed_roots = effective;
    // Normalize old/new config spellings into one effective registry path. Two
    // explicit fields are ambiguous and fail closed rather than guessing
    // precedence. With neither field configured, select the on-disk layout
    // using the shared four-state compatibility contract.
    cfg.project_registry_dir = match (
        cfg.project_registry_dir.take(),
        cfg.legacy_projects_dir.take(),
    ) {
        (Some(_), Some(_)) => {
            return Err(
                "project_registry_dir and legacy projects_dir cannot both be configured; keep exactly one Runner project registry setting"
                    .to_string(),
            );
        }
        (Some(path), None) | (None, Some(path)) => Some(path),
        (None, None) => Some(default_project_registry_dir()?),
    };
    validate_shell_config(&cfg.shell)?;
    validate_ssh_config(&mut cfg.ssh)?;
    if let Some(quic) = &cfg.quic {
        validate_quic_config(quic)?;
    } else if cfg.transport.as_deref().map(str::trim) == Some(TRANSPORT_QUIC) {
        return Err("transport=quic requires a [quic] section in the Runner config".to_string());
    }
    if cfg.tool_providers.claude_code.enabled {
        if cfg.tool_providers.claude_code.command.trim().is_empty() {
            return Err("tool_providers.claude_code.command cannot be empty".to_string());
        }
        if cfg.tool_providers.claude_code.timeout_secs == 0 {
            return Err("tool_providers.claude_code.timeout_secs must be > 0".to_string());
        }
    }
    validate_mcp_gateway_config(&cfg.mcp_gateway)?;
    validate_acp_config(&cfg.acp)?;
    Ok(cfg)
}

fn validate_acp_env_name(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value.len() > ACP_MAX_ENV_NAME_BYTES
        || !value.is_ascii()
        || value.contains('\0')
        || value.contains('=')
    {
        return Err(());
    }
    Ok(())
}

fn validate_acp_config(config: &AcpConfig) -> Result<(), String> {
    use std::collections::HashSet;
    use webcodex_core::coding_agent::{
        validate_provider_id, CODING_AGENT_MAX_CONFIG_KEY_BYTES, CODING_AGENT_MAX_PROVIDERS,
        CODING_AGENT_MAX_PROVIDER_NAME_BYTES,
    };

    if !(1..=8).contains(&config.max_concurrent_runs) {
        return Err("acp.max_concurrent_runs must be between 1 and 8".to_string());
    }
    if !(1..=60).contains(&config.permission_timeout_secs) {
        return Err("acp.permission_timeout_secs must be between 1 and 60".to_string());
    }
    if config.agents.len() > CODING_AGENT_MAX_PROVIDERS {
        return Err(format!(
            "acp.agents may contain at most {CODING_AGENT_MAX_PROVIDERS} entries"
        ));
    }
    let mut ids = HashSet::new();
    for agent in &config.agents {
        validate_provider_id(&agent.id)
            .map_err(|error| format!("ACP agent id is invalid: {error}"))?;
        if agent.name.trim().is_empty()
            || agent.name.len() > CODING_AGENT_MAX_PROVIDER_NAME_BYTES
            || agent.name.chars().any(char::is_control)
        {
            return Err(format!("ACP agent '{}' name is invalid", agent.id));
        }
        if !ids.insert(agent.id.as_str()) {
            return Err("ACP agent ids must be unique".to_string());
        }
        if agent.executable.is_empty()
            || agent.executable.len() > 1024
            || agent.executable.contains('\0')
            || !Path::new(&agent.executable).is_absolute()
        {
            return Err(format!(
                "ACP agent '{}' executable must be an absolute path of at most 1024 bytes",
                agent.id
            ));
        }
        if agent.args.len() > ACP_MAX_ARGS {
            return Err(format!(
                "ACP agent '{}' args may contain at most {ACP_MAX_ARGS} entries",
                agent.id
            ));
        }
        let mut args_bytes = 0usize;
        for arg in &agent.args {
            if arg.len() > ACP_MAX_ARG_BYTES || arg.contains('\0') {
                return Err(format!(
                    "ACP agent '{}' contains an invalid argument",
                    agent.id
                ));
            }
            args_bytes = args_bytes.saturating_add(arg.len()).saturating_add(1);
        }
        if args_bytes > ACP_MAX_ARGS_BYTES {
            return Err(format!(
                "ACP agent '{}' args exceed {ACP_MAX_ARGS_BYTES} bytes",
                agent.id
            ));
        }
        if agent.env_from_env.len() > ACP_MAX_ENV_MAPPINGS {
            return Err(format!(
                "ACP agent '{}' env_from_env may contain at most {ACP_MAX_ENV_MAPPINGS} entries",
                agent.id
            ));
        }
        let mut destinations: Vec<&str> = Vec::new();
        for (destination, source) in &agent.env_from_env {
            if validate_acp_env_name(destination).is_err() || validate_acp_env_name(source).is_err()
            {
                return Err(format!(
                    "ACP agent '{}' env_from_env contains an invalid environment variable name",
                    agent.id
                ));
            }
            if super::shell::is_sensitive_env_key(destination)
                || super::shell::is_sensitive_env_key(source)
            {
                return Err(format!(
                    "ACP agent '{}' env_from_env may not map WebCodex transport credentials",
                    agent.id
                ));
            }
            if destinations
                .iter()
                .any(|existing| super::shell::env_keys_equal(existing, destination))
            {
                return Err(format!("ACP agent '{}' env_from_env contains conflicting destination names for this platform", agent.id));
            }
            destinations.push(destination);
        }
        if agent.allowed_config_options.len() > 64 {
            return Err(format!(
                "ACP agent '{}' allowed_config_options may contain at most 64 entries",
                agent.id
            ));
        }
        let mut config_ids = HashSet::new();
        for option in &agent.allowed_config_options {
            if option.is_empty()
                || option.len() > CODING_AGENT_MAX_CONFIG_KEY_BYTES
                || option.contains(['\0', '\r', '\n'])
                || !config_ids.insert(option.as_str())
            {
                return Err(format!(
                    "ACP agent '{}' contains an invalid or duplicate allowed config option",
                    agent.id
                ));
            }
        }
    }
    Ok(())
}

fn validate_mcp_gateway_env_name(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value.len() > MCP_GATEWAY_MAX_ENV_NAME_BYTES
        || !value.is_ascii()
        || value.contains('\0')
        || value.contains('=')
    {
        return Err(());
    }
    Ok(())
}

fn validate_mcp_gateway_config(config: &McpGatewayConfig) -> Result<(), String> {
    use crate::mcp_gateway::{
        validate_provider_id, validate_provider_name, MCP_GATEWAY_MAX_PROVIDERS,
    };
    use std::collections::HashSet;

    if !(1..=120).contains(&config.request_timeout_secs) {
        return Err("mcp.request_timeout_secs must be between 1 and 120".to_string());
    }
    if config.providers.len() > MCP_GATEWAY_MAX_PROVIDERS {
        return Err(format!(
            "mcp.providers may contain at most {MCP_GATEWAY_MAX_PROVIDERS} entries"
        ));
    }
    let mut ids = HashSet::new();
    for provider in &config.providers {
        validate_provider_id(&provider.id)
            .map_err(|error| format!("mcp provider id is invalid: {error}"))?;
        validate_provider_name(&provider.name)
            .map_err(|error| format!("mcp provider name is invalid: {error}"))?;
        if provider
            .timeout_secs
            .is_some_and(|timeout| !(1..=120).contains(&timeout))
        {
            return Err(format!(
                "mcp provider '{}' timeout_secs must be between 1 and 120",
                provider.id
            ));
        }
        if !ids.insert(provider.id.as_str()) {
            return Err("mcp provider ids must be unique".to_string());
        }
        if provider.executable.is_empty()
            || provider.executable.len() > 1_024
            || provider.executable.contains('\0')
            || !Path::new(&provider.executable).is_absolute()
        {
            return Err(format!(
                "mcp provider '{}' executable must be an absolute path of at most 1024 bytes",
                provider.id
            ));
        }
        if provider.args.len() > 64 {
            return Err(format!(
                "mcp provider '{}' args may contain at most 64 entries",
                provider.id
            ));
        }
        let mut total = 0usize;
        for argument in &provider.args {
            if argument.len() > 4_096 || argument.contains('\0') {
                return Err(format!(
                    "mcp provider '{}' contains an invalid argument",
                    provider.id
                ));
            }
            total = total.saturating_add(argument.len()).saturating_add(1);
        }
        if total > 16 * 1024 {
            return Err(format!(
                "mcp provider '{}' args exceed 16384 bytes",
                provider.id
            ));
        }
        if let Some(cwd) = provider.cwd.as_deref() {
            if cwd.is_empty()
                || cwd.len() > MCP_GATEWAY_MAX_CWD_BYTES
                || cwd.contains('\0')
                || !Path::new(cwd).is_absolute()
            {
                return Err(format!(
                    "mcp provider '{}' cwd must be an absolute path of at most {MCP_GATEWAY_MAX_CWD_BYTES} bytes",
                    provider.id
                ));
            }
        }
        if provider.env_from_env.len() > MCP_GATEWAY_MAX_ENV_MAPPINGS {
            return Err(format!(
                "mcp provider '{}' env_from_env may contain at most {MCP_GATEWAY_MAX_ENV_MAPPINGS} entries",
                provider.id
            ));
        }
        let mut destinations: Vec<&str> = Vec::with_capacity(provider.env_from_env.len());
        for (destination, source) in &provider.env_from_env {
            if validate_mcp_gateway_env_name(destination).is_err()
                || validate_mcp_gateway_env_name(source).is_err()
            {
                return Err(format!(
                    "mcp provider '{}' env_from_env contains an invalid environment variable name",
                    provider.id
                ));
            }
            if super::shell::is_sensitive_env_key(destination)
                || super::shell::is_sensitive_env_key(source)
            {
                return Err(format!(
                    "mcp provider '{}' env_from_env may not map WebCodex-sensitive environment variables",
                    provider.id
                ));
            }
            if destinations
                .iter()
                .any(|existing| super::shell::env_keys_equal(*existing, destination))
            {
                return Err(format!(
                    "mcp provider '{}' env_from_env contains conflicting destination names for this platform",
                    provider.id
                ));
            }
            destinations.push(destination);
        }
    }
    Ok(())
}

#[cfg(test)]
mod mcp_gateway_config_tests {
    use super::*;

    fn provider() -> McpGatewayProviderConfig {
        McpGatewayProviderConfig {
            id: "provider".to_string(),
            name: "Provider".to_string(),
            executable: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            args: Vec::new(),
            cwd: None,
            env_from_env: BTreeMap::new(),
            timeout_secs: None,
        }
    }

    fn validate(provider: McpGatewayProviderConfig) -> Result<(), String> {
        validate_mcp_gateway_config(&McpGatewayConfig {
            request_timeout_secs: 30,
            providers: vec![provider],
        })
    }

    #[test]
    fn mcp_gateway_execution_context_accepts_explicit_cwd_and_env_mapping() {
        let mut provider = provider();
        provider.cwd = Some(
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
        provider.env_from_env = BTreeMap::from([
            ("GITHUB_TOKEN".to_string(), "GITHUB_TOKEN".to_string()),
            ("HOME".to_string(), "HOME".to_string()),
        ]);
        validate(provider).unwrap();
    }

    #[test]
    fn mcp_gateway_execution_context_rejects_invalid_cwd_and_env_bounds() {
        let mut relative = provider();
        relative.cwd = Some("relative/provider-cwd".to_string());
        assert!(validate(relative)
            .unwrap_err()
            .contains("cwd must be an absolute path"));

        let mut nul_cwd = provider();
        let mut invalid_cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        invalid_cwd.push('\0');
        nul_cwd.cwd = Some(invalid_cwd);
        assert!(validate(nul_cwd)
            .unwrap_err()
            .contains("cwd must be an absolute path"));

        for (destination, source) in [
            ("", "SOURCE"),
            ("BAD=NAME", "SOURCE"),
            ("DEST", "BAD=SOURCE"),
            ("DEST", "BAD\0SOURCE"),
            ("DÉST", "SOURCE"),
            ("DEST", "SOURCÉ"),
        ] {
            let mut invalid = provider();
            invalid
                .env_from_env
                .insert(destination.to_string(), source.to_string());
            assert!(validate(invalid)
                .unwrap_err()
                .contains("invalid environment variable name"));
        }

        let mut long_name = provider();
        long_name.env_from_env.insert(
            "D".repeat(MCP_GATEWAY_MAX_ENV_NAME_BYTES + 1),
            "SOURCE".to_string(),
        );
        assert!(validate(long_name)
            .unwrap_err()
            .contains("invalid environment variable name"));

        let mut too_many = provider();
        too_many.env_from_env = (0..=MCP_GATEWAY_MAX_ENV_MAPPINGS)
            .map(|index| (format!("DEST_{index}"), format!("SOURCE_{index}")))
            .collect();
        assert!(validate(too_many)
            .unwrap_err()
            .contains("env_from_env may contain at most"));
    }

    #[test]
    fn mcp_gateway_execution_context_rejects_sensitive_and_platform_duplicate_names() {
        for (destination, source) in [
            ("WEBCODEX_TOKEN", "SOURCE"),
            ("DEST", "WEBCODEX_AGENT_TOKEN"),
            ("WEBCODEX_USER_TOKEN", "SOURCE"),
            ("DEST", "AUTHORIZATION"),
        ] {
            let mut sensitive = provider();
            sensitive
                .env_from_env
                .insert(destination.to_string(), source.to_string());
            assert!(validate(sensitive)
                .unwrap_err()
                .contains("WebCodex-sensitive"));
        }

        let mut case_pair = provider();
        case_pair.env_from_env = BTreeMap::from([
            ("PATH".to_string(), "SOURCE_A".to_string()),
            ("Path".to_string(), "SOURCE_B".to_string()),
        ]);
        if cfg!(windows) {
            assert!(validate(case_pair)
                .unwrap_err()
                .contains("conflicting destination names"));
        } else {
            validate(case_pair).unwrap();
        }
    }
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

fn default_project_registry_dir() -> Result<PathBuf, String> {
    let base = default_client_base_dir()?;
    crate::runner_config::paths::select_project_registry_dir(&base)
}

pub(crate) fn project_registry_dir(cfg: &RunnerConfig) -> Result<PathBuf, String> {
    match &cfg.project_registry_dir {
        Some(project_registry_dir) => Ok(project_registry_dir.clone()),
        None => default_project_registry_dir(),
    }
}

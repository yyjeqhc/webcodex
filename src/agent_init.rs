//! Shared agent-config initialization logic used by both the `webcodex-agent`
//! binary (`agent init`) and the `webcodex-cli` binary (`agent init`).
//!
//! This module is included via `#[path]` from each binary and depends only on
//! the `shell_protocol` module (also inlined by each binary). It owns the
//! `AgentInitOptions` type, validation, token resolution, TOML generation, and
//! atomic 0600 file writing. Each binary keeps its own small flag parser and
//! help text; the large generation/writing code lives here to avoid
//! duplication.
//!
//! Default policy: when `allowed_roots` is not explicitly configured, it
//! defaults to `$HOME` (see `effective_allowed_roots`).

use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::shell_protocol::ShellClientCapabilities;

/// Default projects directory written into generated agent configs.
pub const DEFAULT_INIT_PROJECTS_DIR: &str = "/etc/webcodex/projects.d";
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;
pub const DEFAULT_MAX_TIMEOUT_SECS: u64 = 3600;
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;
/// Config value selecting the polling transport (HTTP `/api/shell/agent/poll`).
pub const TRANSPORT_POLLING: &str = "polling";
/// Config value selecting the WebSocket transport.
pub const TRANSPORT_WEBSOCKET: &str = "websocket";
/// Config value selecting the supported custom QUIC stream transport.
/// Requires a `[quic]` section in `agent.toml` with `server_addr` / `server_name`.
pub const TRANSPORT_QUIC: &str = "quic";
/// Recommended fallback mode for new deployments: try QUIC when `[quic]` is
/// configured, then WebSocket, then polling.
pub const TRANSPORT_AUTO: &str = "auto";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentInitOptions {
    pub server_url: String,
    pub token: Option<String>,
    pub token_file: Option<PathBuf>,
    pub client_id: String,
    pub owner: String,
    pub display_name: Option<String>,
    pub transport: String,
    pub poll_interval_ms: u64,
    pub projects_dir: PathBuf,
    pub output: PathBuf,
    pub allowed_roots: Vec<PathBuf>,
    pub allow_cwd_anywhere: bool,
    pub overwrite: bool,
}

/// Return `$HOME` as an allowed-root candidate, when set and non-empty.
pub fn home_allowed_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// Resolve the effective `allowed_roots` for an agent policy.
///
/// - If `configured` is non-empty, the configured value is used as-is
///   (overrides the HOME default).
/// - If `configured` is empty, default to `[$HOME]` when `HOME` is available
///   (regardless of `allow_cwd_anywhere` — adding HOME as an explicit root is
///   harmless when cwd-anywhere is also enabled, and makes the policy summary
///   + create/register validation predictable).
/// - If `configured` is empty, `HOME` is unavailable, and `allow_cwd_anywhere`
///   is true, return an empty vector (the cwd-anywhere policy still governs
///   access; sensitive path protections apply at validation time).
/// - If `configured` is empty, `HOME` is unavailable, and
///   `allow_cwd_anywhere` is false, surface a clear configuration error.
pub fn effective_allowed_roots(
    configured: &[PathBuf],
    allow_cwd_anywhere: bool,
) -> Result<Vec<PathBuf>, String> {
    if !configured.is_empty() {
        return Ok(configured.to_vec());
    }
    if let Some(home) = home_allowed_root() {
        return Ok(vec![home]);
    }
    if allow_cwd_anywhere {
        return Ok(Vec::new());
    }
    Err(
        "allowed_roots is empty and HOME is not set; set allowed_roots explicitly, \
         set --allow-cwd-anywhere true, or set the HOME environment variable"
            .to_string(),
    )
}

pub fn validate_agent_init_options(opts: &AgentInitOptions) -> Result<(), String> {
    if opts.server_url.trim().is_empty() {
        return Err("--server-url is required".to_string());
    }
    if opts.token.is_some() && opts.token_file.is_some() {
        return Err("use only one of --token or --token-file".to_string());
    }
    if opts.client_id.trim().is_empty() {
        return Err("--client-id is required".to_string());
    }
    if opts.owner.trim().is_empty() {
        return Err("--owner is required".to_string());
    }
    if opts.poll_interval_ms == 0 {
        return Err("--poll-interval-ms must be > 0".to_string());
    }
    if !matches!(
        opts.transport.as_str(),
        TRANSPORT_WEBSOCKET | TRANSPORT_POLLING | TRANSPORT_QUIC | TRANSPORT_AUTO
    ) {
        return Err("--transport must be websocket, polling, quic, or auto".to_string());
    }
    if opts.projects_dir.as_os_str().is_empty() {
        return Err("--projects-dir cannot be empty".to_string());
    }
    if opts.output.as_os_str().is_empty() {
        return Err("--output is required".to_string());
    }
    // Empty allowed_roots no longer errors here; the HOME default
    // is resolved at generation time via `effective_allowed_roots`. We still
    // reject empty path entries so an explicit `--allowed-root ""` is caught.
    if opts
        .allowed_roots
        .iter()
        .any(|path| path.as_os_str().is_empty())
    {
        return Err("--allowed-root cannot be empty".to_string());
    }
    Ok(())
}

pub fn resolve_agent_init_token(opts: &AgentInitOptions) -> Result<String, String> {
    if let Some(token) = &opts.token {
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err("--token cannot be empty".to_string());
        }
        return Ok(token);
    }
    if let Some(path) = &opts.token_file {
        let token = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read token file {}: {}", path.display(), e))?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err("--token-file cannot be empty".to_string());
        }
        return Ok(token);
    }
    let token = std::env::var("WEBCODEX_AGENT_TOKEN")
        .map_err(|_| "--token, --token-file, or WEBCODEX_AGENT_TOKEN is required".to_string())?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err("WEBCODEX_AGENT_TOKEN cannot be empty".to_string());
    }
    Ok(token)
}

#[derive(Debug, Serialize)]
struct GeneratedAgentConfig {
    server_url: String,
    token: String,
    client_id: String,
    display_name: Option<String>,
    owner: String,
    transport: String,
    poll_interval_ms: u64,
    projects_dir: PathBuf,
    capabilities: ShellClientCapabilities,
    policy: GeneratedAgentPolicy,
}

#[derive(Debug, Serialize)]
struct GeneratedAgentPolicy {
    allow_raw_shell: bool,
    allow_cwd_anywhere: bool,
    allowed_roots: Vec<PathBuf>,
    max_timeout_secs: u64,
    max_output_bytes: usize,
}

pub fn generated_agent_config_toml(opts: &AgentInitOptions) -> Result<String, String> {
    let effective_roots = effective_allowed_roots(&opts.allowed_roots, opts.allow_cwd_anywhere)?;
    let cfg = GeneratedAgentConfig {
        server_url: opts.server_url.trim_end_matches('/').to_string(),
        token: resolve_agent_init_token(opts)?,
        client_id: opts.client_id.clone(),
        display_name: opts.display_name.clone(),
        owner: opts.owner.clone(),
        transport: opts.transport.clone(),
        poll_interval_ms: opts.poll_interval_ms,
        projects_dir: opts.projects_dir.clone(),
        capabilities: ShellClientCapabilities {
            shell: true,
            file_read: true,
            file_write: true,
            git: true,
            jobs: true,
            async_jobs: true,
            async_shell_jobs: true,
            lsp_read_only_navigation: true,
        },
        policy: GeneratedAgentPolicy {
            allow_raw_shell: true,
            allow_cwd_anywhere: opts.allow_cwd_anywhere,
            allowed_roots: effective_roots,
            max_timeout_secs: DEFAULT_MAX_TIMEOUT_SECS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        },
    };
    toml::to_string_pretty(&cfg).map_err(|e| format!("failed to serialize agent config: {}", e))
}

/// Generate the agent config TOML and write it to `opts.output`.
///
/// - `--output -` writes the TOML to stdout (returned as a string). The caller
///   is responsible for printing it; this is the only path that returns the
///   token-bearing content, and only when the user explicitly asks for stdout.
/// - Refuses to overwrite an existing file unless `--overwrite` is set.
/// - Writes 0600 permissions on Unix.
pub fn run_agent_init(opts: AgentInitOptions) -> Result<String, String> {
    let content = generated_agent_config_toml(&opts)?;
    if opts.output == PathBuf::from("-") {
        return Ok(content);
    }
    if opts.output.exists() && !opts.overwrite {
        return Err(format!(
            "{} already exists; pass --overwrite to replace it",
            opts.output.display()
        ));
    }
    if let Some(parent) = opts.output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
    }
    let mut open = OpenOptions::new();
    open.write(true);
    if opts.overwrite {
        open.create(true).truncate(true);
    } else {
        open.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    let mut file = open
        .open(&opts.output)
        .map_err(|e| format!("failed to write {}: {}", opts.output.display(), e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("failed to write {}: {}", opts.output.display(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&opts.output, std::fs::Permissions::from_mode(0o600)).map_err(
            |e| {
                format!(
                    "failed to set permissions on {}: {}",
                    opts.output.display(),
                    e
                )
            },
        )?;
    }
    Ok(format!("wrote {}\n", opts.output.display()))
}

/// Test-only mutex serializing tests that mutate process-wide environment
/// variables (`HOME`, `WEBCODEX_AGENT_TOKEN`). Declared at module level so
/// both `agent_init::tests` and tests in binaries that inline this module can
/// acquire the same lock via `agent_init::TEST_ENV_LOCK`.
#[cfg(test)]
pub static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn parse_bool(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err("boolean value must be true or false".to_string()),
    }
}

pub fn required_value<'a, I>(iter: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = &'a String>,
{
    iter.next()
        .cloned()
        .ok_or_else(|| format!("{} requires a value", flag))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_opts(output: PathBuf) -> AgentInitOptions {
        AgentInitOptions {
            server_url: "https://v4.example.test/".to_string(),
            token: Some("wc_agent_fake_test_token".to_string()),
            token_file: None,
            client_id: "alice-laptop".to_string(),
            owner: "alice".to_string(),
            display_name: Some("Alice Laptop".to_string()),
            transport: TRANSPORT_WEBSOCKET.to_string(),
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            projects_dir: PathBuf::from("/etc/webcodex/projects.d"),
            output,
            allowed_roots: vec![PathBuf::from("/srv/projects")],
            allow_cwd_anywhere: false,
            overwrite: false,
        }
    }

    #[test]
    fn effective_allowed_roots_uses_explicit_when_present() {
        let roots = effective_allowed_roots(&[PathBuf::from("/root/git")], false).unwrap();
        assert_eq!(roots, vec![PathBuf::from("/root/git")]);
    }

    #[test]
    fn effective_allowed_roots_defaults_to_home_when_empty() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if let Some(home) = home {
            let roots = effective_allowed_roots(&[], false).unwrap();
            assert_eq!(roots, vec![home]);
        }
    }

    #[test]
    fn effective_allowed_roots_errors_when_empty_and_no_home_and_no_cwd_anywhere() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let saved = std::env::var_os("HOME");
        std::env::remove_var("HOME");
        let err = effective_allowed_roots(&[], false).unwrap_err();
        assert!(err.contains("allowed_roots is empty"));
        match saved {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn effective_allowed_roots_empty_with_cwd_anywhere_returns_empty() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let saved = std::env::var_os("HOME");
        std::env::remove_var("HOME");
        let roots = effective_allowed_roots(&[], true).unwrap();
        assert!(roots.is_empty());
        match saved {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn generated_config_uses_home_default_when_roots_empty() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if let Some(home) = home {
            let mut opts = init_opts(PathBuf::from("-"));
            opts.allowed_roots.clear();
            let content = generated_agent_config_toml(&opts).unwrap();
            assert!(content.contains(&format!("allowed_roots = [\"{}\"]", home.to_string_lossy())));
        }
    }
}

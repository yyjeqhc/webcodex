//! Project-first hosted-client connection orchestration.
//!
//! This is deliberately a thin vertical slice over the existing server and
//! agent. It owns local process supervision and ingress preflight, but it does
//! not move tunnel-provider concepts into the runtime/tool domain.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use url::Url;
use uuid::Uuid;

const DEFAULT_PROFILE: &str = "personal";
const DEFAULT_USERNAME: &str = "owner";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const PUBLIC_INGRESS_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostedTarget {
    ChatGpt,
    Mcp,
    GptActions,
}

impl HostedTarget {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "chatgpt" => Ok(Self::ChatGpt),
            "mcp" => Ok(Self::Mcp),
            "gpt-actions" => Ok(Self::GptActions),
            _ => Err(format!(
                "unknown target '{value}'; expected chatgpt, mcp, or gpt-actions"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ChatGpt => "chatgpt",
            Self::Mcp => "mcp",
            Self::GptActions => "gpt-actions",
        }
    }

    fn uses_mcp(self) -> bool {
        matches!(self, Self::ChatGpt | Self::Mcp)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostedIngress {
    OpenAi,
    Cloudflare,
}

impl HostedIngress {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "openai" => Ok(Self::OpenAi),
            "cloudflare" => Ok(Self::Cloudflare),
            _ => Err(format!(
                "unknown ingress '{value}'; expected openai or cloudflare"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Cloudflare => "cloudflare",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostedConnectOptions {
    target: HostedTarget,
    ingress: HostedIngress,
    root: PathBuf,
    state_dir: Option<PathBuf>,
    profile: String,
    port: Option<u16>,
    temporary: bool,
    tunnel_id: Option<String>,
    public_url: Option<String>,
    cloudflare_config: Option<PathBuf>,
    cloudflare_tunnel: Option<String>,
    dry_run: bool,
}

pub(crate) fn usage() -> &'static str {
    "Usage: webcodex connect <TARGET> --via <INGRESS> [OPTIONS]\n\
\n\
Targets:\n\
  chatgpt                 ChatGPT connector over MCP\n\
  mcp                     Standard remote MCP client\n\
  gpt-actions             GPT Actions/OpenAPI client\n\
\n\
Ingress:\n\
  --via openai            OpenAI Secure MCP Tunnel (MCP targets only)\n\
  --via cloudflare        Cloudflare Named Tunnel, or Quick Tunnel with --temporary\n\
\n\
Project/runtime options:\n\
  --root PATH             Project path; defaults to the current directory\n\
  --profile NAME          Isolated local profile; default personal\n\
  --state-dir PATH        Override private local runtime state directory\n\
  --port PORT             Loopback origin port; Named Tunnel defaults to 8787, others auto-select\n\
  --dry-run               Print a secret-free preflight plan without writing state\n\
\n\
OpenAI options:\n\
  --tunnel-id ID          Secure MCP tunnel id; defaults to CONTROL_PLANE_TUNNEL_ID\n\
                          CONTROL_PLANE_API_KEY must be set in the environment\n\
\n\
Cloudflare options:\n\
  --temporary             Use a test-only random Quick Tunnel (GPT Actions only)\n\
  --public-url HTTPS_URL  Stable hostname already routed by a Named Tunnel\n\
  --cloudflare-config P   cloudflared config; defaults to ~/.cloudflared/config.yml\n\
  --cloudflare-tunnel ID  Named tunnel name/UUID; defaults to --profile\n\
\n\
Provider credentials are never accepted as command-line values.\n"
}

pub(crate) fn parse(args: &[String]) -> Result<HostedConnectOptions, String> {
    let Some(target) = args.first() else {
        return Err("missing connect target".to_string());
    };
    if matches!(target.as_str(), "--help" | "-h") {
        return Err("help requested".to_string());
    }
    let target = HostedTarget::parse(target)?;
    let mut ingress = None;
    let mut root = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
    let mut state_dir = None;
    let mut profile = DEFAULT_PROFILE.to_string();
    let mut port = None;
    let mut temporary = false;
    let mut tunnel_id = None;
    let mut public_url = None;
    let mut cloudflare_config = None;
    let mut cloudflare_tunnel = None;
    let mut dry_run = false;

    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = |index: &mut usize| -> Result<String, String> {
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match flag {
            "--via" => ingress = Some(HostedIngress::parse(&value(&mut index)?)?),
            "--root" => root = PathBuf::from(value(&mut index)?),
            "--state-dir" => state_dir = Some(PathBuf::from(value(&mut index)?)),
            "--profile" => profile = value(&mut index)?,
            "--port" => {
                let raw = value(&mut index)?;
                let parsed = raw
                    .parse::<u16>()
                    .map_err(|_| "--port must be an integer from 1 to 65535".to_string())?;
                if parsed == 0 {
                    return Err("--port must be an integer from 1 to 65535".to_string());
                }
                port = Some(parsed);
            }
            "--temporary" => temporary = true,
            "--tunnel-id" => tunnel_id = Some(value(&mut index)?),
            "--public-url" => public_url = Some(value(&mut index)?),
            "--cloudflare-config" => cloudflare_config = Some(PathBuf::from(value(&mut index)?)),
            "--cloudflare-tunnel" => cloudflare_tunnel = Some(value(&mut index)?),
            "--dry-run" => dry_run = true,
            "--help" | "-h" => return Err("help requested".to_string()),
            _ => return Err(format!("unknown connect option '{flag}'")),
        }
        index += 1;
    }

    let ingress = ingress.ok_or_else(|| "--via is required".to_string())?;
    validate_profile_name(&profile)?;
    if root.as_os_str().is_empty() {
        return Err("--root cannot be empty".to_string());
    }
    if state_dir
        .as_ref()
        .is_some_and(|path| path.as_os_str().is_empty())
    {
        return Err("--state-dir cannot be empty".to_string());
    }

    match ingress {
        HostedIngress::OpenAi => {
            if target == HostedTarget::GptActions {
                return Err(
                    "OpenAI Secure MCP Tunnel supports MCP, not GPT Actions; use target chatgpt/mcp or --via cloudflare"
                        .to_string(),
                );
            }
            if temporary {
                return Err("--temporary is only valid with --via cloudflare".to_string());
            }
            if public_url.is_some() || cloudflare_config.is_some() || cloudflare_tunnel.is_some() {
                return Err("Cloudflare options cannot be used with --via openai".to_string());
            }
            if let Some(id) = tunnel_id.as_deref() {
                validate_openai_tunnel_id(id)?;
            }
        }
        HostedIngress::Cloudflare => {
            if tunnel_id.is_some() {
                return Err("--tunnel-id is only valid with --via openai".to_string());
            }
            if temporary {
                if target != HostedTarget::GptActions {
                    return Err(
                        "Cloudflare Quick Tunnel does not support SSE; this first iteration only enables it for gpt-actions"
                            .to_string(),
                    );
                }
                if public_url.is_some()
                    || cloudflare_config.is_some()
                    || cloudflare_tunnel.is_some()
                {
                    return Err(
                        "--temporary cannot be combined with Named Tunnel options".to_string()
                    );
                }
            } else {
                let url = public_url.as_deref().ok_or_else(|| {
                    "Cloudflare Named Tunnel requires --public-url https://...; use --temporary only for a test URL"
                        .to_string()
                })?;
                validate_public_https_url(url)?;
                if cloudflare_tunnel
                    .as_deref()
                    .is_some_and(|name| name.trim().is_empty())
                {
                    return Err("--cloudflare-tunnel cannot be empty".to_string());
                }
            }
        }
    }

    Ok(HostedConnectOptions {
        target,
        ingress,
        root,
        state_dir,
        profile,
        port,
        temporary,
        tunnel_id,
        public_url,
        cloudflare_config,
        cloudflare_tunnel,
        dry_run,
    })
}

fn validate_profile_name(profile: &str) -> Result<(), String> {
    if profile.is_empty()
        || !profile
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        || matches!(profile, "." | "..")
    {
        return Err(
            "--profile may only contain ASCII letters, digits, '-', '_', and '.'".to_string(),
        );
    }
    Ok(())
}

fn validate_openai_tunnel_id(id: &str) -> Result<(), String> {
    let suffix = id.strip_prefix("tunnel_").unwrap_or_default();
    if suffix.len() != 32
        || !suffix
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        return Err("OpenAI tunnel id must be tunnel_ followed by 32 lowercase hex digits".into());
    }
    Ok(())
}

fn validate_public_https_url(value: &str) -> Result<(), String> {
    let url = Url::parse(value).map_err(|e| format!("invalid --public-url: {e}"))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err("--public-url must be an absolute https URL".to_string());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("--public-url must not contain a query or fragment".to_string());
    }
    Ok(())
}

#[derive(Debug)]
struct ConnectPaths {
    state: PathBuf,
    data: PathBuf,
    credentials: PathBuf,
    projects: PathBuf,
    runs: PathBuf,
    results: PathBuf,
    logs: PathBuf,
    bootstrap_token: PathBuf,
    user_token: PathBuf,
    agent_token: PathBuf,
    mcp_authorization: PathBuf,
    agent_config: PathBuf,
    tunnel_health_url: PathBuf,
}

impl ConnectPaths {
    fn new(state: PathBuf) -> Self {
        let credentials = state.join("credentials");
        let agent = state.join("agent");
        Self {
            data: state.join("data"),
            projects: agent.join("projects.d"),
            runs: state.join("runs"),
            results: state.join("results"),
            logs: state.join("logs"),
            bootstrap_token: credentials.join("bootstrap-token"),
            user_token: credentials.join("webcodex-user-token"),
            agent_token: credentials.join("webcodex-agent-token"),
            mcp_authorization: credentials.join("mcp-authorization"),
            agent_config: agent.join("agent.toml"),
            tunnel_health_url: state.join("tunnel-health.url"),
            credentials,
            state,
        }
    }

    fn create(&self) -> Result<(), String> {
        for path in [
            &self.state,
            &self.data,
            &self.credentials,
            &self.projects,
            &self.runs,
            &self.results,
            &self.logs,
        ] {
            create_private_dir(path)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalTaskState {
    pub root: PathBuf,
    pub state: PathBuf,
    pub data: PathBuf,
    pub runs: PathBuf,
    pub projects: PathBuf,
    pub logical_project_id: String,
}

pub(crate) fn resolve_local_task_state(
    root: &Path,
    profile: &str,
    state_dir: Option<&Path>,
) -> Result<LocalTaskState, String> {
    validate_profile_name(profile)?;
    let root = discover_project_root(root)?;
    let identity = project_identity(&root);
    let executor_project_id = format!("{}-{}", safe_slug(&root), &identity[..10]);
    let state = match state_dir {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => std::env::current_dir()
            .map_err(|error| format!("cannot read cwd: {error}"))?
            .join(path),
        None => default_state_base()?
            .join(profile)
            .join(executor_project_id),
    };
    let paths = ConnectPaths::new(state.clone());
    Ok(LocalTaskState {
        root,
        state,
        data: paths.data,
        runs: paths.runs,
        projects: paths.projects,
        logical_project_id: format!("wc_proj_{}", &identity[..20]),
    })
}

#[derive(Debug)]
struct ResolvedConnect {
    opts: HostedConnectOptions,
    root: PathBuf,
    paths: ConnectPaths,
    port: u16,
    project_id: String,
    logical_project_id: String,
    workspace_id: String,
    project_name: String,
    client_id: String,
    runtime_project_id: String,
    server_bin: PathBuf,
    cli_bin: Option<PathBuf>,
    agent_bin: Option<PathBuf>,
    ingress_bin: Option<PathBuf>,
    tunnel_id: Option<String>,
    cloudflare_config: Option<PathBuf>,
    cloudflare_tunnel: Option<String>,
}

impl ResolvedConnect {
    fn resolve(opts: HostedConnectOptions) -> Result<Self, String> {
        let local_state =
            resolve_local_task_state(&opts.root, &opts.profile, opts.state_dir.as_deref())?;
        let root = local_state.root;
        let identity = project_identity(&root);
        let project_id = format!("{}-{}", safe_slug(&root), &identity[..10]);
        let logical_project_id = local_state.logical_project_id;
        let workspace_identity = format!(
            "{:x}",
            Sha256::digest(format!("{}\0{}", identity, opts.profile).as_bytes())
        );
        let workspace_id = format!("wc_ws_{}", &workspace_identity[..20]);
        let project_name = safe_slug(&root);
        let client_id = format!("hosted-{}", &identity[..12]);
        let runtime_project_id = format!("agent:{client_id}:{project_id}");
        let state = local_state.state;
        let port = match opts.port {
            Some(port) => port,
            None if opts.ingress == HostedIngress::Cloudflare && !opts.temporary => 8787,
            None => available_loopback_port()?,
        };
        let server_bin = std::env::current_exe()
            .map_err(|e| format!("cannot locate current webcodex executable: {e}"))?;
        let sibling_dir = server_bin.parent().unwrap_or_else(|| Path::new("."));
        let cli_bin = locate_binary("WEBCODEX_CLI_BIN", "webcodex-cli", sibling_dir);
        let agent_bin = locate_binary("WEBCODEX_AGENT_BIN", "webcodex-agent", sibling_dir);
        let ingress_bin = match opts.ingress {
            HostedIngress::OpenAi => {
                locate_binary("WEBCODEX_TUNNEL_CLIENT_BIN", "tunnel-client", sibling_dir)
            }
            HostedIngress::Cloudflare => {
                locate_binary("WEBCODEX_CLOUDFLARED_BIN", "cloudflared", sibling_dir)
            }
        };
        let tunnel_id = if opts.ingress == HostedIngress::OpenAi {
            opts.tunnel_id
                .clone()
                .or_else(|| nonempty_env("CONTROL_PLANE_TUNNEL_ID"))
        } else {
            None
        };
        if let Some(id) = tunnel_id.as_deref() {
            validate_openai_tunnel_id(id)?;
        }
        let cloudflare_config = if opts.ingress == HostedIngress::Cloudflare && !opts.temporary {
            match &opts.cloudflare_config {
                Some(path) => Some(expand_home(path)?),
                None => Some(home_dir()?.join(".cloudflared/config.yml")),
            }
        } else {
            None
        };
        let cloudflare_tunnel = if opts.ingress == HostedIngress::Cloudflare && !opts.temporary {
            Some(
                opts.cloudflare_tunnel
                    .clone()
                    .unwrap_or_else(|| opts.profile.clone()),
            )
        } else {
            None
        };
        Ok(Self {
            opts,
            root,
            paths: ConnectPaths::new(state),
            port,
            project_id,
            logical_project_id,
            workspace_id,
            project_name,
            client_id,
            runtime_project_id,
            server_bin,
            cli_bin,
            agent_bin,
            ingress_bin,
            tunnel_id,
            cloudflare_config,
            cloudflare_tunnel,
        })
    }

    fn local_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn preflight(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if self.cli_bin.is_none() {
            issues.push("webcodex-cli was not found beside webcodex or on PATH".to_string());
        }
        if self.agent_bin.is_none() {
            issues.push("webcodex-agent was not found beside webcodex or on PATH".to_string());
        }
        if self.ingress_bin.is_none() {
            let name = match self.opts.ingress {
                HostedIngress::OpenAi => "tunnel-client",
                HostedIngress::Cloudflare => "cloudflared",
            };
            issues.push(format!(
                "{name} was not found; install the official provider client and ensure it is on PATH"
            ));
        }
        if self.opts.ingress == HostedIngress::OpenAi {
            if self.tunnel_id.is_none() {
                issues.push(
                    "OpenAI tunnel id is missing (--tunnel-id or CONTROL_PLANE_TUNNEL_ID)"
                        .to_string(),
                );
            }
            if nonempty_env("CONTROL_PLANE_API_KEY").is_none()
                && nonempty_env("OPENAI_API_KEY").is_none()
            {
                issues.push(
                    "CONTROL_PLANE_API_KEY is missing (OPENAI_API_KEY is also accepted by tunnel-client)"
                        .to_string(),
                );
            }
        }
        if let Some(config) = &self.cloudflare_config {
            if !config.is_file() {
                issues.push(format!(
                    "Cloudflare config does not exist: {}",
                    config.display()
                ));
            }
        }
        issues
    }

    fn dry_run_output(&self, issues: &[String]) -> String {
        let readiness = if issues.is_empty() {
            "ready"
        } else {
            "blocked"
        };
        let mut output = format!(
            "WebCodex connect preflight ({readiness})\n\n  project:  {}\n  scope:    {}\n  target:   {}\n  ingress:  {}{}\n  origin:   {} (loopback only)\n  state:    {}\n",
            self.root.display(),
            self.logical_project_id,
            self.opts.target.as_str(),
            self.opts.ingress.as_str(),
            if self.opts.temporary { " (temporary)" } else { "" },
            self.local_url(),
            self.paths.state.display(),
        );
        if let Some(tunnel_id) = &self.tunnel_id {
            output.push_str(&format!("  tunnel:   {tunnel_id}\n"));
        }
        if let Some(public_url) = &self.opts.public_url {
            output.push_str(&format!("  endpoint: {public_url}\n"));
        }
        if !issues.is_empty() {
            output.push_str("\nBlocking preflight findings:\n");
            for issue in issues {
                output.push_str(&format!("  - {issue}\n"));
            }
        }
        output.push_str("\nNo files were written and no provider process was started.\n");
        output
    }
}

pub(crate) async fn run(opts: HostedConnectOptions) -> Result<(), String> {
    let resolved = ResolvedConnect::resolve(opts)?;
    let issues = resolved.preflight();
    if resolved.opts.dry_run {
        print!("{}", resolved.dry_run_output(&issues));
        return Ok(());
    }
    if !issues.is_empty() {
        return Err(format!(
            "connect preflight failed:\n  - {}\nRun the same command with --dry-run for the full secret-free plan.",
            issues.join("\n  - ")
        ));
    }
    run_resolved(resolved).await
}

async fn run_resolved(resolved: ResolvedConnect) -> Result<(), String> {
    resolved.paths.create()?;
    ensure_port_available(resolved.port)?;

    let cli_bin = resolved.cli_bin.as_deref().expect("preflight checked cli");
    let agent_bin = resolved
        .agent_bin
        .as_deref()
        .expect("preflight checked agent");
    let ingress_bin = resolved
        .ingress_bin
        .as_deref()
        .expect("preflight checked ingress");
    let local_url = resolved.local_url();

    let mut quick_ingress = None;
    let public_url =
        if resolved.opts.ingress == HostedIngress::Cloudflare && resolved.opts.temporary {
            println!("Starting temporary Cloudflare ingress...");
            let log = resolved.paths.logs.join("cloudflared.log");
            let mut command = Command::new(ingress_bin);
            command
                .arg("tunnel")
                .arg("--no-autoupdate")
                .arg("--url")
                .arg(&local_url);
            let mut child = spawn_logged("cloudflared", &mut command, &log)?;
            let url = wait_for_quick_tunnel_url(&mut child, &log).await?;
            quick_ingress = Some(child);
            url
        } else {
            resolved
                .opts
                .public_url
                .clone()
                .unwrap_or_else(|| local_url.clone())
        };

    let bootstrap = ensure_bootstrap_token(&resolved.paths.bootstrap_token)?;
    println!("Starting loopback WebCodex runtime...");
    let server_log = resolved.paths.logs.join("server.log");
    let mut server_command = Command::new(&resolved.server_bin);
    server_command
        .arg("serve")
        .current_dir(&resolved.paths.state)
        .env_remove("WEBCODEX_ENV_FILE")
        .env("WEBCODEX_ADDR", format!("127.0.0.1:{}", resolved.port))
        .env("WEBCODEX_DATA", &resolved.paths.data)
        .env("WEBCODEX_TOKEN", &bootstrap)
        .env("WEBCODEX_ALLOW_ANONYMOUS", "false")
        .env("WEBCODEX_PUBLIC_URL", &public_url)
        .env("WEBCODEX_OAUTH2_ENABLED", "false")
        .env("WEBCODEX_QUIC_ENABLED", "false")
        .env("WEBCODEX_CONNECTOR_SURFACE", "task-v1")
        .env(
            "WEBCODEX_CONNECTOR_PROJECT_ID",
            &resolved.logical_project_id,
        )
        .env("WEBCODEX_CONNECTOR_PROJECT_NAME", &resolved.project_name)
        .env("WEBCODEX_CONNECTOR_WORKSPACE_ID", &resolved.workspace_id)
        .env(
            "WEBCODEX_CONNECTOR_EXECUTOR_PROJECT",
            &resolved.runtime_project_id,
        )
        .env("WEBCODEX_CONNECTOR_EXECUTOR_ROOT", &resolved.root)
        .env("WEBCODEX_CONNECTOR_RUNS_ROOT", &resolved.paths.runs)
        .env("WEBCODEX_CONNECTOR_RESULTS_ROOT", &resolved.paths.results)
        .env("WEBCODEX_CONNECTOR_PROJECTS_DIR", &resolved.paths.projects)
        .env("WEBCODEX_CONNECTOR_PROFILE", &resolved.opts.profile);
    strip_provider_credentials(&mut server_command);
    let mut server = spawn_logged("webcodex runtime", &mut server_command, &server_log)?;
    wait_for_server(&mut server, &local_url, &server_log).await?;

    ensure_managed_tokens(&resolved, cli_bin, &local_url).await?;
    let user_token = read_secret(&resolved.paths.user_token)?;

    ensure_agent_config(&resolved, cli_bin, &local_url).await?;
    write_project_registration(&resolved)?;
    println!("Starting project executor...");
    let agent_log = resolved.paths.logs.join("agent.log");
    let mut agent_command = Command::new(agent_bin);
    agent_command
        .arg("--config")
        .arg(&resolved.paths.agent_config)
        .current_dir(&resolved.paths.state)
        .env_remove("WEBCODEX_TOKEN")
        .env_remove("WEBCODEX_AGENT_TOKEN");
    strip_provider_credentials(&mut agent_command);
    let mut agent = spawn_logged("webcodex agent", &mut agent_command, &agent_log)?;
    wait_for_project(
        &mut agent,
        &local_url,
        &bootstrap,
        &resolved.runtime_project_id,
        &agent_log,
    )
    .await?;
    probe_mcp_initialize(&local_url, &user_token).await?;

    let authorization = format!("Bearer {user_token}\n");
    write_private_file(&resolved.paths.mcp_authorization, authorization.as_bytes())?;
    drop(user_token);

    let mut ingress = if let Some(child) = quick_ingress {
        child
    } else {
        match resolved.opts.ingress {
            HostedIngress::OpenAi => {
                start_openai_ingress(&resolved, ingress_bin, &local_url).await?
            }
            HostedIngress::Cloudflare => start_named_cloudflare_ingress(&resolved, ingress_bin)?,
        }
    };

    match resolved.opts.ingress {
        HostedIngress::OpenAi => {
            wait_for_tunnel_health(
                &mut ingress,
                &resolved.paths.tunnel_health_url,
                &resolved.paths.logs.join("tunnel-client.log"),
            )
            .await?;
        }
        HostedIngress::Cloudflare => {
            wait_for_public_ingress(
                &mut ingress,
                &public_url,
                &resolved.paths.user_token,
                resolved.opts.target,
                &resolved.paths.logs.join("cloudflared.log"),
            )
            .await?;
        }
    }

    print_ready(&resolved, &public_url);
    supervise(&mut server, &mut agent, &mut ingress, &resolved.paths.logs).await
}

fn print_ready(resolved: &ResolvedConnect, public_url: &str) {
    println!("\nWebCodex connector is ready.\n");
    println!("  project:  {}", resolved.root.display());
    println!("  scope:    {}", resolved.logical_project_id);
    println!("  target:   {}", resolved.opts.target.as_str());
    println!("  ingress:  {}", resolved.opts.ingress.as_str());
    match resolved.opts.ingress {
        HostedIngress::OpenAi => {
            println!(
                "  tunnel:   {}",
                resolved.tunnel_id.as_deref().unwrap_or("configured")
            );
            println!("\nNext: select this tunnel in the OpenAI/ChatGPT connector setup.");
        }
        HostedIngress::Cloudflare if resolved.opts.target == HostedTarget::GptActions => {
            println!(
                "  schema:   {}/openapi.json",
                public_url.trim_end_matches('/')
            );
            println!(
                "  API key:  read the private token from {} when the platform asks for Bearer auth",
                resolved.paths.user_token.display()
            );
            if resolved.opts.temporary {
                println!(
                    "\nWarning: this random Quick Tunnel is for testing only, changes on restart, and does not support SSE."
                );
            }
        }
        HostedIngress::Cloudflare => {
            println!("  MCP URL:  {}/mcp", public_url.trim_end_matches('/'));
            println!(
                "  auth:     Bearer token is stored at {}",
                resolved.paths.user_token.display()
            );
        }
    }
    println!("  logs:     {}", resolved.paths.logs.display());
    println!("\nPress Ctrl-C to stop this local runtime and ingress.");
}

async fn ensure_managed_tokens(
    resolved: &ResolvedConnect,
    cli_bin: &Path,
    local_url: &str,
) -> Result<(), String> {
    let user_exists = resolved.paths.user_token.is_file();
    let agent_exists = resolved.paths.agent_token.is_file();
    match (user_exists, agent_exists) {
        (true, true) => return Ok(()),
        (true, false) | (false, true) => {
            return Err(format!(
                "credential state is incomplete under {}; remove that profile directory or restore the missing token file",
                resolved.paths.credentials.display()
            ))
        }
        (false, false) => {}
    }
    println!("Creating separate hosted-client and executor credentials...");
    let log = resolved.paths.logs.join("setup.log");
    let mut command = Command::new(cli_bin);
    command
        .arg("setup")
        .arg("single-user")
        .arg("--server-url")
        .arg(local_url)
        .arg("--token-file")
        .arg(&resolved.paths.bootstrap_token)
        .arg("--username")
        .arg(DEFAULT_USERNAME)
        .arg("--client-id")
        .arg(&resolved.client_id)
        .arg("--display-name")
        .arg("Local owner")
        .arg("--output-dir")
        .arg(&resolved.paths.credentials);
    strip_provider_credentials(&mut command);
    run_logged_command("single-user setup", &mut command, &log).await
}

async fn ensure_agent_config(
    resolved: &ResolvedConnect,
    cli_bin: &Path,
    local_url: &str,
) -> Result<(), String> {
    let log = resolved.paths.logs.join("setup.log");
    let mut command = Command::new(cli_bin);
    command
        .arg("agent")
        .arg("init")
        .arg("--server-url")
        .arg(local_url)
        .arg("--token-file")
        .arg(&resolved.paths.agent_token)
        .arg("--client-id")
        .arg(&resolved.client_id)
        .arg("--owner")
        .arg(DEFAULT_USERNAME)
        .arg("--display-name")
        .arg(format!("{} local executor", safe_slug(&resolved.root)))
        .arg("--transport")
        .arg("websocket")
        .arg("--projects-dir")
        .arg(&resolved.paths.projects)
        .arg("--allowed-root")
        .arg(&resolved.root)
        .arg("--allowed-root")
        .arg(&resolved.paths.runs)
        .arg("--output")
        .arg(&resolved.paths.agent_config)
        .arg("--overwrite");
    strip_provider_credentials(&mut command);
    run_logged_command("agent configuration", &mut command, &log).await
}

#[derive(Serialize)]
struct ManagedProjectFile<'a> {
    id: &'a str,
    path: String,
    name: String,
    kind: &'static str,
    allow_patch: bool,
    disabled: bool,
}

fn write_project_registration(resolved: &ResolvedConnect) -> Result<(), String> {
    let project = ManagedProjectFile {
        id: &resolved.project_id,
        path: resolved.root.to_string_lossy().to_string(),
        name: resolved
            .root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&resolved.project_id)
            .to_string(),
        kind: "auto",
        allow_patch: true,
        disabled: false,
    };
    let content = toml::to_string_pretty(&project)
        .map_err(|e| format!("failed to serialize project registration: {e}"))?;
    write_private_file(
        &resolved
            .paths
            .projects
            .join(format!("{}.toml", resolved.project_id)),
        content.as_bytes(),
    )
}

async fn start_openai_ingress(
    resolved: &ResolvedConnect,
    ingress_bin: &Path,
    local_url: &str,
) -> Result<ManagedChild, String> {
    let tunnel_id = resolved.tunnel_id.as_deref().expect("preflight checked id");
    let log = resolved.paths.logs.join("tunnel-client.log");
    let mcp_url = format!("{}/mcp", local_url.trim_end_matches('/'));
    let header_ref = format!(
        "Authorization: file:{}",
        resolved.paths.mcp_authorization.display()
    );

    println!("Checking OpenAI tunnel permissions and MCP discovery...");
    let mut doctor = Command::new(ingress_bin);
    configure_openai_command(&mut doctor, tunnel_id, &mcp_url, &header_ref);
    doctor.arg("doctor").arg("--explain");
    run_logged_command("tunnel-client doctor", &mut doctor, &log).await?;

    let _ = std::fs::remove_file(&resolved.paths.tunnel_health_url);
    println!("Starting OpenAI Secure MCP Tunnel...");
    let mut command = Command::new(ingress_bin);
    configure_openai_command(&mut command, tunnel_id, &mcp_url, &header_ref);
    command
        .arg("run")
        .arg("--health.listen-addr")
        .arg("127.0.0.1:0")
        .arg("--health.url-file")
        .arg(&resolved.paths.tunnel_health_url);
    spawn_logged("tunnel-client", &mut command, &log)
}

fn configure_openai_command(
    command: &mut Command,
    tunnel_id: &str,
    mcp_url: &str,
    header_ref: &str,
) {
    command
        .env("CONTROL_PLANE_TUNNEL_ID", tunnel_id)
        .env("MCP_SERVER_URL", mcp_url)
        .env("MCP_EXTRA_HEADERS", header_ref)
        .env("MCP_DISCOVERY_EXTRA_HEADERS", header_ref)
        .env_remove("WEBCODEX_TOKEN")
        .env_remove("WEBCODEX_AGENT_TOKEN");
    strip_cloudflare_credentials(command);
    if nonempty_env("CONTROL_PLANE_API_KEY").is_none() {
        command.env_remove("CONTROL_PLANE_API_KEY");
    }
}

fn start_named_cloudflare_ingress(
    resolved: &ResolvedConnect,
    ingress_bin: &Path,
) -> Result<ManagedChild, String> {
    let config = resolved
        .cloudflare_config
        .as_deref()
        .expect("preflight checked cloudflare config");
    let tunnel = resolved
        .cloudflare_tunnel
        .as_deref()
        .expect("preflight checked cloudflare tunnel");
    println!("Starting Cloudflare Named Tunnel...");
    let log = resolved.paths.logs.join("cloudflared.log");
    let mut command = Command::new(ingress_bin);
    command
        .arg("tunnel")
        .arg("--no-autoupdate")
        .arg("--config")
        .arg(config)
        .arg("run")
        .arg(tunnel)
        .env_remove("WEBCODEX_TOKEN")
        .env_remove("WEBCODEX_AGENT_TOKEN");
    strip_openai_credentials(&mut command);
    spawn_logged("cloudflared", &mut command, &log)
}

struct ManagedChild {
    label: &'static str,
    child: Child,
}

impl ManagedChild {
    fn try_exit(&mut self, log: &Path) -> Result<(), String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Err(format!(
                "{} exited early with {}; inspect {}",
                self.label,
                status,
                log.display()
            )),
            Ok(None) => Ok(()),
            Err(e) => Err(format!("failed to inspect {} process: {e}", self.label)),
        }
    }

    async fn stop(&mut self) {
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(3), self.child.wait()).await;
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn spawn_logged(
    label: &'static str,
    command: &mut Command,
    log_path: &Path,
) -> Result<ManagedChild, String> {
    let stdout = open_log(log_path)?;
    let stderr = stdout
        .try_clone()
        .map_err(|e| format!("failed to clone log {}: {e}", log_path.display()))?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .kill_on_drop(true);
    configure_parent_death_signal(command);
    let child = command
        .spawn()
        .map_err(|e| format!("failed to start {label}: {e}"))?;
    Ok(ManagedChild { label, child })
}

#[cfg(target_os = "linux")]
fn configure_parent_death_signal(command: &mut Command) {
    let parent_pid = std::process::id() as libc::pid_t;
    // SAFETY: this closure runs after fork and before exec. It only invokes
    // async-signal-safe libc syscalls and constructs an io::Error on failure.
    // PDEATHSIG prevents supervised runtime/provider children from surviving
    // an abrupt parent loss (for example, an SSH session disappearing).
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent_pid {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "webcodex supervisor exited before child exec",
                ));
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_parent_death_signal(_command: &mut Command) {}

async fn run_logged_command(
    label: &'static str,
    command: &mut Command,
    log_path: &Path,
) -> Result<(), String> {
    let mut child = spawn_logged(label, command, log_path)?;
    let status = child
        .child
        .wait()
        .await
        .map_err(|e| format!("failed waiting for {label}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{label} failed with {status}; inspect {} (credentials are not copied to terminal output)",
            log_path.display()
        ))
    }
}

async fn wait_for_server(
    child: &mut ManagedChild,
    local_url: &str,
    log: &Path,
) -> Result<(), String> {
    let client = http_client()?;
    let url = format!("{}/openapi.json", local_url.trim_end_matches('/'));
    let started = Instant::now();
    while started.elapsed() < STARTUP_TIMEOUT {
        child.try_exit(log)?;
        if client
            .get(&url)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(format!(
        "local runtime did not become ready within {} seconds; inspect {}",
        STARTUP_TIMEOUT.as_secs(),
        log.display()
    ))
}

async fn wait_for_project(
    child: &mut ManagedChild,
    local_url: &str,
    bootstrap_token: &str,
    runtime_project_id: &str,
    log: &Path,
) -> Result<(), String> {
    let client = http_client()?;
    let url = format!("{}/api/projects/list", local_url.trim_end_matches('/'));
    let started = Instant::now();
    while started.elapsed() < STARTUP_TIMEOUT {
        child.try_exit(log)?;
        if let Ok(response) = client
            .post(&url)
            .bearer_auth(bootstrap_token)
            .json(&serde_json::json!({}))
            .send()
            .await
        {
            if matches!(response.status().as_u16(), 401 | 403) {
                return Err(format!(
                    "local bootstrap credential was rejected while checking the executor; the state under {} is inconsistent",
                    log.parent().unwrap_or(log).display()
                ));
            }
            if response.status().is_success() {
                let body = response.text().await.unwrap_or_default();
                if body.contains(runtime_project_id) {
                    return Ok(());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    Err(format!(
        "project executor did not register {} within {} seconds; inspect {}",
        runtime_project_id,
        STARTUP_TIMEOUT.as_secs(),
        log.display()
    ))
}

async fn probe_mcp_initialize(base_url: &str, token: &str) -> Result<(), String> {
    let client = http_client()?;
    let url = format!("{}/mcp", base_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": "webcodex-connect-probe",
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "webcodex-connect", "version": env!("CARGO_PKG_VERSION")}
            }
        }))
        .send()
        .await
        .map_err(|e| format!("MCP initialize probe failed: {e}"))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "MCP initialize probe returned HTTP {}",
            response.status()
        ))
    }
}

async fn wait_for_quick_tunnel_url(child: &mut ManagedChild, log: &Path) -> Result<String, String> {
    let started = Instant::now();
    while started.elapsed() < STARTUP_TIMEOUT {
        child.try_exit(log)?;
        if let Ok(content) = read_file_tail(log, 64 * 1024) {
            if let Some(url) = extract_trycloudflare_url(&content) {
                return Ok(url);
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(format!(
        "cloudflared did not publish a Quick Tunnel URL within {} seconds; inspect {}",
        STARTUP_TIMEOUT.as_secs(),
        log.display()
    ))
}

async fn wait_for_tunnel_health(
    child: &mut ManagedChild,
    health_file: &Path,
    log: &Path,
) -> Result<(), String> {
    let client = http_client()?;
    let started = Instant::now();
    while started.elapsed() < PUBLIC_INGRESS_TIMEOUT {
        child.try_exit(log)?;
        if let Ok(value) = std::fs::read_to_string(health_file) {
            let base = value.trim();
            if !base.is_empty() {
                let ready_url = if base.ends_with("/healthz") {
                    format!("{}/readyz", base.trim_end_matches("/healthz"))
                } else if base.ends_with("/readyz") {
                    base.to_string()
                } else {
                    format!("{}/readyz", base.trim_end_matches('/'))
                };
                if client
                    .get(ready_url)
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
                {
                    return Ok(());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    Err(format!(
        "OpenAI tunnel did not become ready within {} seconds; inspect {}",
        PUBLIC_INGRESS_TIMEOUT.as_secs(),
        log.display()
    ))
}

async fn wait_for_public_ingress(
    child: &mut ManagedChild,
    public_url: &str,
    token_file: &Path,
    target: HostedTarget,
    log: &Path,
) -> Result<(), String> {
    let client = http_client()?;
    let token = read_secret(token_file)?;
    let started = Instant::now();
    while started.elapsed() < PUBLIC_INGRESS_TIMEOUT {
        child.try_exit(log)?;
        let ready = if target.uses_mcp() {
            probe_mcp_initialize(public_url, &token).await.is_ok()
        } else {
            let schema_ready = client
                .get(format!("{}/openapi.json", public_url.trim_end_matches('/')))
                .send()
                .await
                .is_ok_and(|response| response.status().is_success());
            let action_ready = client
                .post(format!(
                    "{}/api/connector/task/review",
                    public_url.trim_end_matches('/')
                ))
                .bearer_auth(&token)
                .json(&serde_json::json!({
                    "task_id": "wc_task_00000000000000000000000000000000"
                }))
                .send()
                .await
                .is_ok_and(|response| response.status() == reqwest::StatusCode::NOT_FOUND);
            schema_ready && action_ready
        };
        if ready {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(format!(
        "public ingress did not pass the {} probe within {} seconds; inspect {} and verify the hostname routes to this command's loopback origin",
        if target.uses_mcp() { "MCP initialize" } else { "GPT Actions" },
        PUBLIC_INGRESS_TIMEOUT.as_secs(),
        log.display()
    ))
}

async fn supervise(
    server: &mut ManagedChild,
    agent: &mut ManagedChild,
    ingress: &mut ManagedChild,
    logs: &Path,
) -> Result<(), String> {
    enum End {
        Signal(Result<(), std::io::Error>),
        Server(Result<std::process::ExitStatus, std::io::Error>),
        Agent(Result<std::process::ExitStatus, std::io::Error>),
        Ingress(Result<std::process::ExitStatus, std::io::Error>),
    }
    let end = tokio::select! {
        result = tokio::signal::ctrl_c() => End::Signal(result),
        result = server.child.wait() => End::Server(result),
        result = agent.child.wait() => End::Agent(result),
        result = ingress.child.wait() => End::Ingress(result),
    };
    server.stop().await;
    agent.stop().await;
    ingress.stop().await;
    match end {
        End::Signal(Ok(())) => {
            println!("\nWebCodex connector stopped.");
            Ok(())
        }
        End::Signal(Err(e)) => Err(format!("failed to listen for Ctrl-C: {e}")),
        End::Server(status) => Err(format!(
            "local runtime exited unexpectedly ({status:?}); inspect {}",
            logs.join("server.log").display()
        )),
        End::Agent(status) => Err(format!(
            "project executor exited unexpectedly ({status:?}); inspect {}",
            logs.join("agent.log").display()
        )),
        End::Ingress(status) => Err(format!(
            "ingress exited unexpectedly ({status:?}); inspect provider log under {}",
            logs.display()
        )),
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("failed to build readiness HTTP client: {e}"))
}

fn ensure_bootstrap_token(path: &Path) -> Result<String, String> {
    if path.is_file() {
        return read_secret(path);
    }
    let token = format!(
        "wc_bootstrap_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    write_new_secret(path, format!("{token}\n").as_bytes())?;
    Ok(token)
}

fn read_secret(path: &Path) -> Result<String, String> {
    let value = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read private credential {}: {e}", path.display()))?;
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!("private credential {} is empty", path.display()));
    }
    Ok(value)
}

fn write_new_secret(path: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("failed to create private file {}: {e}", path.display()))?;
    file.write_all(content)
        .map_err(|e| format!("failed to write private file {}: {e}", path.display()))
}

fn write_private_file(path: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("failed to open private file {}: {e}", path.display()))?;
    file.write_all(content)
        .map_err(|e| format!("failed to write private file {}: {e}", path.display()))?;
    set_private_file_permissions(path)
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|e| format!("failed to create private directory {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(|e| {
            format!(
                "failed to set private directory permissions on {}: {e}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn set_private_file_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
            format!(
                "failed to set private file permissions on {}: {e}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn open_log(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("failed to open log {}: {e}", path.display()))?;
    set_private_file_permissions(path)?;
    Ok(file)
}

fn read_file_tail(path: &Path, max_bytes: u64) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let size = file
        .metadata()
        .map_err(|e| format!("failed to inspect {}: {e}", path.display()))?
        .len();
    if size > max_bytes {
        file.seek(SeekFrom::Start(size - max_bytes))
            .map_err(|e| format!("failed to seek {}: {e}", path.display()))?;
    }
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    Ok(content)
}

fn extract_trycloudflare_url(content: &str) -> Option<String> {
    let marker = "https://";
    let mut remainder = content;
    while let Some(index) = remainder.find(marker) {
        let candidate = &remainder[index..];
        let end = candidate
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, ':' | '/' | '.' | '-')))
            .unwrap_or(candidate.len());
        let value = candidate[..end].trim_end_matches('/');
        if value.ends_with(".trycloudflare.com") && Url::parse(value).is_ok() {
            return Some(value.to_string());
        }
        remainder = &candidate[marker.len()..];
    }
    None
}

fn discover_project_root(input: &Path) -> Result<PathBuf, String> {
    let canonical = input
        .canonicalize()
        .map_err(|e| format!("project path {} is not accessible: {e}", input.display()))?;
    if !canonical.is_dir() {
        return Err(format!(
            "project path is not a directory: {}",
            canonical.display()
        ));
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&canonical)
        .args(["rev-parse", "--show-toplevel"])
        .stderr(Stdio::null())
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !value.is_empty() {
                return PathBuf::from(value)
                    .canonicalize()
                    .map_err(|e| format!("discovered Git worktree root is not accessible: {e}"));
            }
        }
    }
    Ok(canonical)
}

fn project_identity(root: &Path) -> String {
    format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()))
}

fn safe_slug(root: &Path) -> String {
    let raw = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project");
    let slug = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '.'])
        .to_string();
    if slug.is_empty() {
        "project".to_string()
    } else {
        slug
    }
}

fn default_state_base() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("webcodex/hosted"));
    }
    Ok(home_dir()?.join(".local/state/webcodex/hosted"))
}

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set; pass --state-dir explicitly".to_string())
}

fn expand_home(path: &Path) -> Result<PathBuf, String> {
    let value = path.to_string_lossy();
    if value == "~" {
        return home_dir();
    }
    if let Some(suffix) = value.strip_prefix("~/") {
        return Ok(home_dir()?.join(suffix));
    }
    Ok(path.to_path_buf())
}

fn available_loopback_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("failed to allocate a loopback port: {e}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|e| format!("failed to inspect allocated loopback port: {e}"))
}

fn ensure_port_available(port: u16) -> Result<(), String> {
    TcpListener::bind(("127.0.0.1", port))
        .map(|_| ())
        .map_err(|e| format!("loopback port {port} is unavailable: {e}"))
}

fn nonempty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn strip_provider_credentials(command: &mut Command) {
    strip_openai_credentials(command);
    strip_cloudflare_credentials(command);
}

fn strip_openai_credentials(command: &mut Command) {
    for key in [
        "CONTROL_PLANE_API_KEY",
        "OPENAI_API_KEY",
        "OPENAI_ADMIN_KEY",
        "CONTROL_PLANE_CLIENT_KEY",
    ] {
        command.env_remove(key);
    }
}

fn strip_cloudflare_credentials(command: &mut Command) {
    for key in [
        "TUNNEL_TOKEN",
        "CLOUDFLARE_API_TOKEN",
        "CLOUDFLARE_API_KEY",
        "CF_API_TOKEN",
        "CF_API_KEY",
        "TUNNEL_ORIGIN_CERT",
    ] {
        command.env_remove(key);
    }
}

fn locate_binary(env_key: &str, name: &str, sibling_dir: &Path) -> Option<PathBuf> {
    if let Some(value) = std::env::var_os(env_key).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        return path.is_file().then_some(path);
    }
    let executable_name = executable_name(name);
    let sibling = sibling_dir.join(&executable_name);
    if sibling.is_file() {
        return Some(sibling);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(&executable_name))
        .find(|candidate| candidate.is_file())
}

fn executable_name(name: &str) -> OsString {
    #[cfg(windows)]
    {
        OsString::from(format!("{name}.exe"))
    }
    #[cfg(not(windows))]
    {
        OsString::from(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_openai_chatgpt_preset() {
        let parsed = parse(&args(&[
            "chatgpt",
            "--via",
            "openai",
            "--tunnel-id",
            "tunnel_0123456789abcdef0123456789abcdef",
            "--dry-run",
        ]))
        .unwrap();
        assert_eq!(parsed.target, HostedTarget::ChatGpt);
        assert_eq!(parsed.ingress, HostedIngress::OpenAi);
        assert!(parsed.dry_run);
    }

    #[test]
    fn rejects_protocol_ingress_mismatches() {
        let error = parse(&args(&[
            "gpt-actions",
            "--via",
            "openai",
            "--tunnel-id",
            "tunnel_0123456789abcdef0123456789abcdef",
        ]))
        .unwrap_err();
        assert!(error.contains("not GPT Actions"));

        let error = parse(&args(&["mcp", "--via", "cloudflare", "--temporary"])).unwrap_err();
        assert!(error.contains("does not support SSE"));
    }

    #[test]
    fn named_cloudflare_requires_stable_https_url() {
        let error = parse(&args(&["mcp", "--via", "cloudflare"])).unwrap_err();
        assert!(error.contains("--public-url"));
        let error = parse(&args(&[
            "mcp",
            "--via",
            "cloudflare",
            "--public-url",
            "http://example.test",
        ]))
        .unwrap_err();
        assert!(error.contains("https"));
    }

    #[test]
    fn extracts_only_trycloudflare_https_url() {
        let log = "other https://example.com then https://quiet-frog.trycloudflare.com | ready";
        assert_eq!(
            extract_trycloudflare_url(log).as_deref(),
            Some("https://quiet-frog.trycloudflare.com")
        );
        assert!(extract_trycloudflare_url("https://example.com").is_none());
    }

    #[test]
    fn project_identity_is_stable_and_slug_is_safe() {
        let root = Path::new("/tmp/A Project");
        assert_eq!(project_identity(root), project_identity(root));
        assert_eq!(project_identity(root).len(), 64);
        assert_eq!(safe_slug(root), "a-project");
    }

    #[test]
    fn project_registration_uses_agent_project_shape() {
        let project = ManagedProjectFile {
            id: "demo",
            path: "/tmp/demo".to_string(),
            name: "Demo".to_string(),
            kind: "auto",
            allow_patch: true,
            disabled: false,
        };
        let value: toml::Value = toml::from_str(&toml::to_string(&project).unwrap()).unwrap();
        assert_eq!(value["id"].as_str(), Some("demo"));
        assert_eq!(value["path"].as_str(), Some("/tmp/demo"));
        assert!(value.get("projects").is_none());
    }

    #[test]
    fn openai_header_uses_private_file_reference_not_token_value() {
        let path = Path::new("/private/mcp-authorization");
        let header = format!("Authorization: file:{}", path.display());
        assert_eq!(header, "Authorization: file:/private/mcp-authorization");
        assert!(!header.contains("Bearer "));
    }
}

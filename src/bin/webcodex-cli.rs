//! `webcodex-cli` — standalone management/setup binary for WebCodex.
//!
//! Provides users / tokens / agent-tokens management (reusing the
//! shared `admin_cli` module), `agent init` (reusing the shared `agent_init`
//! module), and a first-pass `setup single-user` command that creates a user,
//! a personal API token, and an agent token, then writes the plaintext tokens
//! to 0600 files.
//!
//! This binary intentionally does NOT start a server and does NOT print real
//! tokens, Authorization headers, or full agent.toml contents with secrets
//! (except explicit stdout materialization paths such as `agent init --output -`
//! and `server init --output -`, which the user requests deliberately).
//!
//! The existing `webcodex` server binary keeps its `webcodex users/tokens/...`
//! admin commands as compatibility wrappers; this binary is the new home for
//! management tooling.

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[allow(dead_code)]
#[path = "../shell_protocol.rs"]
mod shell_protocol;

#[allow(dead_code)]
#[path = "../admin_cli.rs"]
mod admin_cli;

#[allow(dead_code)]
#[path = "../agent_init.rs"]
mod agent_init;

#[path = "../build_info.rs"]
mod build_info;

mod webcodex_cli;

#[cfg(test)]
use admin_cli::build_admin_request;
use admin_cli::{parse_admin_cli, run_admin_command, AdminCliCommand, AdminOptions};
use agent_init::{
    run_agent_init, AgentInitOptions, DEFAULT_INIT_PROJECTS_DIR, DEFAULT_POLL_INTERVAL_MS,
    TRANSPORT_WEBSOCKET,
};
#[cfg(test)]
use webcodex_cli::parse_env_content_value;
#[cfg(test)]
use webcodex_cli::token_prefix;
use webcodex_cli::{
    agent_init_usage, agent_install_service_usage, agent_status_usage, agent_usage,
    client_enroll_usage, client_profile_agent_config, client_profile_agent_token_file,
    client_profile_projects_dir, client_profile_service_file, client_profile_user_token_file,
    client_usage, compare_build_commits, connect_usage, default_client_output_dir_for_profile,
    default_server_paths, doctor_usage, fetch_runtime_status, http_post_json_status,
    local_cli_build_metadata, pairing_create_usage, pairing_usage, read_env_file_value,
    render_build_metadata_block, render_token_generate, run_agent_install_service,
    run_agent_status, run_agent_token_create_local, run_client_enroll, run_connect, run_doctor,
    run_pairing_create, run_server_init, run_server_install_service, run_server_up,
    run_setup_single_user, run_token_create_local, runtime_build_metadata, server_init_usage,
    server_install_service_usage, server_status_revision_check, server_status_usage,
    server_up_usage, server_usage, usage, validate_client_profile, DoctorCheck,
};
#[cfg(test)]
use webcodex_cli::{
    client_output_dir_for_profile, doctor_revision_check, ensure_enroll_outputs_available,
    format_error_body, is_effective_root, render_agent_systemd_unit, resolve_account_credential,
    resolve_pairing_create_token, RevisionComparison, RuntimeBuildMetadata, CLIENT_PROFILE_ERROR,
};

const SETUP_GPT_SCOPES: &[&str] = &["runtime:read", "project:read", "project:write", "job:run"];
const SETUP_AGENT_SCOPES: &[&str] = &[
    "agent:register",
    "agent:poll",
    "agent:result",
    "agent:job_update",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliAction {
    Admin(AdminCliCommand),
    TokenGenerate(TokenGenerateOptions),
    TokenCreateLocal(TokenCreateLocalOptions),
    AgentTokenCreateLocal(AgentTokenCreateLocalOptions),
    AgentInit(AgentInitOptions),
    SetupSingleUser(SetupSingleUserOptions),
    PairingCreate(PairingCreateOptions),
    ClientEnroll(ClientEnrollOptions),
    Doctor(DoctorOptions),
    AgentInstallService(AgentInstallServiceOptions),
    AgentStatus(AgentStatusOptions),
    ServerInit(ServerInitOptions),
    ServerInstallService(ServerInstallServiceOptions),
    ServerStatus(ServerStatusOptions),
    ServerUp(ServerUpOptions),
    Connect(ConnectOptions),
    Exit {
        code: i32,
        stdout: String,
        stderr: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenGenerateOptions {
    kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct TokenCreateLocalOptions {
    server_url: String,
    username: String,
    credential: Option<String>,
    credential_env: Option<String>,
    name: Option<String>,
    scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct AgentTokenCreateLocalOptions {
    admin: AdminOptions,
    username: String,
    client_id: String,
    name: Option<String>,
    scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct SetupSingleUserOptions {
    server_url: String,
    token: Option<String>,
    token_file: Option<PathBuf>,
    username: String,
    client_id: String,
    display_name: Option<String>,
    role: String,
    gpt_token_name: String,
    agent_token_name: String,
    output_dir: PathBuf,
    force_create_tokens: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct PairingCreateOptions {
    server_url: String,
    env_file: Option<PathBuf>,
    token: Option<String>,
    token_file: Option<PathBuf>,
    username: String,
    client_id: String,
    display_name: Option<String>,
    ttl_secs: i64,
    user_token_name: Option<String>,
    agent_token_name: Option<String>,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClientEnrollOptions {
    server_url: String,
    pairing_code: String,
    client_id: String,
    display_name: Option<String>,
    transport: String,
    output_dir: PathBuf,
    agent_config: PathBuf,
    projects_dir: PathBuf,
    allowed_roots: Vec<PathBuf>,
    allow_cwd_anywhere: bool,
    overwrite: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct DoctorOptions {
    server_url: Option<String>,
    env_file: Option<PathBuf>,
    token_file: Option<PathBuf>,
    user_token_file: Option<PathBuf>,
    agent_token_file: Option<PathBuf>,
    /// Local agent config path (`agent.toml`) for local shell-profile / project
    /// diagnostics. When set, doctor parses it and checks projects_dir, project
    /// paths, and shell_profile resolution without contacting the server.
    agent_config: Option<PathBuf>,
    /// Restrict the remote shell roundtrip check to a single project id (the
    /// `agent:<client_id>:<project_id>` runtime id, or a bare project id).
    project: Option<String>,
    /// Run QUIC transport diagnostics. By default this performs server-only
    /// QUIC DNS/TLS/ALPN handshake checks; combine with --agent-e2e for
    /// runtime dispatch checks against an already-running QUIC agent.
    quic: bool,
    quic_server_addr: Option<String>,
    quic_server_name: Option<String>,
    quic_alpn: String,
    quic_timeout_secs: u64,
    quic_server_only: bool,
    quic_agent_e2e: bool,
    quic_client_id: Option<String>,
    json: bool,
    strict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerInitOptions {
    listen: String,
    data_dir: PathBuf,
    env_file: PathBuf,
    public_url: Option<String>,
    overwrite: bool,
    output_stdout: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerInstallServiceOptions {
    env_file: PathBuf,
    bin: PathBuf,
    service_file: PathBuf,
    user: Option<String>,
    group: Option<String>,
    working_directory: PathBuf,
    overwrite: bool,
    dry_run: bool,
    output_stdout: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentInstallServiceOptions {
    config: PathBuf,
    bin: PathBuf,
    service_file: PathBuf,
    user: Option<String>,
    group: Option<String>,
    working_directory: PathBuf,
    overwrite: bool,
    dry_run: bool,
    output_stdout: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerStatusOptions {
    url: String,
    env_file: Option<PathBuf>,
    env_file_explicit: bool,
    token_file: Option<PathBuf>,
    json: bool,
}

/// Quick-start connection mode selected by the caller. `--key` and `--open` are
/// mutually exclusive at the CLI layer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ConnectMode {
    /// Shared-key pairing: client and GPT/MCP use the same arbitrary key.
    SharedKey(String),
    /// Anonymous pairing: requires the server to be started with `--open`.
    Open,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectOptions {
    server_url: String,
    mode: ConnectMode,
    root: PathBuf,
    output_dir: Option<PathBuf>,
    client_id: Option<String>,
    overwrite: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerUpOptions {
    public_url: Option<String>,
    listen: Option<String>,
    open: bool,
    data_dir: Option<PathBuf>,
    env_file: Option<PathBuf>,
    foreground: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentStatusOptions {
    config: PathBuf,
    server_url: Option<String>,
    user_token_file: Option<PathBuf>,
    agent_token_file: Option<PathBuf>,
    json: bool,
}

fn cli_action<I, S>(args: I) -> CliAction
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_string()).collect();
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("{}\n", usage()),
        };
    }
    match args[0].as_str() {
        "--help" | "-h" => CliAction::Exit {
            code: 0,
            stdout: usage().to_string(),
            stderr: String::new(),
        },
        "--version" | "-V" => CliAction::Exit {
            code: 0,
            stdout: build_info::version_output("webcodex-cli"),
            stderr: String::new(),
        },
        "connect" => parse_connect_command(&args[1..]),
        "server" => parse_server_subcommand(&args[1..]),
        "pairing" => parse_pairing_subcommand(&args[1..]),
        "client" => parse_client_subcommand(&args[1..]),
        "doctor" => parse_doctor_command(&args[1..]),
        "agent" => parse_agent_subcommand(&args[1..]),
        "agent-token" | "agent-tokens" => {
            parse_agent_token_subcommand(args[0].as_str(), &args[1..])
        }
        "setup" => parse_setup_subcommand(&args[1..]),
        "token" => parse_token_subcommand(&args[1..]),
        _ => {
            // users / tokens / agent-tokens management: reuse admin_cli parser.
            match parse_admin_cli(&args) {
                Ok(cmd) => CliAction::Admin(cmd),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
    }
}

fn parse_token_subcommand(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: "missing token subcommand\n".to_string(),
        };
    }
    match args[0].as_str() {
        "generate" => match parse_token_generate(&args[1..]) {
            Ok(opts) => CliAction::TokenGenerate(opts),
            Err(e) => CliAction::Exit {
                code: 2,
                stdout: String::new(),
                stderr: format!("{}\n", e),
            },
        },
        "create-local" => match parse_token_create_local(&args[1..]) {
            Ok(opts) => CliAction::TokenCreateLocal(opts),
            Err(e) => CliAction::Exit {
                code: 2,
                stdout: String::new(),
                stderr: format!("{}\n", e),
            },
        },
        _ => {
            let mut forwarded = vec!["token".to_string()];
            forwarded.extend_from_slice(args);
            match parse_admin_cli(&forwarded) {
                Ok(cmd) => CliAction::Admin(cmd),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
    }
}

fn parse_token_generate(args: &[String]) -> Result<TokenGenerateOptions, String> {
    let mut kind = "api".to_string();
    let mut p = SimpleFlagParser::new(args);
    while let Some(flag) = p.next() {
        match flag.as_str() {
            "--kind" => kind = p.value(&flag)?,
            "-h" | "--help" => {
                return Err("Usage: webcodex-cli token generate --kind api|agent".to_string())
            }
            _ => return Err(format!("unknown token generate flag: {}", flag)),
        }
    }
    if kind != "api" && kind != "agent" {
        return Err("--kind must be 'api' or 'agent'".to_string());
    }
    Ok(TokenGenerateOptions { kind })
}

fn parse_agent_token_subcommand(group: &str, args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: "missing agent-token subcommand\n".to_string(),
        };
    }
    match args[0].as_str() {
        "create-local" => match parse_agent_token_create_local(&args[1..]) {
            Ok(opts) => CliAction::AgentTokenCreateLocal(opts),
            Err(e) => CliAction::Exit {
                code: 2,
                stdout: String::new(),
                stderr: format!("{}\n", e),
            },
        },
        _ => {
            let mut forwarded = vec![group.to_string()];
            forwarded.extend_from_slice(args);
            match parse_admin_cli(&forwarded) {
                Ok(cmd) => CliAction::Admin(cmd),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
    }
}

fn parse_agent_token_create_local(args: &[String]) -> Result<AgentTokenCreateLocalOptions, String> {
    let mut opts = AgentTokenCreateLocalOptions::default();
    let mut p = SimpleFlagParser::new(args);
    while let Some(flag) = p.next() {
        match flag.as_str() {
            "--server" | "--server-url" => opts.admin.server_url = p.value(&flag)?,
            "--user" | "--username" => opts.username = p.value(&flag)?,
            "--client-id" => opts.client_id = p.value(&flag)?,
            "--credential" => opts.admin.credential = Some(p.value(&flag)?),
            "--credential-env" => opts.admin.credential_env = Some(p.value(&flag)?),
            "--token" | "--admin-token" => opts.admin.token = Some(p.value(&flag)?),
            "--token-env" | "--admin-token-env" => opts.admin.token_env = Some(p.value(&flag)?),
            "--token-file" => opts.admin.token_file = Some(PathBuf::from(p.value(&flag)?)),
            "--name" => opts.name = Some(p.value(&flag)?),
            "--scope" => opts.scopes.push(p.value(&flag)?),
            "--scopes" => {
                opts.scopes.extend(
                    p.value(&flag)?
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                );
            }
            "-h" | "--help" => return Err("Usage: webcodex-cli agent-token create-local --server URL --user USER --credential CRED --client-id ID [--name NAME] [--scopes S1,S2]".to_string()),
            _ => return Err(format!("unknown agent-token create-local flag: {}", flag)),
        }
    }
    if opts.admin.server_url.trim().is_empty() {
        return Err("--server is required".to_string());
    }
    if opts.username.trim().is_empty() {
        return Err("--user is required".to_string());
    }
    if opts.client_id.trim().is_empty() {
        return Err("--client-id is required".to_string());
    }
    if opts.scopes.is_empty() {
        opts.scopes = SETUP_AGENT_SCOPES.iter().map(|s| s.to_string()).collect();
    }
    Ok(opts)
}

fn parse_token_create_local(args: &[String]) -> Result<TokenCreateLocalOptions, String> {
    let mut opts = TokenCreateLocalOptions::default();
    let mut p = SimpleFlagParser::new(args);
    while let Some(flag) = p.next() {
        match flag.as_str() {
            "--server" | "--server-url" => opts.server_url = p.value(&flag)?,
            "--user" | "--username" => opts.username = p.value(&flag)?,
            "--credential" => opts.credential = Some(p.value(&flag)?),
            "--credential-env" => opts.credential_env = Some(p.value(&flag)?),
            "--name" => opts.name = Some(p.value(&flag)?),
            "--scope" => opts.scopes.push(p.value(&flag)?),
            "--scopes" => {
                opts.scopes.extend(
                    p.value(&flag)?
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                );
            }
            "-h" | "--help" => {
                return Err("Usage: webcodex-cli token create-local --server URL --user USER --credential CRED [--name NAME] [--scopes S1,S2]".to_string())
            }
            _ => return Err(format!("unknown token create-local flag: {}", flag)),
        }
    }
    if opts.server_url.trim().is_empty() {
        return Err("--server is required".to_string());
    }
    if opts.username.trim().is_empty() {
        return Err("--user is required".to_string());
    }
    if opts.scopes.is_empty() {
        opts.scopes = SETUP_GPT_SCOPES.iter().map(|s| s.to_string()).collect();
    }
    Ok(opts)
}

#[derive(Debug)]
struct SimpleFlagParser {
    args: Vec<String>,
    idx: usize,
}

impl SimpleFlagParser {
    fn new(args: &[String]) -> Self {
        Self {
            args: args.to_vec(),
            idx: 0,
        }
    }

    fn next(&mut self) -> Option<String> {
        let value = self.args.get(self.idx).cloned();
        if value.is_some() {
            self.idx += 1;
        }
        value
    }

    fn value(&mut self, flag: &str) -> Result<String, String> {
        self.next()
            .ok_or_else(|| format!("{} requires a value", flag))
    }
}

fn parse_agent_subcommand(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("{}\n", agent_usage()),
        };
    }
    match args[0].as_str() {
        "init" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                return CliAction::Exit {
                    code: 0,
                    stdout: agent_init_usage().to_string(),
                    stderr: String::new(),
                };
            }
            match parse_cli_agent_init(&args[1..]) {
                Ok(opts) => CliAction::AgentInit(opts),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
        "install-service" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                return CliAction::Exit {
                    code: 0,
                    stdout: agent_install_service_usage().to_string(),
                    stderr: String::new(),
                };
            }
            match parse_agent_install_service(&args[1..]) {
                Ok(opts) => CliAction::AgentInstallService(opts),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
        "status" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                return CliAction::Exit {
                    code: 0,
                    stdout: agent_status_usage().to_string(),
                    stderr: String::new(),
                };
            }
            match parse_agent_status(&args[1..]) {
                Ok(opts) => CliAction::AgentStatus(opts),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
        "--help" | "-h" => CliAction::Exit {
            code: 0,
            stdout: agent_usage().to_string(),
            stderr: String::new(),
        },
        other => CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("unknown agent subcommand: {}\n", other),
        },
    }
}

fn parse_pairing_subcommand(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("{}\n", pairing_usage()),
        };
    }
    match args[0].as_str() {
        "--help" | "-h" => CliAction::Exit {
            code: 0,
            stdout: pairing_usage().to_string(),
            stderr: String::new(),
        },
        "create" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                return CliAction::Exit {
                    code: 0,
                    stdout: pairing_create_usage().to_string(),
                    stderr: String::new(),
                };
            }
            match parse_pairing_create(&args[1..]) {
                Ok(opts) => CliAction::PairingCreate(opts),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
        other => CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("unknown pairing subcommand: {}\n", other),
        },
    }
}

fn parse_client_subcommand(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("{}\n", client_usage()),
        };
    }
    match args[0].as_str() {
        "--help" | "-h" => CliAction::Exit {
            code: 0,
            stdout: client_usage().to_string(),
            stderr: String::new(),
        },
        "enroll" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                return CliAction::Exit {
                    code: 0,
                    stdout: client_enroll_usage().to_string(),
                    stderr: String::new(),
                };
            }
            match parse_client_enroll(&args[1..]) {
                Ok(opts) => CliAction::ClientEnroll(opts),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
        other => CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("unknown client subcommand: {}\n", other),
        },
    }
}

fn parse_server_subcommand(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("{}\n", server_usage()),
        };
    }
    match args[0].as_str() {
        "--help" | "-h" => CliAction::Exit {
            code: 0,
            stdout: server_usage().to_string(),
            stderr: String::new(),
        },
        "init" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                return CliAction::Exit {
                    code: 0,
                    stdout: server_init_usage().to_string(),
                    stderr: String::new(),
                };
            }
            match parse_server_init(&args[1..]) {
                Ok(opts) => CliAction::ServerInit(opts),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
        "install-service" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                return CliAction::Exit {
                    code: 0,
                    stdout: server_install_service_usage().to_string(),
                    stderr: String::new(),
                };
            }
            match parse_server_install_service(&args[1..]) {
                Ok(opts) => CliAction::ServerInstallService(opts),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
        "status" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                return CliAction::Exit {
                    code: 0,
                    stdout: server_status_usage().to_string(),
                    stderr: String::new(),
                };
            }
            match parse_server_status(&args[1..]) {
                Ok(opts) => CliAction::ServerStatus(opts),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
        "up" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                return CliAction::Exit {
                    code: 0,
                    stdout: server_up_usage().to_string(),
                    stderr: String::new(),
                };
            }
            match parse_server_up(&args[1..]) {
                Ok(opts) => CliAction::ServerUp(opts),
                Err(e) => CliAction::Exit {
                    code: 2,
                    stdout: String::new(),
                    stderr: format!("{}\n", e),
                },
            }
        }
        other => CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("unknown server subcommand: {}\n", other),
        },
    }
}

fn parse_server_init(args: &[String]) -> Result<ServerInitOptions, String> {
    let defaults = default_server_paths();
    let mut opts = ServerInitOptions {
        listen: "127.0.0.1:8080".to_string(),
        data_dir: defaults.data_dir,
        env_file: defaults.env_file,
        public_url: None,
        overwrite: false,
        output_stdout: false,
        json: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--listen" => opts.listen = next_value(&mut iter, arg)?,
            "--data-dir" => opts.data_dir = PathBuf::from(next_value(&mut iter, arg)?),
            "--env-file" => opts.env_file = PathBuf::from(next_value(&mut iter, arg)?),
            "--public-url" => opts.public_url = Some(next_value(&mut iter, arg)?),
            "--overwrite" => opts.overwrite = true,
            "--output" => {
                let value = next_value(&mut iter, arg)?;
                if value != "-" {
                    return Err("--output only supports '-' for stdout".to_string());
                }
                opts.output_stdout = true;
            }
            "--json" => opts.json = true,
            _ => return Err(format!("unknown server init flag: {}", arg)),
        }
    }
    if opts.listen.trim().is_empty() {
        return Err("--listen cannot be empty".to_string());
    }
    if opts.data_dir.as_os_str().is_empty() {
        return Err("--data-dir cannot be empty".to_string());
    }
    if opts.env_file.as_os_str().is_empty() {
        return Err("--env-file cannot be empty".to_string());
    }
    if let Some(url) = &opts.public_url {
        if url.trim().is_empty() {
            return Err("--public-url cannot be empty".to_string());
        }
    }
    Ok(opts)
}

fn parse_connect_command(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("{}\n", connect_usage()),
        };
    }
    match args[0].as_str() {
        "--help" | "-h" => CliAction::Exit {
            code: 0,
            stdout: connect_usage().to_string(),
            stderr: String::new(),
        },
        _ => match parse_connect(&args) {
            Ok(opts) => CliAction::Connect(opts),
            Err(e) => CliAction::Exit {
                code: 2,
                stdout: String::new(),
                stderr: format!("{}\n", e),
            },
        },
    }
}

fn parse_connect(args: &[String]) -> Result<ConnectOptions, String> {
    let mut server_url: Option<String> = None;
    let mut key: Option<String> = None;
    let mut open = false;
    let mut root: Option<PathBuf> = None;
    let mut output_dir: Option<PathBuf> = None;
    let mut client_id: Option<String> = None;
    let mut overwrite = false;
    let mut json = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--key" => key = Some(next_value(&mut iter, arg)?),
            "--open" => open = true,
            "--root" => root = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--output-dir" => output_dir = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--client-id" => client_id = Some(next_value(&mut iter, arg)?),
            "--overwrite" => overwrite = true,
            "--json" => json = true,
            "-h" | "--help" => return Err(connect_usage().to_string()),
            other if !other.starts_with("--") && server_url.is_none() => {
                server_url = Some(other.to_string());
            }
            other => return Err(format!("unknown connect flag: {}", other)),
        }
    }
    // Mutual exclusion: --key and --open cannot be combined.
    if open && key.is_some() {
        return Err("--key and --open are mutually exclusive.\n\
             Use --key for shared-key pairing, or --open for anonymous pairing."
            .to_string());
    }
    // At least one of --key / --open is required.
    if !open && key.is_none() {
        return Err("either --key or --open is required.\n\
             Use --key <KEY> for shared-key pairing, or --open for anonymous pairing."
            .to_string());
    }
    let server_url = server_url.ok_or_else(|| {
        "server URL is required.\n\
         Usage: webcodex-cli connect <SERVER-URL> --key <KEY> | --open"
            .to_string()
    })?;
    if server_url.trim().is_empty() {
        return Err("server URL cannot be empty".to_string());
    }
    let key_value = key.clone().unwrap_or_default();
    if key.is_some() && key_value.trim().is_empty() {
        return Err("--key cannot be empty".to_string());
    }
    let mode = if open {
        ConnectMode::Open
    } else {
        ConnectMode::SharedKey(key_value)
    };
    let root =
        root.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    Ok(ConnectOptions {
        server_url,
        mode,
        root,
        output_dir,
        client_id,
        overwrite,
        json,
    })
}

fn parse_server_up(args: &[String]) -> Result<ServerUpOptions, String> {
    let mut opts = ServerUpOptions {
        public_url: None,
        listen: None,
        open: false,
        data_dir: None,
        env_file: None,
        foreground: false,
        json: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--public-url" => opts.public_url = Some(next_value(&mut iter, arg)?),
            "--listen" => opts.listen = Some(next_value(&mut iter, arg)?),
            "--open" => opts.open = true,
            "--data-dir" => opts.data_dir = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--env-file" => opts.env_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--foreground" => {
                return Err(
                    "--foreground is not implemented yet; load the printed env command and run webcodex"
                        .to_string(),
                )
            }
            "--json" => opts.json = true,
            "-h" | "--help" => return Err(server_up_usage().to_string()),
            other => return Err(format!("unknown server up flag: {}", other)),
        }
    }
    if let Some(url) = &opts.public_url {
        if url.trim().is_empty() {
            return Err("--public-url cannot be empty".to_string());
        }
    }
    Ok(opts)
}

fn parse_agent_install_service(args: &[String]) -> Result<AgentInstallServiceOptions, String> {
    let mut profile: Option<String> = None;
    let mut config: Option<PathBuf> = None;
    let mut bin: Option<PathBuf> = None;
    let mut service_file: Option<PathBuf> = None;
    let mut working_directory = PathBuf::from("/root");
    let mut user = None;
    let mut group = None;
    let mut overwrite = false;
    let mut dry_run = false;
    let mut output_stdout = false;
    let mut json = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--profile" => profile = Some(next_value(&mut iter, arg)?),
            "--config" => config = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--bin" => bin = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--service-file" => service_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--working-directory" => working_directory = PathBuf::from(next_value(&mut iter, arg)?),
            "--user" => user = Some(next_value(&mut iter, arg)?),
            "--group" => group = Some(next_value(&mut iter, arg)?),
            "--overwrite" => overwrite = true,
            "--dry-run" => dry_run = true,
            "--output" => {
                let value = next_value(&mut iter, arg)?;
                if value != "-" {
                    return Err("--output only supports '-' for stdout".to_string());
                }
                output_stdout = true;
            }
            "--json" => json = true,
            _ => return Err(format!("unknown agent install-service flag: {}", arg)),
        }
    }
    let profile = profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?;
    let config = config.unwrap_or_else(|| {
        profile
            .as_deref()
            .map(client_profile_agent_config)
            .unwrap_or_else(|| PathBuf::from("/etc/webcodex/agent.toml"))
    });
    let service_file = service_file.unwrap_or_else(|| {
        profile
            .as_deref()
            .map(client_profile_service_file)
            .unwrap_or_else(|| PathBuf::from("/etc/systemd/system/webcodex-agent.service"))
    });
    let bin = match bin.or_else(|| discover_named_binary_absolute("webcodex-agent")) {
        Some(path) => path,
        None => {
            return Err(
                "--bin is required because webcodex-agent was not found in PATH".to_string(),
            )
        }
    };
    if config.as_os_str().is_empty() {
        return Err("--config cannot be empty".to_string());
    }
    if bin.as_os_str().is_empty() {
        return Err("--bin cannot be empty".to_string());
    }
    if service_file.as_os_str().is_empty() {
        return Err("--service-file cannot be empty".to_string());
    }
    if working_directory.as_os_str().is_empty() {
        return Err("--working-directory cannot be empty".to_string());
    }
    Ok(AgentInstallServiceOptions {
        config,
        bin,
        service_file,
        user,
        group,
        working_directory,
        overwrite,
        dry_run,
        output_stdout,
        json,
    })
}

fn parse_agent_status(args: &[String]) -> Result<AgentStatusOptions, String> {
    let mut profile: Option<String> = None;
    let mut config: Option<PathBuf> = None;
    let mut opts = AgentStatusOptions {
        config: PathBuf::new(),
        server_url: None,
        user_token_file: None,
        agent_token_file: None,
        json: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--profile" => profile = Some(next_value(&mut iter, arg)?),
            "--config" => config = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--server-url" => opts.server_url = Some(next_value(&mut iter, arg)?),
            "--user-token-file" => {
                opts.user_token_file = Some(PathBuf::from(next_value(&mut iter, arg)?))
            }
            "--agent-token-file" => {
                opts.agent_token_file = Some(PathBuf::from(next_value(&mut iter, arg)?))
            }
            "--json" => opts.json = true,
            _ => return Err(format!("unknown agent status flag: {}", arg)),
        }
    }
    if let Some(profile) = profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?
    {
        opts.config = config.unwrap_or_else(|| client_profile_agent_config(&profile));
        opts.user_token_file = opts
            .user_token_file
            .or_else(|| Some(client_profile_user_token_file(&profile)));
        opts.agent_token_file = opts
            .agent_token_file
            .or_else(|| Some(client_profile_agent_token_file(&profile)));
    } else {
        opts.config = config.unwrap_or_else(|| PathBuf::from("/etc/webcodex/agent.toml"));
    }
    if opts.config.as_os_str().is_empty() {
        return Err("--config cannot be empty".to_string());
    }
    if opts
        .server_url
        .as_ref()
        .is_some_and(|url| url.trim().is_empty())
    {
        return Err("--server-url cannot be empty".to_string());
    }
    Ok(opts)
}

fn parse_server_install_service(args: &[String]) -> Result<ServerInstallServiceOptions, String> {
    let mut env_file = PathBuf::from("/etc/webcodex/webcodex.env");
    let mut bin: Option<PathBuf> = None;
    let mut service_file = PathBuf::from("/etc/systemd/system/webcodex.service");
    let mut user = None;
    let mut group = None;
    let mut working_directory = PathBuf::from("/var/lib/webcodex");
    let mut overwrite = false;
    let mut dry_run = false;
    let mut output_stdout = false;
    let mut json = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--env-file" => env_file = PathBuf::from(next_value(&mut iter, arg)?),
            "--bin" => bin = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--service-file" => service_file = PathBuf::from(next_value(&mut iter, arg)?),
            "--user" => user = Some(next_value(&mut iter, arg)?),
            "--group" => group = Some(next_value(&mut iter, arg)?),
            "--working-directory" => working_directory = PathBuf::from(next_value(&mut iter, arg)?),
            "--overwrite" => overwrite = true,
            "--dry-run" => dry_run = true,
            "--output" => {
                let value = next_value(&mut iter, arg)?;
                if value != "-" {
                    return Err("--output only supports '-' for stdout".to_string());
                }
                output_stdout = true;
            }
            "--json" => json = true,
            _ => return Err(format!("unknown server install-service flag: {}", arg)),
        }
    }
    let bin = match bin.or_else(discover_webcodex_binary) {
        Some(path) => path,
        None => return Err("--bin is required because webcodex was not found in PATH".to_string()),
    };
    if env_file.as_os_str().is_empty() {
        return Err("--env-file cannot be empty".to_string());
    }
    if bin.as_os_str().is_empty() {
        return Err("--bin cannot be empty".to_string());
    }
    if service_file.as_os_str().is_empty() {
        return Err("--service-file cannot be empty".to_string());
    }
    if working_directory.as_os_str().is_empty() {
        return Err("--working-directory cannot be empty".to_string());
    }
    Ok(ServerInstallServiceOptions {
        env_file,
        bin,
        service_file,
        user,
        group,
        working_directory,
        overwrite,
        dry_run,
        output_stdout,
        json,
    })
}

fn parse_server_status(args: &[String]) -> Result<ServerStatusOptions, String> {
    let mut opts = ServerStatusOptions {
        url: "http://127.0.0.1:8080".to_string(),
        env_file: Some(default_server_paths().env_file),
        env_file_explicit: false,
        token_file: None,
        json: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--url" => opts.url = next_value(&mut iter, arg)?,
            "--env-file" => {
                opts.env_file = Some(PathBuf::from(next_value(&mut iter, arg)?));
                opts.env_file_explicit = true;
            }
            "--token-file" => opts.token_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--json" => opts.json = true,
            _ => return Err(format!("unknown server status flag: {}", arg)),
        }
    }
    if opts.url.trim().is_empty() {
        return Err("--url cannot be empty".to_string());
    }
    Ok(opts)
}

fn parse_pairing_create(args: &[String]) -> Result<PairingCreateOptions, String> {
    let mut opts = PairingCreateOptions {
        ttl_secs: 600,
        ..PairingCreateOptions::default()
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--server-url" => opts.server_url = next_value(&mut iter, arg)?,
            "--env-file" => opts.env_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--token" => opts.token = Some(next_value(&mut iter, arg)?),
            "--token-file" => opts.token_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--username" => opts.username = next_value(&mut iter, arg)?,
            "--client-id" => opts.client_id = next_value(&mut iter, arg)?,
            "--display-name" => opts.display_name = Some(next_value(&mut iter, arg)?),
            "--ttl-secs" => {
                opts.ttl_secs = next_value(&mut iter, arg)?
                    .parse::<i64>()
                    .map_err(|_| "--ttl-secs must be an integer".to_string())?;
            }
            "--user-token-name" => opts.user_token_name = Some(next_value(&mut iter, arg)?),
            "--agent-token-name" => opts.agent_token_name = Some(next_value(&mut iter, arg)?),
            "--json" => opts.json = true,
            _ => return Err(format!("unknown pairing create flag: {}", arg)),
        }
    }
    if opts.server_url.trim().is_empty() {
        return Err("--server-url is required".to_string());
    }
    if opts.username.trim().is_empty() {
        return Err("--username is required".to_string());
    }
    if opts.client_id.trim().is_empty() {
        return Err("--client-id is required".to_string());
    }
    if !(60..=3600).contains(&opts.ttl_secs) {
        return Err("--ttl-secs must be between 60 and 3600".to_string());
    }
    let auth_sources = opts.token.is_some() as u8
        + opts.token_file.is_some() as u8
        + opts.env_file.is_some() as u8;
    if auth_sources > 1 {
        return Err("use only one of --token, --token-file, or --env-file".to_string());
    }
    Ok(opts)
}

fn parse_client_enroll(args: &[String]) -> Result<ClientEnrollOptions, String> {
    let mut server_url = String::new();
    let mut pairing_code = String::new();
    let mut client_id = String::new();
    let mut display_name = None;
    let mut transport = TRANSPORT_WEBSOCKET.to_string();
    let mut profile: Option<String> = None;
    let mut output_dir: Option<PathBuf> = None;
    let mut agent_config: Option<PathBuf> = None;
    let mut projects_dir: Option<PathBuf> = None;
    let mut allowed_roots = Vec::new();
    let mut allow_cwd_anywhere = false;
    let mut overwrite = false;
    let mut json = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--server-url" => server_url = next_value(&mut iter, arg)?,
            "--pairing-code" => pairing_code = next_value(&mut iter, arg)?,
            "--client-id" => client_id = next_value(&mut iter, arg)?,
            "--display-name" => display_name = Some(next_value(&mut iter, arg)?),
            "--transport" => transport = next_value(&mut iter, arg)?,
            "--profile" => profile = Some(next_value(&mut iter, arg)?),
            "--output-dir" => output_dir = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--agent-config" => agent_config = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--projects-dir" => projects_dir = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--allowed-root" => allowed_roots.push(PathBuf::from(next_value(&mut iter, arg)?)),
            "--allow-cwd-anywhere" => {
                allow_cwd_anywhere = agent_init::parse_bool(&next_value(&mut iter, arg)?)?;
            }
            "--overwrite" => overwrite = true,
            "--json" => json = true,
            _ => return Err(format!("unknown client enroll flag: {}", arg)),
        }
    }
    if server_url.trim().is_empty() {
        return Err("--server-url is required".to_string());
    }
    if pairing_code.trim().is_empty() {
        return Err("--pairing-code is required".to_string());
    }
    if client_id.trim().is_empty() {
        return Err("--client-id is required".to_string());
    }
    if !matches!(
        transport.as_str(),
        agent_init::TRANSPORT_WEBSOCKET
            | agent_init::TRANSPORT_POLLING
            | agent_init::TRANSPORT_QUIC
            | agent_init::TRANSPORT_AUTO
    ) {
        return Err("--transport must be websocket, polling, quic, or auto".to_string());
    }
    let output_dir = if let Some(output_dir) = output_dir {
        if let Some(profile) = profile.as_deref() {
            validate_client_profile(profile)?;
        }
        output_dir
    } else {
        let profile = validate_client_profile(profile.as_deref().unwrap_or(&client_id))?;
        default_client_output_dir_for_profile(&profile)
    };
    if output_dir.as_os_str().is_empty() {
        return Err("--output-dir cannot be empty".to_string());
    }
    let agent_config = agent_config.unwrap_or_else(|| output_dir.join("agent.toml"));
    let projects_dir = projects_dir.unwrap_or_else(|| output_dir.join("projects.d"));
    if agent_config.as_os_str().is_empty() {
        return Err("--agent-config cannot be empty".to_string());
    }
    if projects_dir.as_os_str().is_empty() {
        return Err("--projects-dir cannot be empty".to_string());
    }
    if allowed_roots.iter().any(|path| path.as_os_str().is_empty()) {
        return Err("--allowed-root cannot be empty".to_string());
    }
    Ok(ClientEnrollOptions {
        server_url,
        pairing_code,
        client_id,
        display_name,
        transport,
        output_dir,
        agent_config,
        projects_dir,
        allowed_roots,
        allow_cwd_anywhere,
        overwrite,
        json,
    })
}

fn parse_doctor_command(args: &[String]) -> CliAction {
    if args.first().is_some_and(|a| a == "--help" || a == "-h") {
        return CliAction::Exit {
            code: 0,
            stdout: doctor_usage().to_string(),
            stderr: String::new(),
        };
    }
    match parse_doctor(args) {
        Ok(opts) => CliAction::Doctor(opts),
        Err(e) => CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("{}\n", e),
        },
    }
}

fn parse_doctor(args: &[String]) -> Result<DoctorOptions, String> {
    let mut opts = DoctorOptions {
        quic_timeout_secs: 10,
        ..DoctorOptions::default()
    };
    let mut profile: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--server-url" => opts.server_url = Some(next_value(&mut iter, arg)?),
            "--env-file" => opts.env_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--token-file" => opts.token_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--profile" => profile = Some(next_value(&mut iter, arg)?),
            "--user-token-file" => {
                opts.user_token_file = Some(PathBuf::from(next_value(&mut iter, arg)?))
            }
            "--agent-token-file" => {
                opts.agent_token_file = Some(PathBuf::from(next_value(&mut iter, arg)?))
            }
            "--agent-config" => {
                opts.agent_config = Some(PathBuf::from(next_value(&mut iter, arg)?))
            }
            "--project" => opts.project = Some(next_value(&mut iter, arg)?),
            "--quic" => opts.quic = true,
            "--server-only" => opts.quic_server_only = true,
            "--agent-e2e" => opts.quic_agent_e2e = true,
            "--quic-server-addr" => opts.quic_server_addr = Some(next_value(&mut iter, arg)?),
            "--quic-server-name" => opts.quic_server_name = Some(next_value(&mut iter, arg)?),
            "--quic-alpn" => opts.quic_alpn = next_value(&mut iter, arg)?,
            "--quic-timeout-secs" => {
                let value = next_value(&mut iter, arg)?;
                opts.quic_timeout_secs = value
                    .parse::<u64>()
                    .map_err(|_| "--quic-timeout-secs must be an integer".to_string())?;
            }
            "--quic-client-id" => opts.quic_client_id = Some(next_value(&mut iter, arg)?),
            "--json" => opts.json = true,
            "--strict" => opts.strict = true,
            _ => return Err(format!("unknown doctor flag: {}", arg)),
        }
    }
    if let Some(profile) = profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?
    {
        opts.agent_config = opts
            .agent_config
            .or_else(|| Some(client_profile_agent_config(&profile)));
        opts.user_token_file = opts
            .user_token_file
            .or_else(|| Some(client_profile_user_token_file(&profile)));
        opts.agent_token_file = opts
            .agent_token_file
            .or_else(|| Some(client_profile_agent_token_file(&profile)));
    }
    if let Some(url) = &opts.server_url {
        if url.trim().is_empty() {
            return Err("--server-url cannot be empty".to_string());
        }
    }
    if opts.quic_server_only || opts.quic_agent_e2e {
        opts.quic = true;
    }
    if opts.quic_timeout_secs == 0 {
        return Err("--quic-timeout-secs must be > 0".to_string());
    }
    if opts.quic_server_only && opts.quic_agent_e2e {
        return Err("--server-only and --agent-e2e are mutually exclusive".to_string());
    }
    Ok(opts)
}

/// Small flag parser for `webcodex-cli agent init`. Produces an
/// `AgentInitOptions` consumed by the shared `agent_init::run_agent_init`.
fn parse_cli_agent_init(args: &[String]) -> Result<AgentInitOptions, String> {
    let mut opts = AgentInitOptions {
        server_url: String::new(),
        token: None,
        token_file: None,
        client_id: String::new(),
        owner: String::new(),
        display_name: None,
        transport: TRANSPORT_WEBSOCKET.to_string(),
        poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
        projects_dir: PathBuf::new(),
        output: PathBuf::new(),
        allowed_roots: Vec::new(),
        allow_cwd_anywhere: false,
        overwrite: false,
    };
    let mut profile: Option<String> = None;
    let mut output_explicit = false;
    let mut projects_dir_explicit = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--server-url" => opts.server_url = next_value(&mut iter, arg)?,
            "--token" => opts.token = Some(next_value(&mut iter, arg)?),
            "--token-file" => opts.token_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--client-id" => opts.client_id = next_value(&mut iter, arg)?,
            "--profile" => profile = Some(next_value(&mut iter, arg)?),
            "--owner" => opts.owner = next_value(&mut iter, arg)?,
            "--display-name" => opts.display_name = Some(next_value(&mut iter, arg)?),
            "--transport" => opts.transport = next_value(&mut iter, arg)?,
            "--poll-interval-ms" => {
                let v = next_value(&mut iter, arg)?;
                opts.poll_interval_ms = v
                    .parse::<u64>()
                    .map_err(|_| "--poll-interval-ms must be an integer".to_string())?;
            }
            "--projects-dir" => {
                opts.projects_dir = PathBuf::from(next_value(&mut iter, arg)?);
                projects_dir_explicit = true;
            }
            "--allowed-root" => opts
                .allowed_roots
                .push(PathBuf::from(next_value(&mut iter, arg)?)),
            "--allow-cwd-anywhere" => {
                opts.allow_cwd_anywhere = agent_init::parse_bool(&next_value(&mut iter, arg)?)?;
            }
            "--output" => {
                opts.output = PathBuf::from(next_value(&mut iter, arg)?);
                output_explicit = true;
            }
            "--overwrite" => opts.overwrite = true,
            "--help" | "-h" => return Err(usage().to_string()),
            _ => return Err(format!("unknown agent init flag: {}", arg)),
        }
    }
    if let Some(profile) = profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?
    {
        if !output_explicit {
            opts.output = client_profile_agent_config(&profile);
        }
        if !projects_dir_explicit {
            opts.projects_dir = client_profile_projects_dir(&profile);
        }
    } else {
        if !output_explicit && opts.output.as_os_str().is_empty() {
            let profile = validate_client_profile(&opts.client_id)?;
            opts.output = client_profile_agent_config(&profile);
            if !projects_dir_explicit {
                opts.projects_dir = client_profile_projects_dir(&profile);
            }
        } else if !projects_dir_explicit {
            opts.projects_dir = PathBuf::from(DEFAULT_INIT_PROJECTS_DIR);
        }
    }
    agent_init::validate_agent_init_options(&opts)?;
    Ok(opts)
}

fn parse_setup_subcommand(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: "expected `setup single-user`\n".to_string(),
        };
    }
    match args[0].as_str() {
        "single-user" => match parse_setup_single_user(&args[1..]) {
            Ok(opts) => CliAction::SetupSingleUser(opts),
            Err(e) => CliAction::Exit {
                code: 2,
                stdout: String::new(),
                stderr: format!("{}\n", e),
            },
        },
        "--help" | "-h" => CliAction::Exit {
            code: 0,
            stdout: usage().to_string(),
            stderr: String::new(),
        },
        other => CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("unknown setup subcommand: {}\n", other),
        },
    }
}

fn parse_setup_single_user(args: &[String]) -> Result<SetupSingleUserOptions, String> {
    let mut opts = SetupSingleUserOptions {
        server_url: String::new(),
        token: None,
        token_file: None,
        username: String::new(),
        client_id: String::new(),
        display_name: None,
        role: "admin".to_string(),
        gpt_token_name: "chatgpt-action".to_string(),
        agent_token_name: String::new(),
        output_dir: PathBuf::new(),
        force_create_tokens: false,
        json: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--server-url" => opts.server_url = next_value(&mut iter, arg)?,
            "--token" => opts.token = Some(next_value(&mut iter, arg)?),
            "--token-file" => opts.token_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--username" => opts.username = next_value(&mut iter, arg)?,
            "--client-id" => opts.client_id = next_value(&mut iter, arg)?,
            "--display-name" => opts.display_name = Some(next_value(&mut iter, arg)?),
            "--role" => opts.role = next_value(&mut iter, arg)?,
            "--gpt-token-name" => opts.gpt_token_name = next_value(&mut iter, arg)?,
            "--agent-token-name" => opts.agent_token_name = next_value(&mut iter, arg)?,
            "--output-dir" => opts.output_dir = PathBuf::from(next_value(&mut iter, arg)?),
            "--force-create-tokens" => opts.force_create_tokens = true,
            "--json" => opts.json = true,
            "--help" | "-h" => return Err(usage().to_string()),
            _ => return Err(format!("unknown setup single-user flag: {}", arg)),
        }
    }
    if opts.server_url.trim().is_empty() {
        return Err("--server-url is required".to_string());
    }
    if opts.token.is_some() && opts.token_file.is_some() {
        return Err("use only one of --token or --token-file".to_string());
    }
    if opts.username.trim().is_empty() {
        return Err("--username is required".to_string());
    }
    if opts.client_id.trim().is_empty() {
        return Err("--client-id is required".to_string());
    }
    if opts.output_dir.as_os_str().is_empty() {
        return Err("--output-dir is required".to_string());
    }
    if opts.agent_token_name.is_empty() {
        opts.agent_token_name = format!("{} agent", opts.client_id);
    }
    Ok(opts)
}

fn next_value<'a, I>(iter: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = &'a String>,
{
    iter.next()
        .cloned()
        .ok_or_else(|| format!("{} requires a value", flag))
}

fn write_text_file(
    path: &Path,
    content: &str,
    overwrite: bool,
    secret: bool,
) -> Result<(), String> {
    if path.exists() && !overwrite {
        return Err(format!(
            "{} already exists; pass --overwrite to replace it",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut options = std::fs::OpenOptions::new();
        options.write(true);
        if overwrite {
            options.create(true).truncate(true);
        } else {
            options.create_new(true);
        }
        if secret {
            options.mode(0o600);
        }
        let mut file = options
            .open(path)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        use std::io::Write;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        if secret {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("failed to set permissions on {}: {}", path.display(), e))?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = secret;
        if overwrite {
            std::fs::write(path, content)
                .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        } else {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
            use std::io::Write;
            file.write_all(content.as_bytes())
                .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        }
    }
    Ok(())
}

fn discover_webcodex_binary() -> Option<PathBuf> {
    discover_named_binary_absolute("webcodex")
}

fn discover_named_binary_absolute(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if !dir.is_absolute() {
            continue;
        }
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{}.exe", name));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn systemctl_available() -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join("systemctl").is_file())
}

fn is_systemd_platform() -> bool {
    cfg!(target_os = "linux") && systemctl_available()
}

/// Write `content` to `path` with 0600 permissions on Unix, creating parent
/// directories as needed. Used for one-time plaintext token files.
fn write_secret_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        use std::io::Write;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("failed to set permissions on {}: {}", path.display(), e))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
    }
    Ok(())
}

fn discover_binary(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{}.exe", name));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn read_optional_token(path: &Option<PathBuf>, label: &str) -> Result<Option<String>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let token = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {} {}: {}", label, path.display(), e))?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(format!("{} {} is empty", label, path.display()));
    }
    Ok(Some(token))
}

fn resolve_doctor_general_token(opts: &DoctorOptions) -> Result<Option<String>, String> {
    if let Some(token) = read_optional_token(&opts.token_file, "--token-file")? {
        return Ok(Some(token));
    }
    if let Some(path) = &opts.env_file {
        if let Some(token) = read_env_file_value(path, "WEBCODEX_TOKEN")? {
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Ok(Some(token));
            }
        }
    }
    Ok(None)
}

fn rustls_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn build_doctor_quic_client_crypto(
    alpn: &str,
) -> Result<quinn::crypto::rustls::QuicClientConfig, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut client_crypto = rustls::ClientConfig::builder_with_provider(rustls_provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("failed to select rustls protocol versions: {}", e))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![alpn.as_bytes().to_vec()];
    quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
        .map_err(|e| format!("failed to build quic client crypto: {}", e))
}

#[derive(Debug, Clone)]
struct DoctorQuicResolved {
    server_addr: String,
    server_name: String,
    alpn: String,
    timeout_secs: u64,
    client_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorRuntimeQuicStatus {
    enabled: bool,
    listen: String,
    alpn: String,
    listener_started: bool,
    last_error: Option<String>,
}

fn sanitize_doctor_quic_error(error: &str) -> String {
    let compact = error.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = compact.to_ascii_lowercase();
    if lower.contains("quic_cert") && lower.contains("does not exist") {
        return "WEBCODEX_QUIC_CERT path does not exist".to_string();
    }
    if lower.contains("quic_key") && lower.contains("does not exist") {
        return "WEBCODEX_QUIC_KEY path does not exist".to_string();
    }
    if (lower.contains("quic cert") || lower.contains("quic key") || lower.contains("private key"))
        && lower.contains('/')
    {
        return "QUIC listener startup error; check runtime_status.last_error and journalctl"
            .to_string();
    }
    compact.chars().take(240).collect()
}

fn parse_runtime_quic_status(output: &Value) -> Option<DoctorRuntimeQuicStatus> {
    let quic = output.get("quic")?;
    Some(DoctorRuntimeQuicStatus {
        enabled: quic
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        listen: quic
            .get("listen")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)")
            .to_string(),
        alpn: quic
            .get("alpn")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)")
            .to_string(),
        listener_started: quic
            .get("listener_started")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        last_error: quic
            .get("last_error")
            .and_then(Value::as_str)
            .map(sanitize_doctor_quic_error),
    })
}

fn doctor_runtime_quic_checks(output: &Value) -> (Vec<DoctorCheck>, bool) {
    let Some(status) = parse_runtime_quic_status(output) else {
        return (
            vec![DoctorCheck::warn(
                "quic runtime config",
                "not exposed by this server version; check server logs",
            )],
            true,
        );
    };

    let detail = format!(
        "enabled={} listen={} alpn={} listener_started={}",
        status.enabled, status.listen, status.alpn, status.listener_started
    );
    let mut checks = vec![DoctorCheck::pass("quic runtime config", detail)];
    if !status.enabled {
        checks.push(DoctorCheck::fail(
            "quic runtime enabled",
            "server reports QUIC disabled; set WEBCODEX_QUIC_ENABLED=true and restart webcodex",
        ));
        return (checks, false);
    }
    if !status.listener_started {
        checks.push(DoctorCheck::fail(
            "quic listener started",
            format!(
                "server reports QUIC enabled but listener not started{}",
                status
                    .last_error
                    .as_deref()
                    .map(|e| format!(": {}", e))
                    .unwrap_or_default()
            ),
        ));
        return (checks, false);
    }
    (checks, true)
}

fn read_doctor_agent_config(path: &Path) -> Result<DoctorAgentConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read agent config {}: {}", path.display(), e))?;
    toml::from_str(&content)
        .map_err(|e| format!("failed to parse agent config {}: {}", path.display(), e))
}

fn resolve_doctor_quic_options(opts: &DoctorOptions) -> Result<DoctorQuicResolved, String> {
    let agent_cfg = match opts.agent_config.as_deref() {
        Some(path) => Some(read_doctor_agent_config(path)?),
        None => None,
    };
    let quic_cfg = agent_cfg.as_ref().and_then(|cfg| cfg.quic.as_ref());
    let server_addr = opts
        .quic_server_addr
        .clone()
        .or_else(|| quic_cfg.map(|q| q.server_addr.clone()))
        .unwrap_or_default();
    let server_name = opts
        .quic_server_name
        .clone()
        .or_else(|| quic_cfg.map(|q| q.server_name.clone()))
        .unwrap_or_default();
    let alpn = if opts.quic_alpn.trim().is_empty() {
        quic_cfg
            .map(|q| q.alpn.clone())
            .unwrap_or_else(default_doctor_quic_alpn)
    } else {
        opts.quic_alpn.clone()
    };
    let timeout_secs = if opts.quic_timeout_secs == 0 {
        quic_cfg
            .map(|q| q.connect_timeout_secs)
            .filter(|v| *v > 0)
            .unwrap_or_else(default_doctor_quic_connect_timeout_secs)
    } else {
        opts.quic_timeout_secs
    };
    let client_id = opts.quic_client_id.clone().or_else(|| {
        agent_cfg.as_ref().and_then(|cfg| {
            let id = cfg.client_id.trim();
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        })
    });
    if server_addr.trim().is_empty() {
        return Err(
            "--quic-server-addr is required for --quic unless [quic].server_addr is in --agent-config"
                .to_string(),
        );
    }
    if server_name.trim().is_empty() {
        return Err(
            "--quic-server-name is required for --quic unless [quic].server_name is in --agent-config"
                .to_string(),
        );
    }
    Ok(DoctorQuicResolved {
        server_addr,
        server_name,
        alpn,
        timeout_secs,
        client_id,
    })
}

fn classify_quic_connect_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("certificate")
        || lower.contains("cert")
        || lower.contains("webpki")
        || lower.contains("notvalidforname")
        || lower.contains("unknownissuer")
    {
        "certificate verify failed (check server_name and certificate SAN/issuer)".to_string()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "connect timeout (check UDP firewall/security group/NAT and listener bind)".to_string()
    } else if lower.contains("applicationclosed")
        || lower.contains("connectionclosed")
        || lower.contains("closed")
        || lower.contains("no application protocol")
        || lower.contains("alpn")
    {
        "handshake failed (check QUIC listener is enabled and ALPN matches)".to_string()
    } else {
        "quic connect failed".to_string()
    }
}

async fn doctor_quic_handshake(
    addr: SocketAddr,
    server_name: &str,
    alpn: &str,
    timeout_secs: u64,
) -> Result<(), String> {
    let client_crypto = build_doctor_quic_client_crypto(alpn)?;
    let client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
    let endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().expect("valid local addr"))
        .map_err(|e| format!("failed to bind local quic UDP socket: {}", e))?;
    let connect = endpoint
        .connect_with(client_config, addr, server_name)
        .map_err(|e| format!("failed to start quic connect: {}", e))?;
    let conn = tokio::time::timeout(Duration::from_secs(timeout_secs), connect)
        .await
        .map_err(|_| format!("quic connect to {} timed out after {}s", addr, timeout_secs))?
        .map_err(|e| {
            let raw = e.to_string();
            format!("{}: {}", classify_quic_connect_error(&raw), raw)
        })?;
    conn.close(0u32.into(), b"webcodex doctor done");
    endpoint.wait_idle().await;
    Ok(())
}

async fn run_quic_doctor_checks(
    opts: &DoctorOptions,
    preferred_token: Option<&str>,
) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let resolved = match resolve_doctor_quic_options(opts) {
        Ok(resolved) => resolved,
        Err(e) => {
            checks.push(DoctorCheck::fail("quic config", e));
            return checks;
        }
    };
    checks.push(DoctorCheck::pass(
        "quic config",
        format!(
            "server_addr={} server_name={} alpn={} timeout_secs={} client_id={}",
            resolved.server_addr,
            resolved.server_name,
            resolved.alpn,
            resolved.timeout_secs,
            resolved.client_id.as_deref().unwrap_or("(not specified)")
        ),
    ));

    if let (Some(server_url), Some(token)) = (opts.server_url.as_deref(), preferred_token) {
        match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({})).await
        {
            Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                let output = value.get("output").unwrap_or(&value);
                let (mut runtime_checks, should_continue) = doctor_runtime_quic_checks(output);
                checks.append(&mut runtime_checks);
                if !should_continue {
                    return checks;
                }
            }
            Ok((status, content_type, _)) => checks.push(DoctorCheck::warn(
                "quic runtime config",
                format!(
                    "runtime_status unavailable for QUIC preflight: HTTP {} content-type {}",
                    status, content_type
                ),
            )),
            Err(e) => checks.push(DoctorCheck::warn(
                "quic runtime config",
                format!("runtime_status unavailable for QUIC preflight: {}", e),
            )),
        }
    } else {
        checks.push(DoctorCheck::warn(
            "quic runtime config",
            "not checked; pass --server-url with --user-token-file or --token-file to read runtime_status",
        ));
    }

    let addrs = match resolved.server_addr.to_socket_addrs() {
        Ok(iter) => iter.collect::<Vec<_>>(),
        Err(e) => {
            checks.push(DoctorCheck::fail(
                "quic resolve",
                format!("failed to resolve {}: {}", resolved.server_addr, e),
            ));
            return checks;
        }
    };
    if addrs.is_empty() {
        checks.push(DoctorCheck::fail(
            "quic resolve",
            format!("{} resolved to no socket addresses", resolved.server_addr),
        ));
        return checks;
    }
    checks.push(DoctorCheck::pass(
        "quic resolve",
        format!(
            "{} -> {}",
            resolved.server_addr,
            addrs
                .iter()
                .map(SocketAddr::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ));

    let mut handshake_ok = false;
    let mut handshake_errors = Vec::new();
    for addr in &addrs {
        match doctor_quic_handshake(
            *addr,
            &resolved.server_name,
            &resolved.alpn,
            resolved.timeout_secs,
        )
        .await
        {
            Ok(()) => {
                checks.push(DoctorCheck::pass(
                    "quic handshake",
                    format!(
                        "{} ok; ALPN '{}' negotiated and certificate SAN/chain verified for {}",
                        addr, resolved.alpn, resolved.server_name
                    ),
                ));
                handshake_ok = true;
                break;
            }
            Err(e) => handshake_errors.push(format!("{}: {}", addr, e)),
        }
    }
    if !handshake_ok {
        checks.push(DoctorCheck::fail(
            "quic handshake",
            format!(
                "server listener started but UDP handshake failed: {}",
                handshake_errors.join("; ")
            ),
        ));
        return checks;
    }

    if opts.quic_server_only || !opts.quic_agent_e2e {
        checks.push(DoctorCheck::warn(
            "quic agent e2e",
            "skipped; pass --agent-e2e with --server-url, --user-token-file, --project, and an online quic-v1 agent",
        ));
        return checks;
    }

    let Some(server_url) = opts.server_url.as_deref() else {
        checks.push(DoctorCheck::fail(
            "quic agent e2e",
            "--server-url is required for --agent-e2e",
        ));
        return checks;
    };
    let Some(token) = preferred_token else {
        checks.push(DoctorCheck::fail(
            "quic agent e2e",
            "--user-token-file or --token-file/--env-file is required for runtime API checks",
        ));
        return checks;
    };
    let Some(project) = opts.project.as_deref() else {
        checks.push(DoctorCheck::fail(
            "quic agent e2e",
            "--project is required for run_shell/run_job checks",
        ));
        return checks;
    };

    checks.extend(
        run_quic_agent_e2e_checks(server_url, token, project, resolved.client_id.as_deref()).await,
    );
    checks
}

async fn run_quic_agent_e2e_checks(
    server_url: &str,
    token: &str,
    project: &str,
    expected_client_id: Option<&str>,
) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({})).await {
        Ok((status, _, Some(value))) if (200..300).contains(&status) => {
            let output = value.get("output").unwrap_or(&value);
            let clients = output
                .pointer("/agents/clients")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let matching = clients.iter().find(|client| {
                let transport = client.get("transport").and_then(Value::as_str);
                let protocol = client.get("agent_protocol_version").and_then(Value::as_str);
                let client_id_matches = expected_client_id
                    .is_none_or(|id| client.get("client_id").and_then(Value::as_str) == Some(id));
                client_id_matches && transport == Some("quic") && protocol == Some("quic-v1")
            });
            let wrong_transport_or_protocol = clients.iter().find(|client| {
                let client_id_matches = expected_client_id
                    .is_none_or(|id| client.get("client_id").and_then(Value::as_str) == Some(id));
                let connected = client.get("connected").and_then(Value::as_bool) == Some(true);
                let transport = client.get("transport").and_then(Value::as_str);
                let protocol = client.get("agent_protocol_version").and_then(Value::as_str);
                client_id_matches
                    && connected
                    && (transport != Some("quic") || protocol != Some("quic-v1"))
            });
            match matching {
                Some(client) => {
                    let connected = client.get("connected").and_then(Value::as_bool);
                    let pending = client.get("pending_requests").and_then(Value::as_u64);
                    checks.push(DoctorCheck::pass(
                        "quic agent online",
                        format!(
                            "client_id={} transport=quic protocol=quic-v1 connected={:?} pending_requests={} last_seen={}",
                            client
                                .get("client_id")
                                .and_then(Value::as_str)
                                .unwrap_or("(unknown)"),
                            connected,
                            pending
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "unknown".to_string()),
                            client
                                .get("last_seen")
                                .map(Value::to_string)
                                .unwrap_or_else(|| "unknown".to_string())
                        ),
                    ));
                    if connected != Some(true) {
                        checks.push(DoctorCheck::fail(
                            "quic agent connected",
                            "matching quic-v1 agent is not connected",
                        ));
                    }
                    if !client
                        .get("capabilities")
                        .is_some_and(|cap| cap.is_object() && cap.get("shell").is_some())
                    {
                        checks.push(DoctorCheck::warn(
                            "quic capabilities",
                            "matching agent did not expose a shell capability summary",
                        ));
                    }
                }
                None => checks.push(DoctorCheck::fail(
                    "quic agent online",
                    if let Some(client) = wrong_transport_or_protocol {
                        format!(
                            "agent online but wrong protocol/transport: client_id={} transport={} protocol={}",
                            client
                                .get("client_id")
                                .and_then(Value::as_str)
                                .unwrap_or("(unknown)"),
                            client
                                .get("transport")
                                .and_then(Value::as_str)
                                .unwrap_or("(missing)"),
                            client
                                .get("agent_protocol_version")
                                .and_then(Value::as_str)
                                .unwrap_or("(missing)")
                        )
                    } else {
                        match expected_client_id {
                            Some(id) => {
                                format!("no online quic-v1 agent found for client_id={}", id)
                            }
                            None => "no online quic-v1 agent found".to_string(),
                        }
                    },
                )),
            }
        }
        Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
            "quic agent online",
            format!(
                "runtime_status HTTP {} content-type {}",
                status, content_type
            ),
        )),
        Err(e) => checks.push(DoctorCheck::fail("quic agent online", e)),
    }

    match http_post_json_status(
        server_url,
        "/api/tools/call",
        Some(token),
        json!({"tool":"run_shell","params":{"project":project,"command":"printf webcodex-quic-ok"}}),
    )
    .await
    {
        Ok((status, _, Some(value))) if (200..300).contains(&status) => {
            let stdout = value
                .pointer("/output/stdout")
                .and_then(Value::as_str)
                .unwrap_or("");
            let exit_code = value.pointer("/output/exit_code").and_then(Value::as_i64);
            if stdout.contains("webcodex-quic-ok") && exit_code == Some(0) {
                checks.push(DoctorCheck::pass(
                    "quic run_shell",
                    format!("project '{}' returned marker", project),
                ));
            } else {
                checks.push(DoctorCheck::fail(
                    "quic run_shell",
                    format!(
                        "project '{}' exit_code={:?} without expected marker",
                        project, exit_code
                    ),
                ));
            }
        }
        Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
            "quic run_shell",
            format!("HTTP {} content-type {}", status, content_type),
        )),
        Err(e) => checks.push(DoctorCheck::fail("quic run_shell", e)),
    }

    let job_id = match http_post_json_status(
        server_url,
        "/api/tools/call",
        Some(token),
        json!({"tool":"run_job","params":{"project":project,"command":"printf webcodex-quic-job-ok","timeout_secs":10}}),
    )
    .await
    {
        Ok((status, _, Some(value))) if (200..300).contains(&status) => {
            match value.pointer("/output/job_id").and_then(Value::as_str) {
                Some(job_id) => {
                    checks.push(DoctorCheck::pass(
                        "quic run_job",
                        format!("started job_id={}", job_id),
                    ));
                    Some(job_id.to_string())
                }
                None => {
                    checks.push(DoctorCheck::fail(
                        "quic run_job",
                        "response did not include output.job_id",
                    ));
                    None
                }
            }
        }
        Ok((status, content_type, _)) => {
            checks.push(DoctorCheck::fail(
                "quic run_job",
                format!("HTTP {} content-type {}", status, content_type),
            ));
            None
        }
        Err(e) => {
            checks.push(DoctorCheck::fail("quic run_job", e));
            None
        }
    };

    let Some(job_id) = job_id else {
        return checks;
    };
    let mut final_status = None;
    for _ in 0..20 {
        match http_post_json_status(
            server_url,
            "/api/tools/call",
            Some(token),
            json!({"tool":"job_status","params":{"job_id":job_id}}),
        )
        .await
        {
            Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                let status = value
                    .pointer("/output/status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if matches!(status.as_str(), "completed" | "failed" | "stopped" | "lost") {
                    final_status = Some(status);
                    break;
                }
            }
            Ok((status, content_type, _)) => {
                checks.push(DoctorCheck::fail(
                    "quic job_status",
                    format!("HTTP {} content-type {}", status, content_type),
                ));
                return checks;
            }
            Err(e) => {
                checks.push(DoctorCheck::fail("quic job_status", e));
                return checks;
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    match final_status.as_deref() {
        Some("completed") => checks.push(DoctorCheck::pass(
            "quic job_status",
            format!("job_id={} completed", job_id),
        )),
        Some(status) => checks.push(DoctorCheck::fail(
            "quic job_status",
            format!("job_id={} ended with status={}", job_id, status),
        )),
        None => checks.push(DoctorCheck::fail(
            "quic job_status",
            format!("job_id={} did not finish before timeout", job_id),
        )),
    }

    match http_post_json_status(
        server_url,
        "/api/tools/call",
        Some(token),
        json!({"tool":"job_log","params":{"job_id":job_id,"tail_lines":50}}),
    )
    .await
    {
        Ok((status, _, Some(value))) if (200..300).contains(&status) => {
            let stdout = value
                .pointer("/output/stdout")
                .and_then(Value::as_str)
                .unwrap_or("");
            if stdout.contains("webcodex-quic-job-ok") {
                checks.push(DoctorCheck::pass(
                    "quic job_log",
                    format!("job_id={} output marker found", job_id),
                ));
            } else {
                checks.push(DoctorCheck::fail(
                    "quic job_log",
                    format!("job_id={} output marker missing", job_id),
                ));
            }
        }
        Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
            "quic job_log",
            format!("HTTP {} content-type {}", status, content_type),
        )),
        Err(e) => checks.push(DoctorCheck::fail("quic job_log", e)),
    }
    checks.push(DoctorCheck::warn(
        "quic disconnect",
        "manual step: stop the agent and rerun runtime_status/list_agents to observe stale/offline reconciliation",
    ));
    checks
}

// ============================================================================
// Local agent-config doctor (shell profiles / projects)
// ============================================================================
//
// Parses agent.toml locally (no server contact) to diagnose shell-profile
// configuration: whether projects_dir exists, whether project tomls parse,
// whether project.path exists, and whether project.shell_profile resolves to
// a configured profile (or shell.default_profile). Never prints init_script
// bodies, env values, tokens, or the full env snapshot.

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
struct DoctorShellProfileConfig {
    #[serde(default)]
    program: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    init_script: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DoctorShellConfig {
    #[serde(default)]
    default_profile: Option<String>,
    #[serde(default)]
    profiles: BTreeMap<String, DoctorShellProfileConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
struct DoctorAgentPolicy {
    #[serde(default)]
    allowed_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct DoctorAgentConfig {
    #[serde(default)]
    server_url: String,
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    quic: Option<DoctorQuicConfig>,
    #[serde(default)]
    projects_dir: Option<PathBuf>,
    #[serde(default)]
    shell: DoctorShellConfig,
    #[serde(default)]
    policy: DoctorAgentPolicy,
}

#[derive(Debug, Clone, Deserialize)]
struct DoctorQuicConfig {
    #[serde(default)]
    server_addr: String,
    #[serde(default)]
    server_name: String,
    #[serde(default = "default_doctor_quic_alpn")]
    alpn: String,
    #[serde(default = "default_doctor_quic_connect_timeout_secs")]
    connect_timeout_secs: u64,
}

fn default_doctor_quic_alpn() -> String {
    "webcodex-agent/1".to_string()
}

fn default_doctor_quic_connect_timeout_secs() -> u64 {
    10
}

#[derive(Debug, Clone, Deserialize)]
struct DoctorAgentProject {
    id: String,
    path: String,
    #[serde(default)]
    shell_profile: Option<String>,
    #[serde(default)]
    disabled: bool,
}

/// Resolve the effective profile name for a project: `project.shell_profile`
/// first, else `shell.default_profile`, else `None`. Used to report which
/// profile a project would use without exposing env values or init_script.
fn resolve_doctor_profile_name(
    project: &DoctorAgentProject,
    shell: &DoctorShellConfig,
) -> Option<String> {
    project
        .shell_profile
        .clone()
        .or_else(|| shell.default_profile.clone())
}

/// Run the local agent-config doctor checks. Returns one or more
/// `DoctorCheck`s. Never prints init_script bodies or env values: profile env
/// is reported only as a key count, and init_script only as a boolean.
fn run_local_agent_doctor(config_path: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let content = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(e) => {
            checks.push(DoctorCheck::fail(
                "agent config",
                format!("failed to read {}: {}", config_path.display(), e),
            ));
            return checks;
        }
    };
    let cfg: DoctorAgentConfig = match toml::from_str(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            checks.push(DoctorCheck::fail(
                "agent config",
                format!("failed to parse {}: {}", config_path.display(), e),
            ));
            return checks;
        }
    };
    checks.push(DoctorCheck::pass(
        "agent config",
        format!(
            "parsed {}; client_id={}",
            config_path.display(),
            if cfg.client_id.trim().is_empty() {
                "(empty)"
            } else {
                cfg.client_id.as_str()
            }
        ),
    ));

    // shell.profiles summary (sanitized: no env values, no init_script bodies).
    let configured_count = cfg.shell.profiles.len();
    let profile_names: Vec<&str> = cfg.shell.profiles.keys().map(String::as_str).collect();
    checks.push(DoctorCheck::pass(
        "shell profiles",
        format!(
            "configured_count={} default_profile={} profiles=[{}]",
            configured_count,
            cfg.shell.default_profile.as_deref().unwrap_or("(none)"),
            profile_names.join(", ")
        ),
    ));
    if let Some(default_profile) = &cfg.shell.default_profile {
        if !cfg.shell.profiles.contains_key(default_profile) {
            checks.push(DoctorCheck::fail(
                "shell default_profile",
                format!(
                    "shell.default_profile '{}' does not match any shell.profiles entry",
                    default_profile
                ),
            ));
        }
    }

    // projects_dir + per-project checks.
    let projects_dir = cfg.projects_dir.clone().unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config/webcodex/projects.d")
    });
    if !projects_dir.exists() {
        checks.push(DoctorCheck::warn(
            "projects_dir",
            format!("{} does not exist", projects_dir.display()),
        ));
        return checks;
    }
    let mut project_files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
                project_files.push(path);
            }
        }
    }
    project_files.sort();
    let mut loaded = 0usize;
    let mut parse_errors = 0usize;
    for file in &project_files {
        let content = match std::fs::read_to_string(file) {
            Ok(content) => content,
            Err(e) => {
                parse_errors += 1;
                checks.push(DoctorCheck::warn(
                    "project config",
                    format!("failed to read {}: {}", file.display(), e),
                ));
                continue;
            }
        };
        let project: DoctorAgentProject = match toml::from_str(&content) {
            Ok(project) => project,
            Err(e) => {
                parse_errors += 1;
                checks.push(DoctorCheck::warn(
                    "project config",
                    format!("failed to parse {}: {}", file.display(), e),
                ));
                continue;
            }
        };
        if project.disabled {
            continue;
        }
        loaded += 1;
        let project_path = PathBuf::from(&project.path);
        if !project_path.exists() {
            checks.push(DoctorCheck::fail(
                format!("project '{}' path", project.id),
                format!("path {} does not exist", project_path.display()),
            ));
        }
        // allowed_roots membership (only when not allow_cwd_anywhere; the
        // local doctor does not parse allow_cwd_anywhere, so this is a
        // best-effort informational check).
        if !cfg.policy.allowed_roots.is_empty() {
            let inside = match project_path.canonicalize() {
                Ok(canon) => cfg.policy.allowed_roots.iter().any(|root| {
                    root.canonicalize()
                        .map(|root| canon == root || canon.starts_with(&root))
                        .unwrap_or(false)
                }),
                Err(_) => false,
            };
            if !inside {
                checks.push(DoctorCheck::warn(
                    format!("project '{}' allowed_roots", project.id),
                    format!(
                        "path {} is outside the configured allowed_roots",
                        project_path.display()
                    ),
                ));
            }
        }
        // shell_profile resolution.
        let resolved = resolve_doctor_profile_name(&project, &cfg.shell);
        match resolved {
            None => checks.push(DoctorCheck::pass(
                format!("project '{}' shell_profile", project.id),
                "no profile configured (fallback to plain shell)".to_string(),
            )),
            Some(name) => {
                if cfg.shell.profiles.contains_key(&name) {
                    // Sanitized prepare-shape check: report has_init_script and
                    // env key count only, never the contents.
                    let profile = cfg.shell.profiles.get(&name).expect("checked above");
                    checks.push(DoctorCheck::pass(
                        format!("project '{}' shell_profile", project.id),
                        format!(
                            "resolved='{}' has_init_script={} env_keys_count={}",
                            name,
                            profile.init_script.is_some(),
                            profile.env.len()
                        ),
                    ));
                } else {
                    checks.push(DoctorCheck::fail(
                        format!("project '{}' shell_profile", project.id),
                        format!(
                            "resolved profile '{}' is not in shell.profiles (project.shell_profile={}, default_profile={})",
                            name,
                            project.shell_profile.as_deref().unwrap_or("(none)"),
                            cfg.shell.default_profile.as_deref().unwrap_or("(none)")
                        ),
                    ));
                }
            }
        }
    }
    checks.push(DoctorCheck::pass(
        "projects_dir",
        format!(
            "{} loaded={} parse_errors={}",
            projects_dir.display(),
            loaded,
            parse_errors
        ),
    ));
    checks
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SystemdStatus {
    active: String,
    enabled: String,
}

fn query_systemd_service_status(service_name: &str) -> SystemdStatus {
    fn run_status(args: &[&str]) -> String {
        let output = std::process::Command::new("systemctl").args(args).output();
        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout.is_empty() {
                    "unknown".to_string()
                } else {
                    stdout
                }
            }
            Err(_) => "unknown".to_string(),
        }
    }
    if !systemctl_available() {
        return SystemdStatus {
            active: "unknown".to_string(),
            enabled: "unknown".to_string(),
        };
    }
    SystemdStatus {
        active: run_status(&["is-active", service_name]),
        enabled: run_status(&["is-enabled", service_name]),
    }
}

fn query_systemd_status() -> SystemdStatus {
    query_systemd_service_status("webcodex.service")
}

fn resolve_status_token(opts: &ServerStatusOptions) -> Result<Option<String>, String> {
    if let Some(path) = &opts.token_file {
        let token = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read token file {}: {}", path.display(), e))?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err("--token-file cannot be empty".to_string());
        }
        return Ok(Some(token));
    }
    if let Some(path) = &opts.env_file {
        if !path.exists() {
            if opts.env_file_explicit {
                return Err(format!("env file {} does not exist", path.display()));
            }
        } else if let Some(token) = read_env_file_value(path, "WEBCODEX_TOKEN")? {
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Ok(Some(token));
            }
        }
    }
    if let Ok(token) = std::env::var("WEBCODEX_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(Some(token));
        }
    }
    Ok(None)
}

async fn run_server_status(opts: ServerStatusOptions) -> Result<String, String> {
    let systemd = query_systemd_status();
    let token = resolve_status_token(&opts)?;
    let http = fetch_runtime_status(&opts.url, token.as_deref()).await?;
    let output = http.output.as_ref();
    let auth_enabled = output.and_then(|v| v.get("auth_enabled")).cloned();
    let configured_public_url = output
        .and_then(|v| v.get("configured_public_url"))
        .cloned()
        .unwrap_or(Value::Null);
    let tools_count = output
        .and_then(|v| v.pointer("/tools/count"))
        .and_then(Value::as_u64);
    let agents_online_count = output
        .and_then(|v| v.pointer("/agents/online_count"))
        .and_then(Value::as_u64);
    let server_build = runtime_build_metadata(output);
    let local_build = local_cli_build_metadata();
    let revision_comparison = compare_build_commits(
        local_build.git_commit.as_deref(),
        server_build.git_commit.as_deref(),
    );
    if opts.json {
        let summary = json!({
            "http_reachable": http.reachable,
            "http_status_code": http.status_code,
            "http_content_type": http.content_type,
            "http_error": http.error,
            "service": {
                "active": systemd.active,
                "enabled": systemd.enabled,
            },
            "auth_enabled": auth_enabled.unwrap_or(Value::Null),
            "configured_public_url": configured_public_url,
            "tools": {
                "count": tools_count,
            },
            "agents": {
                "online_count": agents_online_count,
            },
            "server_build": {
                "version": server_build.version,
                "git_commit": server_build.git_commit,
                "git_dirty": server_build.git_dirty,
                "built_at": server_build.built_at,
            },
            "local_cli_build": {
                "version": local_build.version,
                "git_commit": local_build.git_commit,
                "git_dirty": local_build.git_dirty,
                "built_at": local_build.built_at,
            },
            "revision_check": server_status_revision_check(&revision_comparison),
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    out.push_str("Server status:\n\n");
    out.push_str(&format!(
        "  HTTP reachable:        {}\n",
        if http.reachable { "yes" } else { "no" }
    ));
    if !http.reachable {
        if let Some(code) = http.status_code {
            out.push_str(&format!("  HTTP status:           {}\n", code));
        }
        if let Some(content_type) = &http.content_type {
            out.push_str(&format!("  HTTP content-type:     {}\n", content_type));
        }
        if let Some(error) = &http.error {
            out.push_str(&format!("  HTTP error:            {}\n", error));
        }
    }
    out.push_str(&format!("  service active:        {}\n", systemd.active));
    out.push_str(&format!("  service enabled:       {}\n", systemd.enabled));
    out.push_str(&format!(
        "  auth_enabled:          {}\n",
        auth_enabled
            .as_ref()
            .map(Value::to_string)
            .unwrap_or_else(|| "unknown".to_string())
    ));
    out.push_str(&format!(
        "  configured_public_url: {}\n",
        if configured_public_url.is_null() {
            "null".to_string()
        } else {
            configured_public_url
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| configured_public_url.to_string())
        }
    ));
    out.push_str(&format!(
        "  tools.count:           {}\n",
        tools_count
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ));
    out.push_str(&format!(
        "  agents.online_count:   {}\n",
        agents_online_count
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ));
    out.push('\n');
    out.push_str(&render_build_metadata_block("Server build", &server_build));
    out.push('\n');
    out.push_str(&render_build_metadata_block(
        "Local CLI build",
        &local_build,
    ));
    out.push('\n');
    out.push_str("Revision check:\n");
    out.push_str(&format!(
        "  {}\n",
        server_status_revision_check(&revision_comparison)
    ));
    Ok(out)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    match cli_action(std::env::args().skip(1)) {
        CliAction::Admin(cmd) => match run_admin_command(cmd).await {
            Ok(stdout) => {
                println!("{}", stdout);
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::TokenGenerate(opts) => {
            print!("{}", render_token_generate(opts));
            std::process::exit(0);
        }
        CliAction::TokenCreateLocal(opts) => match run_token_create_local(opts).await {
            Ok(stdout) => {
                print!("{}", stdout);
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::AgentTokenCreateLocal(opts) => match run_agent_token_create_local(opts).await {
            Ok(stdout) => {
                print!("{}", stdout);
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::AgentInit(opts) => match run_agent_init(opts) {
            Ok(stdout) => {
                print!("{}", stdout);
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::SetupSingleUser(opts) => match run_setup_single_user(opts).await {
            Ok(stdout) => {
                println!("{}", stdout);
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::PairingCreate(opts) => match run_pairing_create(opts).await {
            Ok(stdout) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::ClientEnroll(opts) => match run_client_enroll(opts).await {
            Ok(stdout) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::Doctor(opts) => match run_doctor(opts.clone()).await {
            Ok((stdout, has_fail)) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(if opts.strict && has_fail { 1 } else { 0 });
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::AgentInstallService(opts) => match run_agent_install_service(opts) {
            Ok(stdout) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::AgentStatus(opts) => match run_agent_status(opts).await {
            Ok(stdout) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::ServerInit(opts) => match run_server_init(opts) {
            Ok(stdout) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::ServerUp(opts) => match run_server_up(opts) {
            Ok(stdout) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::Connect(opts) => match run_connect(opts) {
            Ok(stdout) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::ServerInstallService(opts) => match run_server_install_service(opts) {
            Ok(stdout) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::ServerStatus(opts) => match run_server_status(opts).await {
            Ok(stdout) => {
                print!("{}", stdout);
                if !stdout.ends_with('\n') {
                    println!();
                }
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        CliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            std::process::exit(code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    fn build_metadata(commit: Option<&str>) -> RuntimeBuildMetadata {
        RuntimeBuildMetadata {
            version: Some("0.1.0".to_string()),
            git_commit: commit.map(str::to_string),
            git_dirty: Some(false),
            built_at: Some("1782739890".to_string()),
        }
    }

    #[test]
    fn build_revision_compare_matches_same_commit() {
        assert_eq!(
            compare_build_commits(Some("81f322d5b580"), Some("81f322d5b580")),
            RevisionComparison::Match
        );
    }

    #[test]
    fn build_revision_compare_matches_prefix_commit() {
        assert_eq!(
            compare_build_commits(Some("81f322d5b580"), Some("81f322d")),
            RevisionComparison::Match
        );
        assert_eq!(
            compare_build_commits(Some("81f322d"), Some("81f322d5b580")),
            RevisionComparison::Match
        );
    }

    #[test]
    fn build_revision_compare_reports_mismatch() {
        assert_eq!(
            compare_build_commits(Some("81f322d5b580"), Some("fd156ba92fc7")),
            RevisionComparison::Mismatch {
                local: "81f322d5b580".to_string(),
                remote: "fd156ba92fc7".to_string(),
            }
        );
    }

    #[test]
    fn build_revision_compare_reports_unknown() {
        assert!(matches!(
            compare_build_commits(Some("81f322d5b580"), Some("unknown")),
            RevisionComparison::Unknown { reason } if reason.contains("server runtime did not report")
        ));
        assert!(matches!(
            compare_build_commits(Some(""), Some("81f322d5b580")),
            RevisionComparison::Unknown { reason } if reason.contains("local CLI did not report")
        ));
    }

    #[test]
    fn server_status_includes_remote_build_metadata() {
        let output = json!({
            "version": "0.1.0",
            "build": {
                "git_commit": "81f322d5b580",
                "git_dirty": false,
                "built_at": "1782739890"
            }
        });
        let build = runtime_build_metadata(Some(&output));
        let rendered = render_build_metadata_block("Server build", &build);
        assert!(rendered.contains("Server build:"));
        assert!(rendered.contains("version:    0.1.0"));
        assert!(rendered.contains("commit:     81f322d5b580"));
        assert!(rendered.contains("dirty:      false"));
        assert!(rendered.contains("built_at:   1782739890"));
    }

    #[test]
    fn server_status_reports_revision_match() {
        let local = build_metadata(Some("81f322d5b580"));
        let remote = build_metadata(Some("81f322d"));
        let comparison =
            compare_build_commits(local.git_commit.as_deref(), remote.git_commit.as_deref());
        assert_eq!(comparison, RevisionComparison::Match);
        assert!(server_status_revision_check(&comparison).starts_with("ok:"));
    }

    #[test]
    fn server_status_reports_revision_mismatch() {
        let comparison = compare_build_commits(Some("81f322d5b580"), Some("fd156ba92fc7"));
        let detail = server_status_revision_check(&comparison);
        assert!(detail.starts_with("warning:"));
        assert!(detail.contains("local CLI commit 81f322d5b580"));
        assert!(detail.contains("server runtime commit fd156ba92fc7"));
        assert!(detail.contains("deploy/update one side before debugging old behavior"));
    }

    #[test]
    fn server_status_handles_missing_remote_build_metadata() {
        let output = json!({"version":"0.1.0"});
        let remote = runtime_build_metadata(Some(&output));
        let comparison = compare_build_commits(Some("81f322d5b580"), remote.git_commit.as_deref());
        let detail = server_status_revision_check(&comparison);
        assert!(detail.starts_with("unknown:"));
        assert!(detail.contains("server runtime did not report build.git_commit"));
        assert!(detail.contains("server may be older than build metadata support"));
    }

    #[test]
    fn doctor_revision_check_passes_when_commits_match() {
        let local = build_metadata(Some("81f322d5b580"));
        let remote = build_metadata(Some("81f322d"));
        let check = doctor_revision_check(&local, Some(&remote));
        assert_eq!(check.status, "PASS");
        assert_eq!(check.name, "cli/server revision");
        assert!(check
            .detail
            .contains("local CLI and server runtime commit match"));
    }

    #[test]
    fn doctor_revision_check_warns_when_commits_differ() {
        let local = build_metadata(Some("81f322d5b580"));
        let remote = build_metadata(Some("fd156ba92fc7"));
        let check = doctor_revision_check(&local, Some(&remote));
        assert_eq!(check.status, "WARN");
        assert!(check.detail.contains("local CLI commit 81f322d5b580"));
        assert!(check.detail.contains("server runtime commit fd156ba92fc7"));
        assert!(check
            .detail
            .contains("deploy/update one side before debugging old behavior"));
    }

    #[test]
    fn doctor_revision_check_warns_when_server_build_missing() {
        let local = build_metadata(Some("81f322d5b580"));
        let remote = build_metadata(None);
        let check = doctor_revision_check(&local, Some(&remote));
        assert_eq!(check.status, "WARN");
        assert!(check
            .detail
            .contains("server runtime did not report build.git_commit"));
        assert!(check
            .detail
            .contains("server may be older than build metadata support"));
    }

    #[test]
    fn doctor_revision_check_skips_without_runtime_status_credentials() {
        let local = build_metadata(Some("81f322d5b580"));
        let check = doctor_revision_check(&local, None);
        assert_eq!(check.status, "WARN");
        assert_eq!(check.name, "cli/server revision");
        assert!(check.detail.contains("not checked; pass --server-url"));
        assert!(check.detail.contains("--user-token-file or --token-file"));
    }

    #[test]
    fn cli_help_and_version_exit_before_dispatch() {
        match cli_action(["--help"]) {
            CliAction::Exit { code, stdout, .. } => {
                assert_eq!(code, 0);
                assert!(stdout.contains("Usage: webcodex-cli"));
            }
            other => panic!("expected help exit, got {other:?}"),
        }
        match cli_action(["--version"]) {
            CliAction::Exit { code, stdout, .. } => {
                assert_eq!(code, 0);
                assert!(stdout.starts_with(&format!(
                    "webcodex-cli {} (commit ",
                    env!("CARGO_PKG_VERSION")
                )));
                assert!(stdout.trim_end().ends_with(')'));
                assert_ne!(
                    stdout,
                    format!("webcodex-cli {}\n", env!("CARGO_PKG_VERSION"))
                );
            }
            other => panic!("expected version exit, got {other:?}"),
        }
    }

    #[test]
    fn cli_version_output_includes_build_metadata() {
        match cli_action(["-V"]) {
            CliAction::Exit {
                code,
                stdout,
                stderr,
            } => {
                assert_eq!(code, 0);
                assert!(stdout.contains("commit "));
                assert!(stdout.starts_with("webcodex-cli "));
                assert!(stderr.is_empty());
            }
            other => panic!("expected version exit, got {other:?}"),
        }
    }

    #[test]
    fn webcodex_cli_help_mentions_pairing_client_and_doctor() {
        match cli_action(["--help"]) {
            CliAction::Exit { code, stdout, .. } => {
                assert_eq!(code, 0);
                assert!(stdout.contains("pairing create"));
                assert!(stdout.contains("client enroll"));
                assert!(stdout.contains("doctor"));
                assert!(stdout.contains("token generate"));
                assert!(stdout.contains("token create-local"));
                assert!(stdout.contains("token register-hash"));
                assert!(stdout.contains("agent-token create-local"));
                assert!(stdout.contains("agent-token register-hash"));
                assert!(stdout.contains("agent init/install-service/status"));
            }
            other => panic!("expected help exit, got {other:?}"),
        }
    }

    #[test]
    fn webcodex_cli_agent_help_mentions_new_subcommands() {
        match cli_action(["agent", "--help"]) {
            CliAction::Exit { code, stdout, .. } => {
                assert_eq!(code, 0);
                assert!(stdout.contains("install-service"));
                assert!(stdout.contains("status"));
                assert!(stdout.contains("init"));
            }
            other => panic!("expected help exit, got {other:?}"),
        }
        match cli_action(["agent", "install-service", "--help"]) {
            CliAction::Exit { code, stdout, .. } => {
                assert_eq!(code, 0);
                assert!(stdout.contains("--config PATH"));
                assert!(stdout.contains("--bin PATH"));
                assert!(stdout.contains("Tokens are never inlined"));
            }
            other => panic!("expected help exit, got {other:?}"),
        }
        match cli_action(["agent", "status", "--help"]) {
            CliAction::Exit { code, stdout, .. } => {
                assert_eq!(code, 0);
                assert!(stdout.contains("--user-token-file PATH"));
                assert!(stdout.contains("--agent-token-file PATH"));
                assert!(stdout.contains("no tokens"));
            }
            other => panic!("expected help exit, got {other:?}"),
        }
    }

    #[test]
    fn users_create_builds_admin_request_via_admin_cli() {
        // webcodex-cli users create ... reuses admin_cli parsing.
        let action = cli_action(args(&[
            "users",
            "create",
            "--server-url",
            "https://example.test/",
            "--token",
            "fake-admin",
            "--username",
            "alice",
            "--display-name",
            "Alice",
            "--role",
            "user",
        ]));
        match action {
            CliAction::Admin(AdminCliCommand::UsersCreate(opts, user)) => {
                let req = build_admin_request(&AdminCliCommand::UsersCreate(opts, user)).unwrap();
                assert_eq!(req.server_url, "https://example.test");
                assert_eq!(req.path, "/api/users/create");
                assert_eq!(req.body["username"], "alice");
                assert_eq!(req.body["role"], "user");
            }
            other => panic!("expected Admin, got {other:?}"),
        }
    }

    #[test]
    fn user_create_issue_credential_sets_request_field() {
        let action = cli_action(args(&[
            "user",
            "create",
            "--server",
            "https://example.test/",
            "--admin-token",
            "fake-admin",
            "--username",
            "alice",
            "--issue-credential",
        ]));
        match action {
            CliAction::Admin(AdminCliCommand::UsersCreate(opts, user)) => {
                let req = build_admin_request(&AdminCliCommand::UsersCreate(opts, user)).unwrap();
                assert_eq!(req.path, "/api/users/create");
                assert_eq!(req.token, "fake-admin");
                assert_eq!(req.body["issue_credential"], true);
            }
            other => panic!("expected UsersCreate, got {other:?}"),
        }
    }

    #[test]
    fn token_generate_api_prints_token_hash_and_prefix() {
        let action = cli_action(args(&["token", "generate", "--kind", "api"]));
        match action {
            CliAction::TokenGenerate(opts) => {
                let out = render_token_generate(opts);
                assert!(out.contains("Token:\nwc_pat_"));
                assert!(out.contains("\nHash:\nsha256:"));
                assert!(out.contains("\nPrefix:\nwc_pat_"));
            }
            other => panic!("expected TokenGenerate, got {other:?}"),
        }
    }

    #[test]
    fn token_generate_agent_prints_token_hash_and_prefix() {
        let action = cli_action(args(&["token", "generate", "--kind", "agent"]));
        match action {
            CliAction::TokenGenerate(opts) => {
                let out = render_token_generate(opts);
                assert!(out.contains("Token:\nwc_agent_"));
                assert!(out.contains("\nHash:\nsha256:"));
                assert!(out.contains("\nPrefix:\nwc_agent_"));
            }
            other => panic!("expected TokenGenerate, got {other:?}"),
        }
    }

    #[test]
    fn credential_resolution_priority_is_explicit_then_env_name_then_default_env() {
        let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
        std::env::set_var("CUSTOM_ACCT", "wc_acct_custom");
        assert_eq!(
            resolve_account_credential(&Some("wc_acct_explicit".to_string()), &None).unwrap(),
            "wc_acct_explicit"
        );
        assert_eq!(
            resolve_account_credential(&None, &Some("CUSTOM_ACCT".to_string())).unwrap(),
            "wc_acct_custom"
        );
        assert_eq!(
            resolve_account_credential(&None, &None).unwrap(),
            "wc_acct_default"
        );
        std::env::remove_var("WEBCODEX_ACCOUNT_CREDENTIAL");
        std::env::remove_var("CUSTOM_ACCT");
    }

    #[test]
    fn token_register_hash_builds_hash_registration_request() {
        let action = cli_action(args(&[
            "token",
            "register-hash",
            "--server",
            "https://example.test",
            "--user",
            "alice",
            "--credential",
            "wc_acct_fake",
            "--name",
            "gpt-action",
            "--hash",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--prefix",
            "wc_pat_aaaaaaaa",
            "--scopes",
            "runtime:read,project:read",
        ]));
        match action {
            CliAction::Admin(AdminCliCommand::TokensRegisterHash(opts, t)) => {
                let req =
                    build_admin_request(&AdminCliCommand::TokensRegisterHash(opts, t)).unwrap();
                assert_eq!(req.path, "/api/tokens/register_hash");
                assert_eq!(req.token, "wc_acct_fake");
                assert_eq!(req.body["username"], "alice");
                assert_eq!(req.body["name"], "gpt-action");
                assert_eq!(req.body["token_prefix"], "wc_pat_aaaaaaaa");
                assert_eq!(req.body["scopes"], json!(["runtime:read", "project:read"]));
            }
            other => panic!("expected TokensRegisterHash, got {other:?}"),
        }
    }

    #[test]
    fn agent_token_register_hash_builds_hash_registration_request() {
        let action = cli_action(args(&[
            "agent-token",
            "register-hash",
            "--server",
            "https://example.test",
            "--user",
            "alice",
            "--credential",
            "wc_acct_fake",
            "--client-id",
            "alice-laptop",
            "--name",
            "alice laptop",
            "--hash",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--prefix",
            "wc_agent_aaaaaaa",
            "--scopes",
            "agent:register,agent:poll",
        ]));
        match action {
            CliAction::Admin(AdminCliCommand::AgentTokensRegisterHash(opts, t)) => {
                let req = build_admin_request(&AdminCliCommand::AgentTokensRegisterHash(opts, t))
                    .unwrap();
                assert_eq!(req.path, "/api/agent-tokens/register_hash");
                assert_eq!(req.token, "wc_acct_fake");
                assert_eq!(req.body["username"], "alice");
                assert_eq!(req.body["client_id"], "alice-laptop");
                assert_eq!(req.body["name"], "alice laptop");
                assert_eq!(req.body["token_prefix"], "wc_agent_aaaaaaa");
                assert_eq!(req.body["scopes"], json!(["agent:register", "agent:poll"]));
                assert!(req.body.get("token").is_none());
            }
            other => panic!("expected AgentTokensRegisterHash, got {other:?}"),
        }
    }

    #[test]
    fn tokens_and_agent_tokens_commands_parse_to_admin() {
        let action = cli_action(args(&[
            "tokens",
            "create",
            "--server-url",
            "https://example.test",
            "--token",
            "fake-admin",
            "--username",
            "alice",
            "--name",
            "chatgpt-action",
            "--scope",
            "runtime:read",
            "--scope",
            "project:write",
        ]));
        assert!(matches!(
            action,
            CliAction::Admin(AdminCliCommand::TokensCreate(_, _))
        ));

        let action = cli_action(args(&[
            "agent-tokens",
            "create",
            "--server-url",
            "https://example.test",
            "--token",
            "fake-admin",
            "--username",
            "alice",
            "--client-id",
            "alice-laptop",
        ]));
        match action {
            CliAction::Admin(AdminCliCommand::AgentTokensCreate(_, t)) => {
                // Default agent scopes applied.
                assert_eq!(
                    t.scopes,
                    SETUP_AGENT_SCOPES
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
            }
            other => panic!("expected AgentTokensCreate, got {other:?}"),
        }

        let list = cli_action(args(&[
            "agent-tokens",
            "list",
            "--server-url",
            "https://example.test",
            "--token",
            "fake-admin",
            "--username",
            "alice",
        ]));
        assert!(matches!(
            list,
            CliAction::Admin(AdminCliCommand::AgentTokensList(_, _))
        ));

        let revoke = cli_action(args(&[
            "tokens",
            "revoke",
            "--server-url",
            "https://example.test",
            "--token",
            "fake-admin",
            "--username",
            "alice",
            "--token-id",
            "tok-1",
        ]));
        assert!(matches!(
            revoke,
            CliAction::Admin(AdminCliCommand::TokensRevoke(_, _))
        ));
    }

    #[tokio::test]
    async fn token_create_local_does_not_send_plaintext_token_to_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            assert!(request.starts_with("POST /api/tokens/register_hash "));
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer wc_acct_fake"));
            assert!(request.contains(r#""token_hash":"sha256:"#));
            assert!(request.contains(r#""token_prefix":"wc_pat_"#));
            assert!(!request.contains(r#""token":"wc_pat_"#));
            let body = r#"{"success":true,"token":{"id":"tok-1","token_prefix":"wc_pat_fake"}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let out = run_token_create_local(TokenCreateLocalOptions {
            server_url: format!("http://{}", addr),
            username: "alice".to_string(),
            credential: Some("wc_acct_fake".to_string()),
            credential_env: None,
            name: Some("gpt-action".to_string()),
            scopes: SETUP_GPT_SCOPES.iter().map(|s| s.to_string()).collect(),
        })
        .await
        .unwrap();
        assert_eq!(out.matches("wc_pat_").count(), 1);
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn agent_token_create_local_does_not_send_plaintext_token_to_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            assert!(request.starts_with("POST /api/agent-tokens/register_hash "));
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer wc_acct_fake"));
            assert!(request.contains(r#""token_hash":"sha256:"#));
            assert!(request.contains(r#""token_prefix":"wc_agent_"#));
            assert!(request.contains(r#""client_id":"alice-laptop""#));
            assert!(!request.contains(r#""token":"wc_agent_"#));
            let body = r#"{"success":true,"token":{"id":"tok-1","token_prefix":"wc_agent_fake","allowed_client_id":"alice-laptop"}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let out = run_agent_token_create_local(AgentTokenCreateLocalOptions {
            admin: AdminOptions {
                server_url: format!("http://{}", addr),
                credential: Some("wc_acct_fake".to_string()),
                ..AdminOptions::default()
            },
            username: "alice".to_string(),
            client_id: "alice-laptop".to_string(),
            name: Some("alice laptop".to_string()),
            scopes: SETUP_AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
        })
        .await
        .unwrap();
        assert!(out.contains("Agent token created locally and registered with server."));
        assert!(out.contains("Client ID:\nalice-laptop"));
        assert_eq!(out.matches("wc_agent_").count(), 1);
        handle.join().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_token_create_local_prefers_admin_token_over_default_account_credential() {
        let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer fake-admin"));
            let body = r#"{"success":true,"token":{"id":"tok-1","token_prefix":"wc_agent_fake","allowed_client_id":"alice-laptop"}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let out = run_agent_token_create_local(AgentTokenCreateLocalOptions {
            admin: AdminOptions {
                server_url: format!("http://{}", addr),
                token: Some("fake-admin".to_string()),
                ..AdminOptions::default()
            },
            username: "alice".to_string(),
            client_id: "alice-laptop".to_string(),
            name: None,
            scopes: SETUP_AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
        })
        .await
        .unwrap();
        assert_eq!(out.matches("wc_agent_").count(), 1);
        handle.join().unwrap();
        std::env::remove_var("WEBCODEX_ACCOUNT_CREDENTIAL");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_token_create_local_uses_default_account_credential() {
        let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer wc_acct_default"));
            let body = r#"{"success":true,"token":{"id":"tok-1","token_prefix":"wc_agent_fake","allowed_client_id":"alice-laptop"}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let out = run_agent_token_create_local(AgentTokenCreateLocalOptions {
            admin: AdminOptions {
                server_url: format!("http://{}", addr),
                ..AdminOptions::default()
            },
            username: "alice".to_string(),
            client_id: "alice-laptop".to_string(),
            name: None,
            scopes: SETUP_AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
        })
        .await
        .unwrap();
        assert_eq!(out.matches("wc_agent_").count(), 1);
        handle.join().unwrap();
        std::env::remove_var("WEBCODEX_ACCOUNT_CREDENTIAL");
    }

    #[test]
    fn agent_init_writes_valid_toml_and_refuses_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("agent.toml");
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://v4.example.test/",
            "--token",
            "agent_fake_test_token",
            "--client-id",
            "alice-laptop",
            "--owner",
            "alice",
            "--display-name",
            "Alice Laptop",
            "--allowed-root",
            "/srv/projects",
            "--output",
            output.to_str().unwrap(),
        ]))
        .unwrap();
        let msg = run_agent_init(opts).unwrap();
        assert!(msg.contains("agent.toml"));

        // Refuse overwrite without --overwrite.
        let opts2 = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://v4.example.test/",
            "--token",
            "agent_fake_test_token",
            "--client-id",
            "alice-laptop",
            "--owner",
            "alice",
            "--allowed-root",
            "/srv/projects",
            "--output",
            output.to_str().unwrap(),
        ]))
        .unwrap();
        let err = run_agent_init(opts2).unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[test]
    fn agent_init_stdout_output_contains_token_only_once() {
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://v4.example.test",
            "--token",
            "agent_fake_stdout_token",
            "--client-id",
            "alice-laptop",
            "--owner",
            "alice",
            "--allowed-root",
            "/srv/projects",
            "--output",
            "-",
        ]))
        .unwrap();
        let content = run_agent_init(opts).unwrap();
        assert_eq!(content.matches("agent_fake_stdout_token").count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn agent_init_writes_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("agent.toml");
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://v4.example.test",
            "--token",
            "agent_fake_perms_token",
            "--client-id",
            "alice-laptop",
            "--owner",
            "alice",
            "--allowed-root",
            "/srv/projects",
            "--output",
            output.to_str().unwrap(),
        ]))
        .unwrap();
        run_agent_init(opts).unwrap();
        let mode = std::fs::metadata(&output).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn agent_init_token_file_and_env_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let token_file = tmp.path().join("agent.token");
        std::fs::write(&token_file, "agent_fake_file_token\n").unwrap();
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://v4.example.test",
            "--token-file",
            token_file.to_str().unwrap(),
            "--client-id",
            "alice-laptop",
            "--owner",
            "alice",
            "--allowed-root",
            "/srv/projects",
            "--output",
            "-",
        ]))
        .unwrap();
        let content = run_agent_init(opts).unwrap();
        assert!(content.contains("agent_fake_file_token"));

        let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_AGENT_TOKEN", "agent_fake_env_token");
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://v4.example.test",
            "--client-id",
            "alice-laptop",
            "--owner",
            "alice",
            "--allowed-root",
            "/srv/projects",
            "--output",
            "-",
        ]))
        .unwrap();
        let content = run_agent_init(opts).unwrap();
        assert!(content.contains("agent_fake_env_token"));
        std::env::remove_var("WEBCODEX_AGENT_TOKEN");
    }

    #[test]
    fn agent_init_empty_tokens_are_rejected() {
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://v4.example.test",
            "--token",
            "   ",
            "--client-id",
            "alice-laptop",
            "--owner",
            "alice",
            "--allowed-root",
            "/srv/projects",
            "--output",
            "-",
        ]))
        .unwrap();
        let err = run_agent_init(opts).unwrap_err();
        assert!(err.contains("--token cannot be empty"), "{err}");
    }

    #[test]
    fn agent_init_allows_empty_allowed_roots_with_home_default() {
        let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
        let home = std::env::var_os("HOME");
        if home.is_some() {
            let opts = parse_cli_agent_init(&args(&[
                "--server-url",
                "https://v4.example.test",
                "--token",
                "agent_fake_home_token",
                "--client-id",
                "alice-laptop",
                "--owner",
                "alice",
                "--output",
                "-",
            ]))
            .unwrap();
            let content = run_agent_init(opts).unwrap();
            let home = std::env::var_os("HOME").unwrap();
            assert!(content.contains(&home.to_string_lossy().to_string()));
        }
    }

    #[test]
    fn setup_single_user_parse_validates_required_fields() {
        let err = parse_setup_single_user(&args(&[
            "--server-url",
            "https://example.test",
            "--token",
            "fake-bootstrap",
            "--username",
            "yyjeqhc",
            // missing --client-id and --output-dir
        ]))
        .unwrap_err();
        assert!(err.contains("--client-id is required"));
    }

    #[test]
    fn setup_single_user_parse_defaults() {
        let opts = parse_setup_single_user(&args(&[
            "--server-url",
            "https://example.test",
            "--token",
            "fake-bootstrap",
            "--username",
            "yyjeqhc",
            "--client-id",
            "oe",
            "--output-dir",
            "/tmp/webcodex-setup-test",
        ]))
        .unwrap();
        assert_eq!(opts.role, "admin");
        assert_eq!(opts.gpt_token_name, "chatgpt-action");
        assert_eq!(opts.agent_token_name, "oe agent");
    }

    #[test]
    fn pairing_create_parse_defaults() {
        let opts = parse_pairing_create(&args(&[
            "--server-url",
            "https://example.test",
            "--token-file",
            "/tmp/webcodex-token",
            "--username",
            "alice",
            "--client-id",
            "alice-laptop",
        ]))
        .unwrap();
        assert_eq!(opts.ttl_secs, 600);
        assert_eq!(opts.username, "alice");
        assert_eq!(opts.client_id, "alice-laptop");
        assert_eq!(opts.token_file, Some(PathBuf::from("/tmp/webcodex-token")));
    }

    #[test]
    fn pairing_create_missing_env_file_error_includes_server_admin_guidance() {
        let opts = PairingCreateOptions {
            server_url: "https://example.test".to_string(),
            env_file: Some(PathBuf::from("/tmp/webcodex-missing-server-env-file")),
            username: "alice".to_string(),
            client_id: "alice-laptop".to_string(),
            ttl_secs: 600,
            ..PairingCreateOptions::default()
        };
        let err = resolve_pairing_create_token(&opts).unwrap_err();
        assert!(err.contains("failed to read server env file"));
        assert!(err.contains("pairing create is a server/admin-side command"));
        assert!(err.contains("Run it on the server or pass a server/admin token file"));
    }

    #[test]
    fn client_output_dir_for_profile_uses_clients_subdir() {
        let base = PathBuf::from("/tmp/wc-base");
        assert_eq!(
            client_output_dir_for_profile(&base, "alice-laptop"),
            PathBuf::from("/tmp/wc-base/clients/alice-laptop")
        );
    }

    #[test]
    fn client_enroll_parse_defaults_to_client_id_profile() {
        let opts = parse_client_enroll(&args(&[
            "--server-url",
            "https://example.test",
            "--pairing-code",
            "wc_pair_fake",
            "--client-id",
            "alice-laptop",
        ]))
        .unwrap();
        let default_dir = default_client_output_dir_for_profile("alice-laptop");
        assert_eq!(opts.output_dir, default_dir);
        assert_eq!(opts.agent_config, opts.output_dir.join("agent.toml"));
        assert_eq!(opts.projects_dir, opts.output_dir.join("projects.d"));
        assert_eq!(opts.transport, TRANSPORT_WEBSOCKET);
        assert!(!opts.overwrite);
    }

    #[test]
    fn client_enroll_parse_uses_explicit_profile_for_default_output_dir() {
        let opts = parse_client_enroll(&args(&[
            "--server-url",
            "https://example.test",
            "--pairing-code",
            "wc_pair_fake",
            "--client-id",
            "alice-laptop",
            "--profile",
            "special",
        ]))
        .unwrap();
        assert_eq!(
            opts.output_dir,
            default_client_output_dir_for_profile("special")
        );
        assert_eq!(opts.agent_config, opts.output_dir.join("agent.toml"));
        assert_eq!(opts.projects_dir, opts.output_dir.join("projects.d"));
    }

    #[test]
    fn client_enroll_parse_output_dir_overrides_profile_default() {
        let opts = parse_client_enroll(&args(&[
            "--server-url",
            "https://example.test",
            "--pairing-code",
            "wc_pair_fake",
            "--client-id",
            "alice-laptop",
            "--profile",
            "special",
            "--output-dir",
            "/tmp/wc",
        ]))
        .unwrap();
        assert_eq!(opts.output_dir, PathBuf::from("/tmp/wc"));
        assert_eq!(opts.agent_config, PathBuf::from("/tmp/wc/agent.toml"));
        assert_eq!(opts.projects_dir, PathBuf::from("/tmp/wc/projects.d"));
    }

    #[test]
    fn client_enroll_parse_output_dir_does_not_derive_profile_from_client_id() {
        let opts = parse_client_enroll(&args(&[
            "--server-url",
            "https://example.test",
            "--pairing-code",
            "wc_pair_fake",
            "--client-id",
            "alice laptop",
            "--output-dir",
            "/tmp/wc",
        ]))
        .unwrap();
        assert_eq!(opts.output_dir, PathBuf::from("/tmp/wc"));
        assert_eq!(opts.agent_config, PathBuf::from("/tmp/wc/agent.toml"));
        assert_eq!(opts.projects_dir, PathBuf::from("/tmp/wc/projects.d"));
    }

    #[test]
    fn client_enroll_parse_agent_config_and_projects_dir_override_defaults() {
        let opts = parse_client_enroll(&args(&[
            "--server-url",
            "https://example.test",
            "--pairing-code",
            "wc_pair_fake",
            "--client-id",
            "alice-laptop",
            "--profile",
            "special",
            "--agent-config",
            "/tmp/custom-agent.toml",
            "--projects-dir",
            "/tmp/custom-projects.d",
        ]))
        .unwrap();
        assert_eq!(
            opts.output_dir,
            default_client_output_dir_for_profile("special")
        );
        assert_eq!(opts.agent_config, PathBuf::from("/tmp/custom-agent.toml"));
        assert_eq!(opts.projects_dir, PathBuf::from("/tmp/custom-projects.d"));
    }

    #[test]
    fn client_enroll_rejects_unsafe_profiles() {
        for profile in [
            "",
            "   ",
            ".",
            "..",
            "../x",
            "a/b",
            r"a\b",
            "has space",
            "ümlaut",
        ] {
            let err = parse_client_enroll(&args(&[
                "--server-url",
                "https://example.test",
                "--pairing-code",
                "wc_pair_fake",
                "--client-id",
                "alice-laptop",
                "--profile",
                profile,
            ]))
            .unwrap_err();
            assert_eq!(err, CLIENT_PROFILE_ERROR);
        }
    }

    #[test]
    fn client_enroll_rejects_unsafe_default_client_id_profile() {
        let err = parse_client_enroll(&args(&[
            "--server-url",
            "https://example.test",
            "--pairing-code",
            "wc_pair_fake",
            "--client-id",
            "alice laptop",
        ]))
        .unwrap_err();
        assert_eq!(err, CLIENT_PROFILE_ERROR);
    }

    #[test]
    fn client_enroll_help_documents_profile_and_output_dir_precedence() {
        let help = client_enroll_usage();
        assert!(help.contains("--profile NAME"));
        assert!(help.contains("/etc/webcodex/clients/<profile>"));
        assert!(help.contains("~/.config/webcodex/clients/<profile>"));
        assert!(help.contains("Explicit --output-dir overrides"));
    }

    #[test]
    fn agent_init_defaults_to_client_id_profile_paths() {
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://example.test",
            "--token",
            "agent_fake_token",
            "--client-id",
            "special-container",
            "--owner",
            "alice",
        ]))
        .unwrap();
        assert_eq!(
            opts.output,
            client_profile_agent_config("special-container")
        );
        assert_eq!(
            opts.projects_dir,
            client_profile_projects_dir("special-container")
        );
    }

    #[test]
    fn agent_init_profile_overrides_client_id_profile_paths() {
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://example.test",
            "--token",
            "agent_fake_token",
            "--client-id",
            "special-container",
            "--profile",
            "special",
            "--owner",
            "alice",
        ]))
        .unwrap();
        assert_eq!(opts.output, client_profile_agent_config("special"));
        assert_eq!(opts.projects_dir, client_profile_projects_dir("special"));
    }

    #[test]
    fn agent_init_explicit_output_and_projects_dir_win() {
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://example.test",
            "--token",
            "agent_fake_token",
            "--client-id",
            "special-container",
            "--profile",
            "special",
            "--owner",
            "alice",
            "--output",
            "/tmp/a.toml",
            "--projects-dir",
            "/tmp/projects.d",
        ]))
        .unwrap();
        assert_eq!(opts.output, PathBuf::from("/tmp/a.toml"));
        assert_eq!(opts.projects_dir, PathBuf::from("/tmp/projects.d"));
    }

    #[test]
    fn agent_init_explicit_output_without_profile_preserves_legacy_projects_dir() {
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://example.test",
            "--token",
            "agent_fake_token",
            "--client-id",
            "client id with spaces",
            "--owner",
            "alice",
            "--output",
            "/tmp/a.toml",
        ]))
        .unwrap();
        assert_eq!(opts.output, PathBuf::from("/tmp/a.toml"));
        assert_eq!(opts.projects_dir, PathBuf::from(DEFAULT_INIT_PROJECTS_DIR));
    }

    #[test]
    fn agent_init_rejects_unsafe_profile() {
        let err = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://example.test",
            "--token",
            "agent_fake_token",
            "--client-id",
            "special-container",
            "--profile",
            "../x",
            "--owner",
            "alice",
        ]))
        .unwrap_err();
        assert_eq!(err, CLIENT_PROFILE_ERROR);
    }

    #[test]
    fn agent_status_profile_derives_config_and_token_paths() {
        let opts = parse_agent_status(&args(&["--profile", "special"])).unwrap();
        assert_eq!(opts.config, client_profile_agent_config("special"));
        assert_eq!(
            opts.user_token_file,
            Some(client_profile_user_token_file("special"))
        );
        assert_eq!(
            opts.agent_token_file,
            Some(client_profile_agent_token_file("special"))
        );
    }

    #[test]
    fn agent_status_explicit_paths_win_and_no_profile_keeps_legacy_default() {
        let opts = parse_agent_status(&args(&[
            "--profile",
            "special",
            "--config",
            "/tmp/agent.toml",
            "--user-token-file",
            "/tmp/user-token",
            "--agent-token-file",
            "/tmp/agent-token",
        ]))
        .unwrap();
        assert_eq!(opts.config, PathBuf::from("/tmp/agent.toml"));
        assert_eq!(opts.user_token_file, Some(PathBuf::from("/tmp/user-token")));
        assert_eq!(
            opts.agent_token_file,
            Some(PathBuf::from("/tmp/agent-token"))
        );

        let legacy = parse_agent_status(&args(&[])).unwrap();
        assert_eq!(legacy.config, PathBuf::from("/etc/webcodex/agent.toml"));
        assert_eq!(legacy.user_token_file, None);
        assert_eq!(legacy.agent_token_file, None);
    }

    #[test]
    fn agent_install_service_profile_derives_config_and_service_file() {
        let opts = parse_agent_install_service(&args(&[
            "--profile",
            "special",
            "--bin",
            "/opt/webcodex/bin/webcodex-agent",
            "--dry-run",
        ]))
        .unwrap();
        assert_eq!(opts.config, client_profile_agent_config("special"));
        assert_eq!(opts.service_file, client_profile_service_file("special"));
        let unit = render_agent_systemd_unit(&opts);
        assert!(unit.contains(
            "ExecStart=/opt/webcodex/bin/webcodex-agent --config /etc/webcodex/clients/special/agent.toml"
        ));
    }

    #[test]
    fn agent_install_service_explicit_paths_win_and_rejects_unsafe_profile() {
        let opts = parse_agent_install_service(&args(&[
            "--profile",
            "special",
            "--config",
            "/tmp/agent.toml",
            "--service-file",
            "/tmp/webcodex-agent.service",
            "--bin",
            "/opt/webcodex/bin/webcodex-agent",
        ]))
        .unwrap();
        assert_eq!(opts.config, PathBuf::from("/tmp/agent.toml"));
        assert_eq!(
            opts.service_file,
            PathBuf::from("/tmp/webcodex-agent.service")
        );

        let err = parse_agent_install_service(&args(&[
            "--profile",
            "../x",
            "--bin",
            "/opt/webcodex/bin/webcodex-agent",
        ]))
        .unwrap_err();
        assert_eq!(err, CLIENT_PROFILE_ERROR);
    }

    #[test]
    fn doctor_profile_derives_agent_config_and_token_paths() {
        let opts = parse_doctor(&args(&["--profile", "special"])).unwrap();
        assert_eq!(
            opts.agent_config,
            Some(client_profile_agent_config("special"))
        );
        assert_eq!(
            opts.user_token_file,
            Some(client_profile_user_token_file("special"))
        );
        assert_eq!(
            opts.agent_token_file,
            Some(client_profile_agent_token_file("special"))
        );
    }

    #[test]
    fn doctor_explicit_paths_win_and_no_profile_keeps_legacy_behavior() {
        let opts = parse_doctor(&args(&[
            "--profile",
            "special",
            "--agent-config",
            "/tmp/agent.toml",
            "--user-token-file",
            "/tmp/user-token",
            "--agent-token-file",
            "/tmp/agent-token",
        ]))
        .unwrap();
        assert_eq!(opts.agent_config, Some(PathBuf::from("/tmp/agent.toml")));
        assert_eq!(opts.user_token_file, Some(PathBuf::from("/tmp/user-token")));
        assert_eq!(
            opts.agent_token_file,
            Some(PathBuf::from("/tmp/agent-token"))
        );

        let legacy = parse_doctor(&args(&[])).unwrap();
        assert_eq!(legacy.agent_config, None);
        assert_eq!(legacy.user_token_file, None);
        assert_eq!(legacy.agent_token_file, None);
    }

    #[test]
    fn doctor_rejects_unsafe_profile() {
        let err = parse_doctor(&args(&["--profile", "a/b"])).unwrap_err();
        assert_eq!(err, CLIENT_PROFILE_ERROR);
    }

    #[test]
    fn doctor_parse_quic_flags() {
        let opts = parse_doctor(&args(&[
            "--quic",
            "--server-only",
            "--quic-server-addr",
            "v4.example.test:8443",
            "--quic-server-name",
            "v4.example.test",
            "--quic-alpn",
            "webcodex-agent/1",
            "--quic-timeout-secs",
            "7",
            "--quic-client-id",
            "alice-laptop",
        ]))
        .unwrap();
        assert!(opts.quic);
        assert!(opts.quic_server_only);
        assert!(!opts.quic_agent_e2e);
        assert_eq!(
            opts.quic_server_addr.as_deref(),
            Some("v4.example.test:8443")
        );
        assert_eq!(opts.quic_server_name.as_deref(), Some("v4.example.test"));
        assert_eq!(opts.quic_alpn, "webcodex-agent/1");
        assert_eq!(opts.quic_timeout_secs, 7);
        assert_eq!(opts.quic_client_id.as_deref(), Some("alice-laptop"));
    }

    #[test]
    fn doctor_parse_accepts_quic_and_auto_transport_flags() {
        let opts = parse_client_enroll(&args(&[
            "--server-url",
            "https://v4.example.test",
            "--pairing-code",
            "abc123",
            "--client-id",
            "alice-laptop",
            "--transport",
            agent_init::TRANSPORT_AUTO,
        ]))
        .unwrap();
        assert_eq!(opts.transport, agent_init::TRANSPORT_AUTO);
    }

    #[test]
    fn doctor_runtime_quic_checks_fail_when_disabled_or_listener_failed() {
        let disabled = json!({
            "quic": {
                "enabled": false,
                "listen": "0.0.0.0:8443",
                "alpn": "webcodex-agent/1",
                "listener_started": false,
                "last_error": null
            }
        });
        let (checks, should_continue) = doctor_runtime_quic_checks(&disabled);
        assert!(!should_continue);
        assert!(checks
            .iter()
            .any(|c| c.status == "FAIL" && c.detail.contains("server reports QUIC disabled")));

        let listener_failed = json!({
            "quic": {
                "enabled": true,
                "listen": "0.0.0.0:8443",
                "alpn": "webcodex-agent/1",
                "listener_started": false,
                "last_error": "WEBCODEX_QUIC_KEY path does not exist: /etc/secret/privkey.pem"
            }
        });
        let (checks, should_continue) = doctor_runtime_quic_checks(&listener_failed);
        assert!(!should_continue);
        let detail = checks
            .iter()
            .find(|c| c.name == "quic listener started")
            .unwrap()
            .detail
            .clone();
        assert!(detail.contains("listener not started"));
        assert!(detail.contains("WEBCODEX_QUIC_KEY path does not exist"));
        assert!(!detail.contains("/etc/secret"));
        assert!(!detail.contains("privkey.pem"));
    }

    #[test]
    fn doctor_runtime_quic_checks_warn_for_older_server() {
        let (checks, should_continue) = doctor_runtime_quic_checks(&json!({}));
        assert!(should_continue);
        assert_eq!(checks[0].status, "WARN");
        assert!(checks[0]
            .detail
            .contains("not exposed by this server version"));
    }

    #[test]
    fn doctor_runtime_quic_checks_pass_when_listener_started() {
        let value = json!({
            "quic": {
                "enabled": true,
                "listen": "0.0.0.0:8443",
                "alpn": "webcodex-agent/1",
                "listener_started": true,
                "last_error": null
            }
        });
        let (checks, should_continue) = doctor_runtime_quic_checks(&value);
        assert!(should_continue);
        assert!(checks.iter().any(|c| c.name == "quic runtime config"
            && c.detail.contains("enabled=true listen=0.0.0.0:8443")
            && c.detail.contains("listener_started=true")));
    }

    #[test]
    fn doctor_parse_quic_modes_are_mutually_exclusive() {
        let err = parse_doctor(&args(&["--quic", "--server-only", "--agent-e2e"])).unwrap_err();
        assert!(err.contains("mutually exclusive"));
    }

    #[test]
    fn doctor_quic_options_fall_back_to_agent_config() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("agent.toml");
        std::fs::write(
            &config,
            r#"
server_url = "https://v4.example.test"
token = "redacted"
client_id = "alice-laptop"
transport = "quic"

[quic]
server_addr = "v4.example.test:8443"
server_name = "v4.example.test"
alpn = "webcodex-agent/1"
connect_timeout_secs = 12
"#,
        )
        .unwrap();
        let opts = parse_doctor(&args(&[
            "--quic",
            "--agent-config",
            config.to_str().unwrap(),
        ]))
        .unwrap();
        let resolved = resolve_doctor_quic_options(&opts).unwrap();
        assert_eq!(resolved.server_addr, "v4.example.test:8443");
        assert_eq!(resolved.server_name, "v4.example.test");
        assert_eq!(resolved.alpn, "webcodex-agent/1");
        assert_eq!(resolved.client_id.as_deref(), Some("alice-laptop"));
    }

    #[test]
    fn client_enroll_refuses_overwrite_before_network() {
        let tmp = tempfile::tempdir().unwrap();
        let existing = tmp.path().join("webcodex-user-token");
        std::fs::write(&existing, "old\n").unwrap();
        let opts = ClientEnrollOptions {
            server_url: "http://127.0.0.1:9".to_string(),
            pairing_code: "wc_pair_fake".to_string(),
            client_id: "alice-laptop".to_string(),
            display_name: None,
            transport: TRANSPORT_WEBSOCKET.to_string(),
            output_dir: tmp.path().to_path_buf(),
            agent_config: tmp.path().join("agent.toml"),
            projects_dir: tmp.path().join("projects.d"),
            allowed_roots: vec![tmp.path().to_path_buf()],
            allow_cwd_anywhere: false,
            overwrite: false,
            json: false,
        };
        let err = ensure_enroll_outputs_available(&opts).unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[tokio::test]
    async fn pairing_create_prints_pairing_code_once_without_auth_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            assert!(request.starts_with("POST /api/pairing/create "));
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer fake-bootstrap"));
            let body = r#"{"success":true,"pairing_code":"wc_pair_copy_once","expires_at":123,"username":"alice","client_id":"alice-laptop"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let opts = PairingCreateOptions {
            server_url: format!("http://{}", addr),
            token: Some("fake-bootstrap".to_string()),
            username: "alice".to_string(),
            client_id: "alice-laptop".to_string(),
            ttl_secs: 600,
            ..PairingCreateOptions::default()
        };
        let output = run_pairing_create(opts).await.unwrap();
        handle.join().unwrap();
        assert_eq!(output.matches("wc_pair_copy_once").count(), 1);
        assert!(!output.contains("fake-bootstrap"));
    }

    #[tokio::test]
    async fn client_enroll_posts_without_authorization_and_writes_files() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 16384];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            tx.send(request).unwrap();
            let body = r#"{"success":true,"username":"alice","client_id":"alice-laptop","user_token":"pat_fake_plaintext_123456","agent_token":"agent_fake_plaintext_abcdef","user_token_prefix":"wc_pat_fake_pre","agent_token_prefix":"wc_agent_fake_p"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let tmp = tempfile::tempdir().unwrap();
        let opts = ClientEnrollOptions {
            server_url: format!("http://{}", addr),
            pairing_code: "wc_pair_fake".to_string(),
            client_id: "alice-laptop".to_string(),
            display_name: Some("Alice Laptop".to_string()),
            transport: TRANSPORT_WEBSOCKET.to_string(),
            output_dir: tmp.path().to_path_buf(),
            agent_config: tmp.path().join("agent.toml"),
            projects_dir: tmp.path().join("projects.d"),
            allowed_roots: vec![tmp.path().to_path_buf()],
            allow_cwd_anywhere: false,
            overwrite: false,
            json: true,
        };
        let output = run_client_enroll(opts).await.unwrap();
        handle.join().unwrap();
        let request = rx.recv().unwrap();
        assert!(request.starts_with("POST /api/pairing/enroll "));
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        assert!(request.contains(r#""pairing_code":"wc_pair_fake""#));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("webcodex-user-token"))
                .unwrap()
                .trim(),
            "pat_fake_plaintext_123456"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("webcodex-agent-token"))
                .unwrap()
                .trim(),
            "agent_fake_plaintext_abcdef"
        );
        let agent_config = std::fs::read_to_string(tmp.path().join("agent.toml")).unwrap();
        assert!(agent_config.contains("agent_fake_plaintext_abcdef"));
        assert!(output.contains(
            tmp.path()
                .join("webcodex-user-token")
                .to_string_lossy()
                .as_ref()
        ));
        assert!(output.contains(
            tmp.path()
                .join("webcodex-agent-token")
                .to_string_lossy()
                .as_ref()
        ));
        assert!(output.contains(tmp.path().join("agent.toml").to_string_lossy().as_ref()));
        assert!(output.contains(tmp.path().join("projects.d").to_string_lossy().as_ref()));
        assert!(!output.contains("pat_fake_plaintext_123456"));
        assert!(!output.contains("agent_fake_plaintext_abcdef"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [
                tmp.path().join("webcodex-user-token"),
                tmp.path().join("webcodex-agent-token"),
                tmp.path().join("agent.toml"),
            ] {
                let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o600);
            }
        }
    }

    #[tokio::test]
    async fn doctor_does_not_print_token_or_html_body() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf).unwrap();
            let body = "<html>secret-token-in-body</html>";
            write!(
                stream,
                "HTTP/1.1 502 Bad Gateway\r\ncontent-type: text/html\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let tmp = tempfile::tempdir().unwrap();
        let token_file = tmp.path().join("user-token");
        std::fs::write(&token_file, "secret-doctor-token\n").unwrap();
        let opts = DoctorOptions {
            server_url: Some(format!("http://{}", addr)),
            user_token_file: Some(token_file),
            ..DoctorOptions::default()
        };
        let (output, has_fail) = run_doctor(opts).await.unwrap();
        handle.join().unwrap();
        assert!(has_fail);
        assert!(!output.contains("secret-doctor-token"));
        assert!(!output.contains("secret-token-in-body"));
        assert!(output.contains("non-JSON response"));
    }

    /// Write a minimal agent.toml (with shell profiles) into `dir` and return
    /// its path. Used by the local agent-config doctor tests.
    fn write_doctor_agent_config(
        dir: &Path,
        projects_dir: &Path,
        default_profile: Option<&str>,
    ) -> PathBuf {
        let default_line = default_profile
            .map(|p| format!("default_profile = {:?}\n", p))
            .unwrap_or_default();
        let agent_toml = format!(
            "server_url = \"http://127.0.0.1:8000\"\n\
             token = \"test-token\"\n\
             client_id = \"oe\"\n\
             projects_dir = {:?}\n\
             [shell]\n\
             {default_line}\
             [shell.profiles.rust]\n\
             program = \"sh\"\n\
             args = [\"-c\"]\n\
             init_script = \"export SECRET=DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY\"\n\
             [shell.profiles.rust.env]\n\
             CARGO_HOME = \"/root/.cargo\"\n\
             SECRET_ENV = \"DO_NOT_LEAK_THIS_ENV_VALUE\"\n",
            projects_dir
        );
        let path = dir.join("agent.toml");
        std::fs::write(&path, agent_toml).unwrap();
        path
    }

    fn write_doctor_project(
        projects_dir: &Path,
        id: &str,
        path: &Path,
        shell_profile: Option<&str>,
    ) {
        std::fs::create_dir_all(projects_dir).unwrap();
        let shell_line = shell_profile
            .map(|p| format!("shell_profile = {:?}\n", p))
            .unwrap_or_default();
        std::fs::write(
            projects_dir.join(format!("{id}.toml")),
            format!(
                "id = {:?}\npath = {:?}\nname = {:?}\n{shell_line}",
                id,
                path.to_string_lossy(),
                id
            ),
        )
        .unwrap();
    }

    #[test]
    fn doctor_local_agent_config_detects_configured_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("projects.d");
        let project_dir = tmp.path().join("rust-proj");
        std::fs::create_dir_all(&project_dir).unwrap();
        write_doctor_project(&projects_dir, "rust-proj", &project_dir, Some("rust"));
        let cfg_path = write_doctor_agent_config(tmp.path(), &projects_dir, Some("rust"));
        let checks = run_local_agent_doctor(&cfg_path);
        let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"agent config"), "{names:?}");
        assert!(names.contains(&"shell profiles"), "{names:?}");
        assert!(names.contains(&"projects_dir"), "{names:?}");
        // The configured profile resolves successfully.
        let profile_check = checks
            .iter()
            .find(|c| c.name == "project 'rust-proj' shell_profile")
            .expect("shell_profile check present");
        assert_eq!(profile_check.status, "PASS", "{:?}", profile_check);
        assert!(profile_check.detail.contains("resolved='rust'"));
        assert!(profile_check.detail.contains("has_init_script=true"));
        assert!(profile_check.detail.contains("env_keys_count=2"));
        // Sanitization: never print the init_script body or env value.
        assert!(!profile_check
            .detail
            .contains("DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY"));
        let all_rendered = format!(
            "{}",
            checks
                .iter()
                .map(|c| c.detail.as_str())
                .collect::<Vec<_>>()
                .join("|")
        );
        assert!(
            !all_rendered.contains("DO_NOT_LEAK_THIS_ENV_VALUE"),
            "{all_rendered}"
        );
    }

    #[test]
    fn doctor_local_agent_config_detects_missing_shell_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = tmp.path().join("projects.d");
        let project_dir = tmp.path().join("bad-proj");
        std::fs::create_dir_all(&project_dir).unwrap();
        // Project asks for a profile that is not configured, and there is no
        // default_profile to fall back to.
        write_doctor_project(&projects_dir, "bad-proj", &project_dir, Some("nope"));
        let cfg_path = write_doctor_agent_config(tmp.path(), &projects_dir, None);
        let checks = run_local_agent_doctor(&cfg_path);
        let profile_check = checks
            .iter()
            .find(|c| c.name == "project 'bad-proj' shell_profile")
            .expect("shell_profile check present");
        assert_eq!(profile_check.status, "FAIL", "{:?}", profile_check);
        assert!(profile_check.detail.contains("not in shell.profiles"));
    }

    #[test]
    fn doctor_parse_accepts_agent_config_and_project_flags() {
        let opts = parse_doctor(&args(&[
            "--agent-config",
            "/tmp/agent.toml",
            "--project",
            "agent:oe:webcodex",
            "--strict",
        ]))
        .unwrap();
        assert_eq!(
            opts.agent_config.as_deref(),
            Some(Path::new("/tmp/agent.toml"))
        );
        assert_eq!(opts.project.as_deref(), Some("agent:oe:webcodex"));
        assert!(opts.strict);
    }

    #[test]
    fn shell_profiles_doc_exists_and_index_links_it() {
        // The shell-profiles user doc must exist and be linked from INDEX.md.
        let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/SHELL_PROFILES.md");
        assert!(doc.is_file(), "docs/SHELL_PROFILES.md must exist");
        let index =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/INDEX.md"))
                .unwrap();
        assert!(
            index.contains("SHELL_PROFILES.md"),
            "INDEX.md must link SHELL_PROFILES.md"
        );
    }

    #[test]
    fn server_init_parse_defaults() {
        let opts = parse_server_init(&args(&[])).unwrap();
        assert_eq!(opts.listen, "127.0.0.1:8080");
        if is_effective_root() {
            assert_eq!(opts.data_dir, PathBuf::from("/var/lib/webcodex"));
            assert_eq!(opts.env_file, PathBuf::from("/etc/webcodex/webcodex.env"));
        } else {
            assert!(opts.data_dir.ends_with(".local/share/webcodex"));
            assert!(opts.env_file.ends_with(".config/webcodex/webcodex.env"));
        }
        assert!(!opts.overwrite);
        assert!(!opts.json);
    }

    #[test]
    fn server_init_writes_env_file_and_0600_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("etc/webcodex.env");
        let data_dir = tmp.path().join("data");
        let opts = parse_server_init(&args(&[
            "--listen",
            "127.0.0.1:9090",
            "--data-dir",
            data_dir.to_str().unwrap(),
            "--env-file",
            env_file.to_str().unwrap(),
            "--public-url",
            "https://example.test/",
        ]))
        .unwrap();
        let output = run_server_init(opts).unwrap();
        let content = std::fs::read_to_string(&env_file).unwrap();
        assert!(content.contains("WEBCODEX_ADDR=127.0.0.1:9090\n"));
        assert!(content.contains(&format!("WEBCODEX_DATA={}\n", data_dir.display())));
        assert!(content.contains("WEBCODEX_TOKEN=wc_boot_"));
        assert!(content.contains("WEBCODEX_PUBLIC_URL=https://example.test\n"));
        let token = parse_env_content_value(&content, "WEBCODEX_TOKEN").unwrap();
        assert!(!output.contains(&token));
        assert!(output.contains("token prefix:"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&env_file).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn server_init_refuses_overwrite_unless_requested() {
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("webcodex.env");
        std::fs::write(&env_file, "WEBCODEX_TOKEN=old\n").unwrap();
        let mut opts = parse_server_init(&args(&[
            "--env-file",
            env_file.to_str().unwrap(),
            "--data-dir",
            tmp.path().to_str().unwrap(),
        ]))
        .unwrap();
        let err = run_server_init(opts.clone()).unwrap_err();
        assert!(err.contains("already exists"));
        opts.overwrite = true;
        run_server_init(opts).unwrap();
        let content = std::fs::read_to_string(&env_file).unwrap();
        assert!(content.contains("WEBCODEX_ADDR="));
        assert!(!content.contains("WEBCODEX_TOKEN=old"));
    }

    #[test]
    fn server_init_json_output_does_not_include_full_token() {
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("webcodex.env");
        let opts = parse_server_init(&args(&[
            "--env-file",
            env_file.to_str().unwrap(),
            "--data-dir",
            tmp.path().to_str().unwrap(),
            "--json",
        ]))
        .unwrap();
        let output = run_server_init(opts).unwrap();
        let content = std::fs::read_to_string(&env_file).unwrap();
        let token = parse_env_content_value(&content, "WEBCODEX_TOKEN").unwrap();
        assert!(!output.contains(&token));
        let json: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["wrote_env_file"], true);
        assert!(json["token_prefix"]
            .as_str()
            .unwrap()
            .starts_with("wc_boot"));
        assert!(json.get("token").is_none());
    }

    #[test]
    fn server_init_output_stdout_explicitly_prints_env_contents_with_token() {
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("webcodex.env");
        let opts = parse_server_init(&args(&[
            "--env-file",
            env_file.to_str().unwrap(),
            "--data-dir",
            tmp.path().to_str().unwrap(),
            "--output",
            "-",
        ]))
        .unwrap();
        let output = run_server_init(opts).unwrap();
        let content = std::fs::read_to_string(&env_file).unwrap();
        let token = parse_env_content_value(&content, "WEBCODEX_TOKEN").unwrap();
        assert_eq!(output, content);
        assert!(output.contains(&format!("WEBCODEX_TOKEN={}", token)));
        assert!(server_init_usage().contains("including the full WEBCODEX_TOKEN"));
    }

    #[test]
    fn install_service_generates_expected_unit_without_tokens() {
        let opts = parse_server_install_service(&args(&[
            "--env-file",
            "/etc/webcodex/webcodex.env",
            "--bin",
            "/usr/local/bin/webcodex",
            "--working-directory",
            "/var/lib/webcodex",
            "--user",
            "webcodex",
            "--group",
            "webcodex",
            "--dry-run",
        ]))
        .unwrap();
        let unit = run_server_install_service(opts).unwrap();
        assert!(unit.contains("[Unit]\nDescription=WebCodex Runtime\n"));
        assert!(unit.contains("EnvironmentFile=/etc/webcodex/webcodex.env\n"));
        assert!(unit.contains("ExecStart=/usr/local/bin/webcodex\n"));
        assert!(unit.contains("WorkingDirectory=/var/lib/webcodex\n"));
        assert!(unit.contains("User=webcodex\n"));
        assert!(unit.contains("Group=webcodex\n"));
        assert!(!unit.contains("WEBCODEX_TOKEN"));
        assert!(!unit.contains("wc_boot_"));
    }

    #[test]
    fn install_service_refuses_overwrite_unless_requested() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        std::fs::write(&service_file, "old").unwrap();
        let opts = parse_server_install_service(&args(&[
            "--bin",
            "/usr/local/bin/webcodex",
            "--service-file",
            service_file.to_str().unwrap(),
        ]))
        .unwrap();
        let err = run_server_install_service(opts).unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[test]
    fn install_service_dry_run_and_output_work_without_systemd() {
        let dry =
            parse_server_install_service(&args(&["--bin", "/usr/local/bin/webcodex", "--dry-run"]))
                .unwrap();
        assert!(run_server_install_service(dry)
            .unwrap()
            .contains("ExecStart=/usr/local/bin/webcodex"));

        let out = parse_server_install_service(&args(&[
            "--bin",
            "/usr/local/bin/webcodex",
            "--output",
            "-",
            "--json",
        ]))
        .unwrap();
        let json: Value = serde_json::from_str(&run_server_install_service(out).unwrap()).unwrap();
        assert_eq!(json["dry_run"], true);
        assert!(json["unit"].as_str().unwrap().contains("[Service]"));
    }

    #[test]
    fn agent_install_service_generates_expected_unit_without_tokens() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("agent.toml");
        std::fs::write(&config, "token = \"agent_secret_should_not_print\"\n").unwrap();
        let opts = parse_agent_install_service(&args(&[
            "--config",
            config.to_str().unwrap(),
            "--bin",
            "/opt/webcodex/bin/webcodex-agent",
            "--working-directory",
            "/root",
            "--user",
            "webcodex",
            "--group",
            "webcodex",
            "--dry-run",
        ]))
        .unwrap();
        let unit = run_agent_install_service(opts).unwrap();
        assert!(unit.contains("[Unit]\nDescription=WebCodex Agent\n"));
        assert!(unit.contains(&format!(
            "ExecStart=/opt/webcodex/bin/webcodex-agent --config {}\n",
            config.display()
        )));
        assert!(unit.contains("Restart=on-failure\n"));
        assert!(unit.contains("RestartSec=3\n"));
        assert!(unit.contains("WorkingDirectory=/root\n"));
        assert!(unit.contains("User=webcodex\n"));
        assert!(unit.contains("Group=webcodex\n"));
        assert!(!unit.contains("agent_secret_should_not_print"));
        assert!(!unit.contains("Authorization"));
        assert!(!unit.contains("token ="));
    }

    #[test]
    fn agent_install_service_refuses_overwrite_unless_requested() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex-agent.service");
        std::fs::write(&service_file, "old").unwrap();
        let opts = parse_agent_install_service(&args(&[
            "--config",
            "/etc/webcodex/agent.toml",
            "--bin",
            "/opt/webcodex/bin/webcodex-agent",
            "--service-file",
            service_file.to_str().unwrap(),
        ]))
        .unwrap();
        let err = run_agent_install_service(opts).unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[test]
    fn agent_install_service_dry_run_and_output_work_without_systemd() {
        let dry = parse_agent_install_service(&args(&[
            "--config",
            "/etc/webcodex/agent.toml",
            "--bin",
            "/opt/webcodex/bin/webcodex-agent",
            "--dry-run",
        ]))
        .unwrap();
        assert!(run_agent_install_service(dry).unwrap().contains(
            "ExecStart=/opt/webcodex/bin/webcodex-agent --config /etc/webcodex/agent.toml"
        ));

        let out = parse_agent_install_service(&args(&[
            "--config",
            "/etc/webcodex/agent.toml",
            "--bin",
            "/opt/webcodex/bin/webcodex-agent",
            "--output",
            "-",
            "--json",
        ]))
        .unwrap();
        let json: Value = serde_json::from_str(&run_agent_install_service(out).unwrap()).unwrap();
        assert_eq!(json["dry_run"], true);
        assert!(json["unit"].as_str().unwrap().contains(
            "ExecStart=/opt/webcodex/bin/webcodex-agent --config /etc/webcodex/agent.toml"
        ));
    }

    #[test]
    fn agent_status_parses_agent_toml_without_printing_token_and_systemd_unknown() {
        let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", "");
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("agent.toml");
        let secret = "agent_status_secret_1234567890";
        std::fs::write(
            &config,
            format!(
                r#"
server_url = "https://example.test"
token = "{secret}"
client_id = "alice-laptop"
owner = "alice"
display_name = "Alice Laptop"
transport = "websocket"
projects_dir = "/etc/webcodex/projects.d"

[policy]
allowed_roots = ["/srv/projects"]
"#
            ),
        )
        .unwrap();
        let opts =
            parse_agent_status(&args(&["--config", config.to_str().unwrap(), "--json"])).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let output = rt.block_on(run_agent_status(opts)).unwrap();
        if let Some(path) = old_path {
            std::env::set_var("PATH", path);
        } else {
            std::env::remove_var("PATH");
        }
        assert!(!output.contains(secret));
        let json: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["service"]["active"], "unknown");
        assert_eq!(json["service"]["enabled"], "unknown");
        assert_eq!(json["config"]["client_id"], "alice-laptop");
        assert_eq!(json["config"]["owner"], "alice");
        assert_eq!(json["config"]["allowed_roots"]["count"], 1);
        assert!(json.get("token").is_none());
        assert!(json["config"].get("token").is_none());
    }

    #[tokio::test]
    async fn agent_status_detects_current_client_online_and_agent_boundary() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            for i in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 16384];
                let n = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                tx.send(request.clone()).unwrap();
                if i == 0 {
                    let body = r#"{"success":true,"output":{"agents":{"clients":[{"client_id":"alice-laptop","connected":true,"status":"online"}]}}}"#;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                } else {
                    let body = r#"{"error":"forbidden"}"#;
                    write!(
                        stream,
                        "HTTP/1.1 403 Forbidden\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                }
            }
        });
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("agent.toml");
        std::fs::write(
            &config,
            r#"
server_url = "http://127.0.0.1:1"
token = "agent_config_secret_abcdef"
client_id = "alice-laptop"
owner = "alice"
transport = "websocket"
"#,
        )
        .unwrap();
        let user_token_file = tmp.path().join("webcodex-user-token");
        let agent_token_file = tmp.path().join("webcodex-agent-token");
        std::fs::write(&user_token_file, "pat_online_secret_1234567890\n").unwrap();
        std::fs::write(&agent_token_file, "agent_boundary_secret_1234567890\n").unwrap();
        let opts = parse_agent_status(&args(&[
            "--config",
            config.to_str().unwrap(),
            "--server-url",
            &format!("http://{}", addr),
            "--user-token-file",
            user_token_file.to_str().unwrap(),
            "--agent-token-file",
            agent_token_file.to_str().unwrap(),
        ]))
        .unwrap();
        let output = run_agent_status(opts).await.unwrap();
        handle.join().unwrap();
        let first_request = rx.recv().unwrap();
        let second_request = rx.recv().unwrap();
        assert!(first_request
            .to_ascii_lowercase()
            .contains("authorization: bearer pat_online_secret_1234567890"));
        assert!(second_request
            .to_ascii_lowercase()
            .contains("authorization: bearer agent_boundary_secret_1234567890"));
        for secret in [
            "agent_config_secret_abcdef",
            "pat_online_secret_1234567890",
            "agent_boundary_secret_1234567890",
        ] {
            assert!(!output.contains(secret));
        }
        assert!(output.contains("client online:        yes"));
        assert!(output.contains("agent token boundary: PASS"));
    }

    #[test]
    fn token_prefix_never_exposes_full_token() {
        let p = token_prefix("wc_abcdef0123456789");
        assert!(p.ends_with('…'));
        assert!(!p.contains("0123456789"));
        assert_eq!(p, "wc_abcde…");
    }

    /// Fake server: respond to a sequence of (path -> response body) entries.
    /// Captures the inbound Authorization header so tests can assert the
    /// bootstrap token is present but never echoed in our output.
    #[tokio::test]
    async fn setup_single_user_runs_expected_calls_and_writes_0600_files() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let bootstrap = "fake-bootstrap-token-xyz".to_string();
        let bootstrap_for_thread = bootstrap.clone();
        let handle = thread::spawn(move || {
            let mut remaining = 3u32;
            while remaining > 0 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 16384];
                let n = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);
                let auth_ok = request
                    .to_ascii_lowercase()
                    .contains(&format!("authorization: bearer {}", bootstrap_for_thread));
                assert!(auth_ok, "bootstrap token must be sent as bearer");
                let path = request.lines().next().unwrap_or("");
                let body = if path.contains("/api/users/create") {
                    r#"{"success":true,"user":{"username":"yyjeqhc"}}"#
                } else if path.contains("/api/tokens/create") {
                    r#"{"success":true,"token":"wc_user_fake_plaintext_12345","token_id":"ut-1"}"#
                } else if path.contains("/api/agent-tokens/create") {
                    r#"{"success":true,"token":"agent_fake_plaintext_67890","token_id":"at-1"}"#
                } else {
                    r#"{"error":"unexpected path"}"#
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
                remaining -= 1;
            }
        });

        let tmp = tempfile::tempdir().unwrap();
        let opts = SetupSingleUserOptions {
            server_url: format!("http://{}", addr),
            token: Some(bootstrap.clone()),
            token_file: None,
            username: "yyjeqhc".to_string(),
            client_id: "oe".to_string(),
            display_name: None,
            role: "admin".to_string(),
            gpt_token_name: "chatgpt-action".to_string(),
            agent_token_name: "oe agent".to_string(),
            output_dir: tmp.path().to_path_buf(),
            force_create_tokens: false,
            json: false,
        };
        let summary = run_setup_single_user(opts).await.unwrap();
        handle.join().unwrap();

        // Summary must NOT contain full tokens or the bootstrap token.
        assert!(!summary.contains("wc_user_fake_plaintext_12345"));
        assert!(!summary.contains("agent_fake_plaintext_67890"));
        assert!(!summary.contains(&bootstrap));
        // Prefixes are present.
        assert!(summary.contains("wc_user_"));
        assert!(summary.contains("wc_agent"));
        assert!(summary.contains("yyjeqhc"));

        // Files written with 0600 and contain the full one-time tokens.
        let user_token = std::fs::read_to_string(tmp.path().join("webcodex-user-token")).unwrap();
        assert_eq!(user_token.trim(), "wc_user_fake_plaintext_12345");
        let agent_token = std::fs::read_to_string(tmp.path().join("webcodex-agent-token")).unwrap();
        assert_eq!(agent_token.trim(), "agent_fake_plaintext_67890");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(tmp.path().join("webcodex-user-token"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(m, 0o600);
            let m = std::fs::metadata(tmp.path().join("webcodex-agent-token"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(m, 0o600);
        }
    }

    #[tokio::test]
    async fn setup_single_user_handles_user_already_exists() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let bootstrap = "fake-bootstrap-ae".to_string();
        let bootstrap_for_thread = bootstrap.clone();
        let handle = thread::spawn(move || {
            let mut remaining = 3u32;
            while remaining > 0 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 16384];
                let n = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);
                let _ = request
                    .to_ascii_lowercase()
                    .contains(&format!("authorization: bearer {}", bootstrap_for_thread));
                let path = request.lines().next().unwrap_or("");
                let (status, body) = if path.contains("/api/users/create") {
                    ("409 Conflict", r#"{"error":"user already exists"}"#)
                } else if path.contains("/api/tokens/create") {
                    (
                        "200 OK",
                        r#"{"success":true,"token":"wc_user_ae_fake_token","token_id":"ut-1"}"#,
                    )
                } else {
                    (
                        "200 OK",
                        r#"{"success":true,"token":"agent_ae_fake_token","token_id":"at-1"}"#,
                    )
                };
                write!(
                    stream,
                    "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                )
                .unwrap();
                remaining -= 1;
            }
        });
        let tmp = tempfile::tempdir().unwrap();
        let opts = SetupSingleUserOptions {
            server_url: format!("http://{}", addr),
            token: Some(bootstrap.clone()),
            token_file: None,
            username: "yyjeqhc".to_string(),
            client_id: "oe".to_string(),
            display_name: None,
            role: "admin".to_string(),
            gpt_token_name: "chatgpt-action".to_string(),
            agent_token_name: "oe agent".to_string(),
            output_dir: tmp.path().to_path_buf(),
            force_create_tokens: false,
            json: true,
        };
        let summary = run_setup_single_user(opts).await.unwrap();
        handle.join().unwrap();
        assert!(summary.contains("\"user_already_existed\": true"));
        assert!(!summary.contains(&bootstrap));
        assert!(!summary.contains("wc_user_ae_fake_token"));
    }

    #[tokio::test]
    async fn server_status_parses_env_token_posts_and_does_not_print_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 16384];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            tx.send(request.clone()).unwrap();
            let body = r#"{"success":true,"output":{"service":"webcodex","auth_enabled":true,"configured_public_url":"https://example.test","tools":{"count":12},"agents":{"online_count":2}}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("webcodex.env");
        let token = "secret-status-token";
        std::fs::write(&env_file, format!("WEBCODEX_TOKEN={}\n", token)).unwrap();
        let opts = parse_server_status(&args(&[
            "--url",
            &format!("http://{}", addr),
            "--env-file",
            env_file.to_str().unwrap(),
        ]))
        .unwrap();
        let output = run_server_status(opts).await.unwrap();
        handle.join().unwrap();
        let request = rx.recv().unwrap();
        assert!(request.starts_with("POST /api/runtime/status "));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer secret-status-token"));
        assert!(!output.contains(token));
        assert!(output.contains("HTTP reachable:        yes"));
        assert!(output.contains("auth_enabled:          true"));
        assert!(output.contains("configured_public_url: https://example.test"));
        assert!(output.contains("tools.count:           12"));
        assert!(output.contains("agents.online_count:   2"));
    }

    #[tokio::test]
    async fn server_status_token_file_takes_priority_over_env_file() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 16384];
            let n = stream.read(&mut buf).unwrap();
            tx.send(String::from_utf8_lossy(&buf[..n]).to_string())
                .unwrap();
            let body = r#"{"success":true,"output":{"auth_enabled":true,"configured_public_url":null,"tools":{"count":0},"agents":{"online_count":0}}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("webcodex.env");
        let token_file = tmp.path().join("token");
        std::fs::write(&env_file, "WEBCODEX_TOKEN=env-token\n").unwrap();
        std::fs::write(&token_file, "file-token\n").unwrap();
        let opts = parse_server_status(&args(&[
            "--url",
            &format!("http://{}", addr),
            "--env-file",
            env_file.to_str().unwrap(),
            "--token-file",
            token_file.to_str().unwrap(),
            "--json",
        ]))
        .unwrap();
        let output = run_server_status(opts).await.unwrap();
        handle.join().unwrap();
        let request = rx.recv().unwrap();
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer file-token"));
        assert!(!request
            .to_ascii_lowercase()
            .contains("authorization: bearer env-token"));
        assert!(!output.contains("file-token"));
        assert!(!output.contains("env-token"));
    }

    #[tokio::test]
    async fn server_status_connection_failure_reports_unreachable_without_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("webcodex.env");
        let token = "connection-failure-token";
        std::fs::write(&env_file, format!("WEBCODEX_TOKEN={}\n", token)).unwrap();
        let opts = parse_server_status(&args(&[
            "--url",
            &format!("http://{}", addr),
            "--env-file",
            env_file.to_str().unwrap(),
        ]))
        .unwrap();
        let output = run_server_status(opts).await.unwrap();
        assert!(output.contains("HTTP reachable:        no"));
        assert!(output.contains("HTTP error:"));
        assert!(!output.contains(token));
    }

    #[tokio::test]
    async fn server_status_non_json_error_reports_status_and_content_type_only() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).unwrap();
            let body = "secret body should not be printed";
            write!(
                stream,
                "HTTP/1.1 502 Bad Gateway\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let opts = parse_server_status(&args(&["--url", &format!("http://{}", addr)])).unwrap();
        let output = run_server_status(opts).await.unwrap();
        handle.join().unwrap();
        assert!(output.contains("HTTP reachable:        no"));
        assert!(output.contains("HTTP status:           502"));
        assert!(output.contains("HTTP content-type:     text/html; charset=utf-8"));
        assert!(!output.contains("secret body"));
    }

    #[test]
    fn non_json_error_reports_status_and_content_type_only() {
        let body = "<html>".repeat(500);
        let msg = format_error_body(502, "text/html; charset=utf-8", &body);
        assert_eq!(
            msg,
            "request failed: HTTP 502 (content-type: text/html; charset=utf-8)"
        );
        assert!(!msg.contains("<html>"));
    }

    #[test]
    fn token_not_printed_in_json_error() {
        // Simulate a server error body that echoes the token; the formatter
        // must surface the error text but must never have received the bearer
        // token to echo in the first place. We assert the helper does not add
        // any token of its own.
        let msg = format_error_body(500, "application/json", r#"{"error":"bad request"}"#);
        assert!(msg.contains("HTTP 500"));
        assert!(msg.contains("bad request"));
        assert!(!msg.contains("fake-secret"));
    }

    // ------------------------------------------------------------------
    // connect + server up quick-start CLI tests
    // ------------------------------------------------------------------

    fn cli_exit<I, S>(args: I) -> Result<String, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        match cli_action(args) {
            CliAction::Exit {
                code: 0, stdout, ..
            } => Ok(stdout),
            CliAction::Exit { stderr, .. } => Err(stderr),
            other => Err(format!("expected exit, got {other:?}")),
        }
    }

    #[test]
    fn connect_help_prints_usage() {
        let out = cli_exit(["connect", "--help"]).unwrap();
        assert!(out.contains("Usage: webcodex-cli connect"));
        assert!(out.contains("--key"));
        assert!(out.contains("--open"));
        assert!(out.contains("mutually exclusive"));
    }

    #[test]
    fn connect_key_and_open_are_mutually_exclusive() {
        let err =
            cli_exit(["connect", "http://127.0.0.1:8080", "--key", "abc", "--open"]).unwrap_err();
        assert!(err.contains("mutually exclusive"), "err was: {err}");
    }

    #[test]
    fn connect_requires_key_or_open() {
        let err = cli_exit(["connect", "http://127.0.0.1:8080"]).unwrap_err();
        assert!(
            err.contains("--key") || err.contains("--open"),
            "err was: {err}"
        );
    }

    #[test]
    fn connect_key_parses_with_default_root() {
        match cli_action(["connect", "http://127.0.0.1:8080", "--key", "abc123"]) {
            CliAction::Connect(opts) => {
                assert_eq!(opts.server_url, "http://127.0.0.1:8080");
                assert_eq!(opts.mode, ConnectMode::SharedKey("abc123".to_string()));
                // Default root is the current working directory.
                assert!(opts.root.is_absolute() || !opts.root.as_os_str().is_empty());
                assert!(!opts.overwrite);
                assert!(!opts.json);
            }
            other => panic!("expected Connect, got {other:?}"),
        }
    }

    #[test]
    fn connect_open_parses() {
        match cli_action([
            "connect",
            "http://127.0.0.1:8080",
            "--open",
            "--root",
            "/tmp/proj",
        ]) {
            CliAction::Connect(opts) => {
                assert_eq!(opts.server_url, "http://127.0.0.1:8080");
                assert_eq!(opts.mode, ConnectMode::Open);
                assert_eq!(opts.root, PathBuf::from("/tmp/proj"));
            }
            other => panic!("expected Connect, got {other:?}"),
        }
    }

    #[test]
    fn connect_explicit_client_id_and_output_dir() {
        match cli_action([
            "connect",
            "https://example.com",
            "--key",
            "k",
            "--root",
            "/tmp/p",
            "--client-id",
            "my-laptop",
            "--output-dir",
            "/tmp/out",
            "--overwrite",
            "--json",
        ]) {
            CliAction::Connect(opts) => {
                assert_eq!(opts.client_id.as_deref(), Some("my-laptop"));
                assert_eq!(
                    opts.output_dir.as_deref(),
                    Some(std::path::Path::new("/tmp/out"))
                );
                assert!(opts.overwrite);
                assert!(opts.json);
            }
            other => panic!("expected Connect, got {other:?}"),
        }
    }

    #[test]
    fn connect_requires_server_url() {
        let err = cli_exit(["connect", "--key", "abc"]).unwrap_err();
        assert!(err.contains("server URL"), "err was: {err}");
    }

    #[test]
    fn server_up_help_prints_usage() {
        let out = cli_exit(["server", "up", "--help"]).unwrap();
        assert!(out.contains("Usage: webcodex-cli server up"));
        assert!(out.contains("--open"));
        assert!(!out.contains("--foreground"));
    }

    #[test]
    fn server_up_parses_open_mode() {
        match cli_action([
            "server",
            "up",
            "--open",
            "--public-url",
            "https://x.example",
        ]) {
            CliAction::ServerUp(opts) => {
                assert!(opts.open);
                assert_eq!(opts.public_url.as_deref(), Some("https://x.example"));
            }
            other => panic!("expected ServerUp, got {other:?}"),
        }
    }

    #[test]
    fn server_up_defaults_to_closed_mode() {
        match cli_action(["server", "up"]) {
            CliAction::ServerUp(opts) => {
                assert!(!opts.open);
                assert!(opts.public_url.is_none());
            }
            other => panic!("expected ServerUp, got {other:?}"),
        }
    }

    #[test]
    fn server_up_foreground_reports_not_implemented() {
        let err = cli_exit(["server", "up", "--foreground"]).unwrap_err();
        assert!(
            err.contains("--foreground is not implemented yet"),
            "err was: {err}"
        );
        assert!(
            !err.contains("Starting server in foreground"),
            "foreground must not imply a server was started"
        );
    }

    #[test]
    fn server_up_output_hides_full_admin_key() {
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("webcodex.env");
        let out = match cli_action([
            "server",
            "up",
            "--env-file",
            env_file.to_str().unwrap(),
            "--data-dir",
            tmp.path().join("data").to_str().unwrap(),
        ]) {
            CliAction::ServerUp(opts) => run_server_up(opts).unwrap(),
            other => panic!("expected ServerUp, got {other:?}"),
        };
        let env_content = fs::read_to_string(&env_file).unwrap();
        let token = read_env_file_value(&env_file, "WEBCODEX_TOKEN")
            .unwrap()
            .unwrap();
        assert!(env_content.contains(&token));
        assert!(
            !out.contains(&token),
            "stdout must not contain full admin key"
        );
        assert!(out.contains("admin key:"));
        assert!(out.contains("token prefix:"));
    }

    #[test]
    fn server_up_json_hides_full_admin_key() {
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("webcodex.env");
        let out = match cli_action([
            "server",
            "up",
            "--json",
            "--env-file",
            env_file.to_str().unwrap(),
            "--data-dir",
            tmp.path().join("data").to_str().unwrap(),
        ]) {
            CliAction::ServerUp(opts) => run_server_up(opts).unwrap(),
            other => panic!("expected ServerUp, got {other:?}"),
        };
        let token = read_env_file_value(&env_file, "WEBCODEX_TOKEN")
            .unwrap()
            .unwrap();
        assert!(
            !out.contains(&token),
            "json must not contain full admin key"
        );
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["token_generated"], true);
        assert!(value["token_prefix"]
            .as_str()
            .unwrap()
            .starts_with("wc_boot"));
        assert!(value.get("token").is_none());
    }

    #[test]
    fn connect_output_uses_agent_registration_quick_start_model() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        fs::create_dir_all(&root).unwrap();
        let output_dir = tmp.path().join("client");
        let out = match cli_action([
            "connect",
            "http://127.0.0.1:8080",
            "--key",
            "abc123",
            "--root",
            root.to_str().unwrap(),
            "--output-dir",
            output_dir.to_str().unwrap(),
        ]) {
            CliAction::Connect(opts) => run_connect(opts).unwrap(),
            other => panic!("expected Connect, got {other:?}"),
        };
        assert!(out.contains("The project should appear after the agent registers"));
        assert!(!out.contains("merge projects.toml"));
        assert!(!out.contains("use the runtime API"));
        assert!(!out.contains("Register the project on the server"));
    }

    #[test]
    fn top_level_usage_mentions_connect_and_server_up() {
        let out = cli_exit(["--help"]).unwrap();
        assert!(out.contains("connect"));
        assert!(out.contains("server up"));
    }
}

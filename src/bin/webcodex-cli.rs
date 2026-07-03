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

use std::path::PathBuf;

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

use admin_cli::{parse_admin_cli, run_admin_command, AdminCliCommand, AdminOptions};
use agent_init::{
    run_agent_init, AgentInitOptions, DEFAULT_INIT_PROJECTS_DIR, DEFAULT_POLL_INTERVAL_MS,
    TRANSPORT_WEBSOCKET,
};
use webcodex_cli::{
    agent_init_usage, agent_install_service_usage, agent_status_usage, agent_usage,
    client_enroll_usage, client_profile_agent_config, client_profile_agent_token_file,
    client_profile_projects_dir, client_profile_service_file, client_profile_user_token_file,
    client_usage, connect_usage, default_client_output_dir_for_profile, default_server_paths,
    discover_binary, discover_named_binary_absolute, discover_webcodex_binary, doctor_usage,
    is_systemd_platform, pairing_create_usage, pairing_usage, query_systemd_service_status,
    read_optional_token, render_token_generate, resolve_doctor_general_token,
    run_agent_install_service, run_agent_status, run_agent_token_create_local, run_client_enroll,
    run_connect, run_doctor, run_local_agent_doctor, run_pairing_create, run_quic_doctor_checks,
    run_server_init, run_server_install_service, run_server_status, run_server_up,
    run_setup_single_user, run_token_create_local, server_init_usage, server_install_service_usage,
    server_status_usage, server_up_usage, server_usage, usage, validate_client_profile,
    write_secret_file, write_text_file, ServerStatusOptions,
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
#[path = "webcodex_cli/tests/mod.rs"]
mod tests;

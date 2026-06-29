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

use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[allow(dead_code)]
#[path = "../shell_protocol.rs"]
mod shell_protocol;

#[allow(dead_code)]
#[path = "../admin_cli.rs"]
mod admin_cli;

#[allow(dead_code)]
#[path = "../agent_init.rs"]
mod agent_init;

use admin_cli::{
    build_admin_request, parse_admin_cli, run_admin_command, AdminCliCommand, AdminOptions,
};
use agent_init::{
    run_agent_init, AgentInitOptions, DEFAULT_INIT_PROJECTS_DIR, DEFAULT_POLL_INTERVAL_MS,
    TRANSPORT_WEBSOCKET,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentStatusOptions {
    config: PathBuf,
    server_url: Option<String>,
    user_token_file: Option<PathBuf>,
    agent_token_file: Option<PathBuf>,
    json: bool,
}

fn usage() -> &'static str {
    "Usage: webcodex-cli <COMMAND>\n\n\
     Management/setup commands for WebCodex.\n\n\
     Commands:\n\
       server init                                      Create server env bootstrap file\n\
       server install-service                           Generate/install a systemd unit\n\
       server status                                    Check service and runtime status\n\
       pairing create                                   Create a temporary client pairing code\n\
       client enroll                                    Enroll a client from a pairing code\n\
       doctor                                           Run non-destructive diagnostics\n\
       user/users create/list                             Manage users\n\
       token generate                                   Generate a local wc_pat_* value and hash\n\
       token create-local                               Locally create and register a wc_pat_* with an account credential\n\
       token register-hash                              Register a precomputed wc_pat_* hash\n\
       token list/revoke                                Manage personal API tokens\n\
       tokens create-local/register-hash/list/revoke    Manage personal API tokens\n\
       agent-token create-local                         Locally create and register a wc_agent_* with an account credential\n\
       agent-token register-hash                        Register a precomputed wc_agent_* hash\n\
       agent-tokens create-local/register-hash/list/revoke Manage agent tokens\n\
       agent init/install-service/status                  Manage client-side agent config/service\n\
       setup single-user                                  Create a user + GPT + agent token set\n\n\
     Options:\n\
       -h, --help       Print help and exit\n\
       -V, --version    Print version and exit\n\n\
     Common flags (users/tokens/agent-tokens/setup):\n\
       --server-url URL    WebCodex server URL (required)\n\
       --token TOKEN       Bootstrap/admin/self bearer token\n\
       --token-file PATH   Read bearer token from file\n\
       Token fallback: WEBCODEX_TOKEN\n\
     Output: JSON unless noted otherwise.\n"
}

fn pairing_usage() -> &'static str {
    "Usage: webcodex-cli pairing <COMMAND>\n\n\
     Commands:\n\
       create       Create a short-lived pairing code for client enrollment\n"
}

fn pairing_create_usage() -> &'static str {
    "Usage: webcodex-cli pairing create --server-url URL --username USER --client-id CLIENT_ID [OPTIONS]\n\n\
     Options:\n\
       --server-url URL          WebCodex server URL\n\
       --env-file PATH           Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH         Read bootstrap/admin bearer token from file\n\
       --token TOKEN             Bootstrap/admin bearer token (discouraged in shell history)\n\
       --username USER           User to ensure/create for enrollment\n\
       --client-id CLIENT_ID     Client id the code is bound to\n\
       --display-name NAME       Optional display name for a newly created user\n\
       --ttl-secs SECS           Pairing code lifetime [default: 600; range: 60..3600]\n\
       --user-token-name NAME    Name for the user API token created during enroll\n\
       --agent-token-name NAME   Name for the agent token created during enroll\n\
       --json                    Print machine-readable output\n\
       -h, --help                Print help and exit\n\n\
     Server/admin-side command:\n\
       pairing create needs server bootstrap/admin auth. The default server\n\
       bootstrap env file lives on the server, not the client.\n\
       On the client, use: webcodex-cli client enroll\n\n\
     Copy only the short-lived wc_pair_* code to the client. Do not copy\n\
     WEBCODEX_TOKEN, wc_pat_*, or wc_agent_* values from server to client.\n\
     This command does not create wc_pat_* or wc_agent_* token files on the\n\
     server.\n"
}

fn client_usage() -> &'static str {
    "Usage: webcodex-cli client <COMMAND>\n\n\
     Commands:\n\
       enroll       Enroll this client using a temporary pairing code\n"
}

fn client_enroll_usage() -> &'static str {
    "Usage: webcodex-cli client enroll --server-url URL --pairing-code CODE --client-id CLIENT_ID [OPTIONS]\n\n\
     Options:\n\
       --server-url URL              WebCodex server URL\n\
       --pairing-code CODE           Temporary one-time pairing code\n\
       --client-id CLIENT_ID         Client id matching the pairing record\n\
       --display-name NAME           Optional agent display name\n\
       --transport websocket|polling|quic|auto Agent transport [default: websocket]\n\
       --output-dir DIR              Output dir [root: /etc/webcodex; user: ~/.config/webcodex]\n\
       --agent-config PATH           Agent config path [default: <output-dir>/agent.toml]\n\
       --projects-dir PATH           Projects registry dir [default: <output-dir>/projects.d]\n\
       --allowed-root PATH           Repeatable allowed project root\n\
       --allow-cwd-anywhere BOOL     Allow cwd outside allowed roots [default: false]\n\
       --overwrite                   Replace existing token/config files\n\
       --json                        Print machine-readable output without full tokens\n\
       -h, --help                    Print help and exit\n\n\
     Enroll receives wc_pat_* and wc_agent_* tokens over HTTPS and writes them\n\
     locally with 0600 permissions. It never sends an Authorization header.\n"
}

fn doctor_usage() -> &'static str {
    "Usage: webcodex-cli doctor [OPTIONS]\n\n\
     Options:\n\
       --server-url URL          WebCodex server URL for HTTP checks\n\
       --env-file PATH           Read WEBCODEX_TOKEN from env file\n\
       --token-file PATH         Read bearer token from file\n\
       --user-token-file PATH    Read user API token for runtime/project checks\n\
       --agent-token-file PATH   Read agent token for boundary checks\n\
       --agent-config PATH       Local agent.toml for shell-profile/project diagnostics\n\
       --project ID              Restrict the remote shell roundtrip to this project id\n\
       --quic                    Run QUIC transport diagnostics\n\
       --server-only             With --quic, only check API + QUIC UDP/TLS/ALPN handshake\n\
       --agent-e2e               With --quic, require an online quic-v1 agent and run dispatch checks\n\
       --quic-server-addr ADDR   QUIC UDP host:port; falls back to [quic].server_addr\n\
       --quic-server-name NAME   QUIC TLS/SNI name; falls back to [quic].server_name\n\
       --quic-alpn ALPN          QUIC ALPN [default: webcodex-agent/1]\n\
       --quic-timeout-secs SECS  QUIC connect timeout [default: 10]\n\
       --quic-client-id ID       Expected QUIC agent client id; falls back to agent.toml client_id\n\
       --json                    Print machine-readable diagnostics\n\
       --strict                  Exit non-zero if any check fails\n\
       -h, --help                Print help and exit\n\n\
     Doctor is non-destructive and never prints tokens or response bodies from\n\
     non-JSON/HTML errors. With --agent-config it parses agent.toml locally and\n\
     checks projects_dir, project paths, and shell_profile resolution without\n\
     contacting the server. It never prints init_script bodies or env values.\n"
}

fn server_usage() -> &'static str {
    "Usage: webcodex-cli server <COMMAND>\n\n\
     Server bootstrap commands.\n\n\
     Commands:\n\
       init                 Create WEBCODEX_TOKEN env bootstrap file\n\
       install-service      Generate/install a systemd unit\n\
       status               Check systemd and /api/runtime/status\n\n\
     Notes:\n\
       server init only creates the server bootstrap/admin WEBCODEX_TOKEN.\n\
       It does not create user API tokens or agent tokens.\n"
}

fn server_init_usage() -> &'static str {
    "Usage: webcodex-cli server init [OPTIONS]\n\n\
     Options:\n\
       --listen ADDR          Listen address [default: 127.0.0.1:8080]\n\
       --data-dir PATH        Data directory [root: /var/lib/webcodex; user: ~/.local/share/webcodex]\n\
       --env-file PATH        Env file [root: /etc/webcodex/webcodex.env; user: ~/.config/webcodex/webcodex.env]\n\
       --public-url URL       Optional public URL to report from runtime status\n\
       --overwrite            Replace an existing env file\n\
       --output -             Also print env contents to stdout, including the full WEBCODEX_TOKEN\n\
       --json                 Print a machine-readable summary without the full token\n\
       -h, --help             Print help and exit\n\n\
     server init writes only WEBCODEX_TOKEN. It does not create wc_pat_* user\n\
     tokens or wc_agent_* agent tokens.\n"
}

fn server_install_service_usage() -> &'static str {
    "Usage: webcodex-cli server install-service [OPTIONS]\n\n\
     Options:\n\
       --env-file PATH             Env file [default: /etc/webcodex/webcodex.env]\n\
       --bin PATH                  webcodex server binary path; defaults to webcodex from PATH when safely discoverable\n\
       --service-file PATH         systemd unit path [default: /etc/systemd/system/webcodex.service]\n\
       --user USER                 Optional systemd User=\n\
       --group GROUP               Optional systemd Group=\n\
       --working-directory PATH    WorkingDirectory= [default: /var/lib/webcodex]\n\
       --overwrite                 Replace an existing service file\n\
       --dry-run                   Print the unit instead of writing it\n\
       --output -                  Print the unit instead of writing it\n\
       --json                      Print a machine-readable summary\n\
       -h, --help                  Print help and exit\n\n\
     Tokens are never inlined in the unit; it uses EnvironmentFile=.\n"
}

fn server_status_usage() -> &'static str {
    "Usage: webcodex-cli server status [OPTIONS]\n\n\
     Options:\n\
       --url URL              Runtime URL [default: http://127.0.0.1:8080]\n\
       --env-file PATH        Read WEBCODEX_TOKEN from env file [default: root /etc/webcodex/webcodex.env; user ~/.config/webcodex/webcodex.env]\n\
       --token-file PATH      Read bearer token from file\n\
       --json                 Print a machine-readable summary\n\
       -h, --help             Print help and exit\n\n\
     Token priority: --token-file, WEBCODEX_TOKEN from --env-file, process\n\
     WEBCODEX_TOKEN, then no token for auth-disabled servers.\n"
}

fn agent_usage() -> &'static str {
    "Usage: webcodex-cli agent <COMMAND>\n\n\
     Client-side agent commands.\n\n\
     Commands:\n\
       init                 Generate an agent.toml config\n\
       install-service      Generate/install a webcodex-agent systemd unit\n\
       status               Check systemd status and safe agent metadata\n"
}

fn agent_install_service_usage() -> &'static str {
    "Usage: webcodex-cli agent install-service --config PATH [--bin PATH] [OPTIONS]\n\n\
     Options:\n\
       --config PATH              Agent config path [default: /etc/webcodex/agent.toml]\n\
       --bin PATH                 webcodex-agent binary path; defaults to webcodex-agent from PATH when safely discoverable\n\
       --service-file PATH        systemd unit path [default: /etc/systemd/system/webcodex-agent.service]\n\
       --working-directory PATH   WorkingDirectory= [default: /root]\n\
       --user USER                Optional systemd User=\n\
       --group GROUP              Optional systemd Group=\n\
       --overwrite                Replace an existing service file\n\
       --dry-run                  Print the unit instead of writing it\n\
       --output -                 Print the unit instead of writing it\n\
       --json                     Print a machine-readable summary\n\
       -h, --help                 Print help and exit\n\n\
     The unit runs: webcodex-agent --config <config>. Tokens are never inlined.\n"
}

fn agent_status_usage() -> &'static str {
    "Usage: webcodex-cli agent status [OPTIONS]\n\n\
     Options:\n\
       --config PATH              Agent config path [default: /etc/webcodex/agent.toml]\n\
       --server-url URL           Override server URL for runtime checks\n\
       --user-token-file PATH     Read user API token for /api/runtime/status\n\
       --agent-token-file PATH    Read agent token for boundary check\n\
       --json                     Print a machine-readable summary\n\
       -h, --help                 Print help and exit\n\n\
     Status prints safe metadata only: no tokens, Authorization headers, full\n\
     agent.toml, env files, or secrets.\n"
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
            stdout: format!("webcodex-cli {}\n", env!("CARGO_PKG_VERSION")),
            stderr: String::new(),
        },
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
        "init" => match parse_cli_agent_init(&args[1..]) {
            Ok(opts) => CliAction::AgentInit(opts),
            Err(e) => CliAction::Exit {
                code: 2,
                stdout: String::new(),
                stderr: format!("{}\n", e),
            },
        },
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

fn parse_agent_install_service(args: &[String]) -> Result<AgentInstallServiceOptions, String> {
    let mut config = PathBuf::from("/etc/webcodex/agent.toml");
    let mut bin: Option<PathBuf> = None;
    let mut service_file = PathBuf::from("/etc/systemd/system/webcodex-agent.service");
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
            "--config" => config = PathBuf::from(next_value(&mut iter, arg)?),
            "--bin" => bin = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--service-file" => service_file = PathBuf::from(next_value(&mut iter, arg)?),
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
    let mut opts = AgentStatusOptions {
        config: PathBuf::from("/etc/webcodex/agent.toml"),
        server_url: None,
        user_token_file: None,
        agent_token_file: None,
        json: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => opts.config = PathBuf::from(next_value(&mut iter, arg)?),
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

fn default_client_output_dir() -> PathBuf {
    if is_effective_root() {
        PathBuf::from("/etc/webcodex")
    } else {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".config/webcodex")
    }
}

fn parse_client_enroll(args: &[String]) -> Result<ClientEnrollOptions, String> {
    let mut server_url = String::new();
    let mut pairing_code = String::new();
    let mut client_id = String::new();
    let mut display_name = None;
    let mut transport = TRANSPORT_WEBSOCKET.to_string();
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
    let output_dir = output_dir.unwrap_or_else(default_client_output_dir);
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
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--server-url" => opts.server_url = Some(next_value(&mut iter, arg)?),
            "--env-file" => opts.env_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--token-file" => opts.token_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
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
        projects_dir: PathBuf::from(DEFAULT_INIT_PROJECTS_DIR),
        output: PathBuf::new(),
        allowed_roots: Vec::new(),
        allow_cwd_anywhere: false,
        overwrite: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--server-url" => opts.server_url = next_value(&mut iter, arg)?,
            "--token" => opts.token = Some(next_value(&mut iter, arg)?),
            "--token-file" => opts.token_file = Some(PathBuf::from(next_value(&mut iter, arg)?)),
            "--client-id" => opts.client_id = next_value(&mut iter, arg)?,
            "--owner" => opts.owner = next_value(&mut iter, arg)?,
            "--display-name" => opts.display_name = Some(next_value(&mut iter, arg)?),
            "--transport" => opts.transport = next_value(&mut iter, arg)?,
            "--poll-interval-ms" => {
                let v = next_value(&mut iter, arg)?;
                opts.poll_interval_ms = v
                    .parse::<u64>()
                    .map_err(|_| "--poll-interval-ms must be an integer".to_string())?;
            }
            "--projects-dir" => opts.projects_dir = PathBuf::from(next_value(&mut iter, arg)?),
            "--allowed-root" => opts
                .allowed_roots
                .push(PathBuf::from(next_value(&mut iter, arg)?)),
            "--allow-cwd-anywhere" => {
                opts.allow_cwd_anywhere = agent_init::parse_bool(&next_value(&mut iter, arg)?)?;
            }
            "--output" => opts.output = PathBuf::from(next_value(&mut iter, arg)?),
            "--overwrite" => opts.overwrite = true,
            "--help" | "-h" => return Err(usage().to_string()),
            _ => return Err(format!("unknown agent init flag: {}", arg)),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerPathDefaults {
    data_dir: PathBuf,
    env_file: PathBuf,
}

fn default_server_paths() -> ServerPathDefaults {
    if is_effective_root() {
        return ServerPathDefaults {
            data_dir: PathBuf::from("/var/lib/webcodex"),
            env_file: PathBuf::from("/etc/webcodex/webcodex.env"),
        };
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    ServerPathDefaults {
        data_dir: home.join(".local/share/webcodex"),
        env_file: home.join(".config/webcodex/webcodex.env"),
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

fn generate_bootstrap_token() -> String {
    format!(
        "wc_boot_{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn generate_local_api_token() -> String {
    format!(
        "wc_pat_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn generate_local_agent_token() -> String {
    format!(
        "wc_agent_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn hash_local_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn local_token_prefix(token: &str) -> String {
    token[..token.len().min(16)].to_string()
}

fn render_token_generate(opts: TokenGenerateOptions) -> String {
    let token = if opts.kind == "agent" {
        generate_local_agent_token()
    } else {
        generate_local_api_token()
    };
    let hash = hash_local_token(&token);
    format!(
        "Token:\n{}\n\nHash:\nsha256:{}\n\nPrefix:\n{}\n",
        token,
        hash,
        local_token_prefix(&token)
    )
}

fn resolve_account_credential(
    explicit: &Option<String>,
    env_name: &Option<String>,
) -> Result<String, String> {
    if let Some(value) = explicit {
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err("--credential cannot be empty".to_string());
        }
        return Ok(value);
    }
    if let Some(env_name) = env_name {
        let env_name = env_name.trim();
        if env_name.is_empty() {
            return Err("--credential-env cannot be empty".to_string());
        }
        let value = std::env::var(env_name)
            .map_err(|_| format!("credential env var {} is not set", env_name))?
            .trim()
            .to_string();
        if value.is_empty() {
            return Err(format!("{} cannot be empty", env_name));
        }
        return Ok(value);
    }
    let value = std::env::var("WEBCODEX_ACCOUNT_CREDENTIAL")
        .map_err(|_| {
            "--credential, --credential-env, or WEBCODEX_ACCOUNT_CREDENTIAL is required".to_string()
        })?
        .trim()
        .to_string();
    if value.is_empty() {
        return Err("WEBCODEX_ACCOUNT_CREDENTIAL cannot be empty".to_string());
    }
    Ok(value)
}

async fn run_token_create_local(opts: TokenCreateLocalOptions) -> Result<String, String> {
    let credential = resolve_account_credential(&opts.credential, &opts.credential_env)?;
    let token = generate_local_api_token();
    let hash = hash_local_token(&token);
    let prefix = local_token_prefix(&token);
    let mut body = json!({
        "username": opts.username,
        "token_hash": format!("sha256:{}", hash),
        "token_prefix": prefix,
        "scopes": opts.scopes,
    });
    if let Some(name) = opts.name {
        body["name"] = json!(name);
    }
    let url = format!(
        "{}/api/tokens/register_hash",
        opts.server_url.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let resp = client
        .post(url)
        .bearer_auth(&credential)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e).replace(&credential, "[redacted]"))?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let _text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "request failed: HTTP {} (content-type: {})",
            status.as_u16(),
            content_type
        ));
    }
    Ok(format!(
        "API token created locally and registered with server.\n\nToken:\n{}\n\nUse this as Bearer token in GPT Action or MCP.\nThis token will not be shown again.\n",
        token
    ))
}

async fn run_agent_token_create_local(
    opts: AgentTokenCreateLocalOptions,
) -> Result<String, String> {
    let token = generate_local_agent_token();
    let hash = hash_local_token(&token);
    let prefix = local_token_prefix(&token);
    let cmd = AdminCliCommand::AgentTokensRegisterHash(
        opts.admin,
        admin_cli::AgentTokenRegisterHashArgs {
            username: opts.username,
            client_id: opts.client_id.clone(),
            name: opts.name,
            token_hash: format!("sha256:{}", hash),
            token_prefix: prefix,
            scopes: opts.scopes,
        },
    );
    let req = build_admin_request(&cmd)?;
    post_json_with_bearer(&req).await?;
    Ok(format!(
        "Agent token created locally and registered with server.\n\nClient ID:\n{}\n\nToken:\n{}\n\nUse this token in webcodex-agent config or WEBCODEX_AGENT_TOKEN.\nThis token will not be shown again.\n",
        opts.client_id, token
    ))
}

async fn post_json_with_bearer(req: &admin_cli::AdminCliRequest) -> Result<(), String> {
    let url = format!("{}{}", req.server_url, req.path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let resp = client
        .post(url)
        .bearer_auth(&req.token)
        .json(&req.body)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e).replace(&req.token, "[redacted]"))?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let _text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "request failed: HTTP {} (content-type: {})",
            status.as_u16(),
            content_type
        )
        .replace(&req.token, "[redacted]"));
    }
    Ok(())
}

fn render_server_env(opts: &ServerInitOptions, token: &str) -> String {
    let mut content = String::new();
    content.push_str(&format!("WEBCODEX_ADDR={}\n", opts.listen.trim()));
    content.push_str(&format!("WEBCODEX_DATA={}\n", opts.data_dir.display()));
    content.push_str(&format!("WEBCODEX_TOKEN={}\n", token));
    if let Some(public_url) = &opts.public_url {
        content.push_str(&format!(
            "WEBCODEX_PUBLIC_URL={}\n",
            public_url.trim().trim_end_matches('/')
        ));
    }
    content
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

fn read_env_file_value(path: &Path, key: &str) -> Result<Option<String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read env file {}: {}", path.display(), e))?;
    Ok(parse_env_content_value(&content, key))
}

fn read_pairing_server_env_file_value(path: &Path, key: &str) -> Result<Option<String>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "failed to read server env file {}: {}; pairing create is a server/admin-side command. Run it on the server or pass a server/admin token file.",
            path.display(),
            e
        )
    })?;
    Ok(parse_env_content_value(&content, key))
}

fn parse_env_content_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((k, value)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let value = value.trim();
        let value = if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        return Some(value.to_string());
    }
    None
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

fn render_systemd_unit(opts: &ServerInstallServiceOptions) -> String {
    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str("Description=WebCodex Runtime\n");
    unit.push_str("After=network-online.target\n");
    unit.push_str("Wants=network-online.target\n\n");
    unit.push_str("[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str(&format!("EnvironmentFile={}\n", opts.env_file.display()));
    unit.push_str(&format!("ExecStart={}\n", opts.bin.display()));
    unit.push_str("Restart=on-failure\n");
    unit.push_str("RestartSec=3\n");
    unit.push_str(&format!(
        "WorkingDirectory={}\n",
        opts.working_directory.display()
    ));
    if let Some(user) = &opts.user {
        unit.push_str(&format!("User={}\n", user));
    }
    if let Some(group) = &opts.group {
        unit.push_str(&format!("Group={}\n", group));
    }
    unit.push_str("\n[Install]\n");
    unit.push_str("WantedBy=multi-user.target\n");
    unit
}

fn render_agent_systemd_unit(opts: &AgentInstallServiceOptions) -> String {
    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str("Description=WebCodex Agent\n");
    unit.push_str("After=network-online.target\n");
    unit.push_str("Wants=network-online.target\n\n");
    unit.push_str("[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str(&format!(
        "ExecStart={} --config {}\n",
        opts.bin.display(),
        opts.config.display()
    ));
    unit.push_str("Restart=on-failure\n");
    unit.push_str("RestartSec=3\n");
    unit.push_str(&format!(
        "WorkingDirectory={}\n",
        opts.working_directory.display()
    ));
    if let Some(user) = &opts.user {
        unit.push_str(&format!("User={}\n", user));
    }
    if let Some(group) = &opts.group {
        unit.push_str(&format!("Group={}\n", group));
    }
    unit.push_str("\n[Install]\n");
    unit.push_str("WantedBy=multi-user.target\n");
    unit
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

/// Resolve the bootstrap token for setup/admin commands. Order:
/// `--token` > `--token-file` > `WEBCODEX_TOKEN`. Errors never echo the token.
fn resolve_token(opts: &AdminOptions, env_key: &str) -> Result<String, String> {
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
    let token = std::env::var(env_key)
        .map_err(|_| format!("--token, --token-file, or {} is required", env_key))?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(format!("{} cannot be empty", env_key));
    }
    Ok(token)
}

/// Return a short non-secret prefix of a token, e.g. `wc_abcd…`. Never
/// returns enough to reconstruct the token.
fn token_prefix(token: &str) -> String {
    let take = token.chars().take(8).collect::<String>();
    if token.chars().count() > 8 {
        format!("{}…", take)
    } else {
        take
    }
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

/// A single authenticated JSON POST against the server. Reuses
/// `build_admin_request` to construct the path/body for known admin commands,
/// but accepts arbitrary `(path, body)` so setup can issue its own calls.
struct ApiCall<'a> {
    server_url: &'a str,
    token: &'a str,
    path: &'a str,
    body: Value,
}

async fn post_json_authed(call: ApiCall<'_>) -> Result<Value, String> {
    let url = format!("{}{}", call.server_url.trim_end_matches('/'), call.path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let resp = client
        .post(url)
        .bearer_auth(call.token)
        .json(&call.body)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    if !status.is_success() {
        return Err(format_error_body(status.as_u16(), &content_type, &text));
    }
    serde_json::from_str(&text).map_err(|e| {
        format!(
            "failed to parse JSON response: {} (content-type: {})",
            e, content_type
        )
    })
}

/// Format an error response without echoing the bearer token. For JSON
/// errors, surface the server's `error` field (sanitized). For non-JSON
/// errors, report status + content-type only (never the body).
fn format_error_body(status: u16, content_type: &str, body: &str) -> String {
    if content_type
        .split(';')
        .next()
        .is_some_and(|ct| ct.trim().eq_ignore_ascii_case("application/json"))
    {
        if let Ok(value) = serde_json::from_str::<Value>(body) {
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                return format!("request failed: HTTP {}: {}", status, error);
            }
            return format!("request failed: HTTP {}: {}", status, value);
        }
    }
    format!(
        "request failed: HTTP {} (content-type: {})",
        status, content_type
    )
}

fn resolve_pairing_create_token(opts: &PairingCreateOptions) -> Result<String, String> {
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
    if let Some(path) = &opts.env_file {
        let token = read_pairing_server_env_file_value(path, "WEBCODEX_TOKEN")?
            .unwrap_or_default()
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(format!(
                "env file {} does not contain WEBCODEX_TOKEN",
                path.display()
            ));
        }
        return Ok(token);
    }
    let token = std::env::var("WEBCODEX_TOKEN")
        .map_err(|_| {
            "--env-file, --token-file, --token, or WEBCODEX_TOKEN is required".to_string()
        })?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err("WEBCODEX_TOKEN cannot be empty".to_string());
    }
    Ok(token)
}

async fn post_json_unauthed(server_url: &str, path: &str, body: Value) -> Result<Value, String> {
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    if !status.is_success() {
        return Err(format_error_body(status.as_u16(), &content_type, &text));
    }
    serde_json::from_str(&text).map_err(|e| {
        format!(
            "failed to parse JSON response: {} (content-type: {})",
            e, content_type
        )
    })
}

async fn run_pairing_create(opts: PairingCreateOptions) -> Result<String, String> {
    let token = resolve_pairing_create_token(&opts)?;
    let mut body = json!({
        "username": opts.username,
        "client_id": opts.client_id,
        "ttl_secs": opts.ttl_secs,
    });
    if let Some(display_name) = &opts.display_name {
        body["display_name"] = json!(display_name);
    }
    if let Some(name) = &opts.user_token_name {
        body["user_token_name"] = json!(name);
    }
    if let Some(name) = &opts.agent_token_name {
        body["agent_token_name"] = json!(name);
    }
    let value = post_json_authed(ApiCall {
        server_url: &opts.server_url,
        token: &token,
        path: "/api/pairing/create",
        body,
    })
    .await
    .map_err(|e| e.replace(&token, "[redacted]"))?;
    if opts.json {
        let summary = json!({
            "pairing_code": value["pairing_code"],
            "expires_at": value["expires_at"],
            "username": value["username"],
            "client_id": value["client_id"],
        });
        serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
    } else {
        let mut out = String::new();
        out.push_str("Pairing code created.\n\n");
        out.push_str(&format!(
            "  username:     {}\n",
            value["username"].as_str().unwrap_or("unknown")
        ));
        out.push_str(&format!(
            "  client_id:    {}\n",
            value["client_id"].as_str().unwrap_or("unknown")
        ));
        out.push_str(&format!(
            "  expires_at:   {}\n",
            value["expires_at"]
                .as_i64()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
        out.push_str(&format!(
            "  pairing code: {}\n",
            value["pairing_code"].as_str().unwrap_or("")
        ));
        out.push_str(
            "\nCopy the pairing code to the client and run `webcodex-cli client enroll`.\n",
        );
        out.push_str("No wc_pat_* or wc_agent_* token files were created on the server.\n");
        Ok(out)
    }
}

fn ensure_enroll_outputs_available(opts: &ClientEnrollOptions) -> Result<(), String> {
    if opts.overwrite {
        return Ok(());
    }
    for path in [
        opts.output_dir.join("webcodex-user-token"),
        opts.output_dir.join("webcodex-agent-token"),
        opts.agent_config.clone(),
    ] {
        if path.exists() {
            return Err(format!(
                "{} already exists; pass --overwrite to replace it",
                path.display()
            ));
        }
    }
    Ok(())
}

async fn run_client_enroll(opts: ClientEnrollOptions) -> Result<String, String> {
    ensure_enroll_outputs_available(&opts)?;
    let mut body = json!({
        "pairing_code": opts.pairing_code,
        "client_id": opts.client_id,
        "transport": opts.transport,
        "projects_dir": opts.projects_dir.to_string_lossy(),
        "allow_cwd_anywhere": opts.allow_cwd_anywhere,
    });
    if let Some(display_name) = &opts.display_name {
        body["display_name"] = json!(display_name);
    }
    if !opts.allowed_roots.is_empty() {
        body["allowed_roots"] = json!(opts
            .allowed_roots
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>());
    }
    let value = post_json_unauthed(&opts.server_url, "/api/pairing/enroll", body).await?;
    let user_token = value
        .get("user_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "enroll response missing user_token".to_string())?
        .to_string();
    let agent_token = value
        .get("agent_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "enroll response missing agent_token".to_string())?
        .to_string();
    let username = value
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let user_prefix = value
        .get("user_token_prefix")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| token_prefix(&user_token));
    let agent_prefix = value
        .get("agent_token_prefix")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| token_prefix(&agent_token));

    let user_token_path = opts.output_dir.join("webcodex-user-token");
    let agent_token_path = opts.output_dir.join("webcodex-agent-token");
    write_text_file(
        &user_token_path,
        &format!("{}\n", user_token),
        opts.overwrite,
        true,
    )?;
    write_text_file(
        &agent_token_path,
        &format!("{}\n", agent_token),
        opts.overwrite,
        true,
    )?;
    let agent_opts = AgentInitOptions {
        server_url: opts.server_url.clone(),
        token: Some(agent_token.clone()),
        token_file: None,
        client_id: opts.client_id.clone(),
        owner: username.clone(),
        display_name: opts.display_name.clone(),
        transport: opts.transport.clone(),
        poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
        projects_dir: opts.projects_dir.clone(),
        output: opts.agent_config.clone(),
        allowed_roots: opts.allowed_roots.clone(),
        allow_cwd_anywhere: opts.allow_cwd_anywhere,
        overwrite: opts.overwrite,
    };
    run_agent_init(agent_opts)?;

    if opts.json {
        let summary = json!({
            "username": username,
            "client_id": opts.client_id,
            "user_token_prefix": user_prefix,
            "agent_token_prefix": agent_prefix,
            "user_token_file": user_token_path.to_string_lossy(),
            "agent_token_file": agent_token_path.to_string_lossy(),
            "agent_config": opts.agent_config.to_string_lossy(),
            "projects_dir": opts.projects_dir.to_string_lossy(),
            "next_steps": [
                "start webcodex-agent with the generated agent.toml",
                "configure GPT Actions with the user-token file"
            ],
        });
        serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
    } else {
        let mut out = String::new();
        out.push_str("Client enrollment complete.\n\n");
        out.push_str(&format!("  username:          {}\n", username));
        out.push_str(&format!("  client_id:         {}\n", opts.client_id));
        out.push_str(&format!("  user token prefix: {}\n", user_prefix));
        out.push_str(&format!("  agent token prefix:{}\n", agent_prefix));
        out.push_str(&format!(
            "  user token file:   {}\n",
            user_token_path.display()
        ));
        out.push_str(&format!(
            "  agent token file:  {}\n",
            agent_token_path.display()
        ));
        out.push_str(&format!(
            "  agent config:      {}\n",
            opts.agent_config.display()
        ));
        out.push_str("\nNext steps:\n");
        out.push_str(&format!(
            "  - Start the agent: `webcodex-agent --config {}`\n",
            opts.agent_config.display()
        ));
        out.push_str(&format!(
            "  - GPT Actions should use the user-token file: {}\n",
            user_token_path.display()
        ));
        Ok(out)
    }
}

#[derive(Debug, Clone)]
struct DoctorCheck {
    name: String,
    status: &'static str,
    detail: String,
}

impl DoctorCheck {
    fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "PASS",
            detail: detail.into(),
        }
    }

    fn warn(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "WARN",
            detail: detail.into(),
        }
    }

    fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "FAIL",
            detail: detail.into(),
        }
    }
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

async fn http_post_json_status(
    server_url: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> Result<(u16, String, Option<Value>), String> {
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let mut req = client.post(url).json(&body);
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;
    let status = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    let json = if content_type
        .split(';')
        .next()
        .is_some_and(|ct| ct.trim().eq_ignore_ascii_case("application/json"))
    {
        serde_json::from_str::<Value>(&text).ok()
    } else {
        None
    };
    Ok((status, content_type, json))
}

async fn http_get_json_status(
    server_url: &str,
    path: &str,
) -> Result<(u16, String, Option<Value>), String> {
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;
    let status = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    let json = if content_type
        .split(';')
        .next()
        .is_some_and(|ct| ct.trim().eq_ignore_ascii_case("application/json"))
    {
        serde_json::from_str::<Value>(&text).ok()
    } else {
        None
    };
    Ok((status, content_type, json))
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

async fn run_doctor(opts: DoctorOptions) -> Result<(String, bool), String> {
    let mut checks = Vec::new();
    for name in ["webcodex", "webcodex-agent", "webcodex-cli"] {
        match discover_binary(name) {
            Some(path) => checks.push(DoctorCheck::pass(
                format!("binary {}", name),
                path.display().to_string(),
            )),
            None => checks.push(DoctorCheck::warn(
                format!("binary {}", name),
                "not found in PATH",
            )),
        }
    }

    // Local agent-config doctor (shell profiles / projects). Runs without
    // contacting the server; never prints init_script bodies or env values.
    if let Some(agent_config) = opts.agent_config.as_deref() {
        checks.extend(run_local_agent_doctor(agent_config));
    } else {
        checks.push(DoctorCheck::warn(
            "agent config",
            "--agent-config not provided; skipped local shell-profile/project checks",
        ));
    }

    let general_token = resolve_doctor_general_token(&opts)?;
    let user_token = read_optional_token(&opts.user_token_file, "--user-token-file")?;
    let agent_token = read_optional_token(&opts.agent_token_file, "--agent-token-file")?;
    let preferred_token = user_token.as_deref().or(general_token.as_deref());

    if let Some(server_url) = opts.server_url.as_deref() {
        match http_post_json_status(
            server_url,
            "/api/runtime/status",
            preferred_token,
            json!({}),
        )
        .await
        {
            Ok((status, _content_type, Some(value))) if (200..300).contains(&status) => {
                let output = value.get("output").unwrap_or(&value);
                let auth_enabled = output.get("auth_enabled").and_then(Value::as_bool);
                let public_url = output
                    .get("configured_public_url")
                    .cloned()
                    .unwrap_or(Value::Null);
                let tools = output.pointer("/tools/count").and_then(Value::as_u64);
                let online = output
                    .pointer("/agents/online_count")
                    .and_then(Value::as_u64);
                checks.push(DoctorCheck::pass(
                    "runtime status",
                    format!(
                        "auth_enabled={:?} configured_public_url={} tools.count={} agents.online_count={}",
                        auth_enabled,
                        public_url,
                        tools.map(|v| v.to_string()).unwrap_or_else(|| "unknown".to_string()),
                        online.map(|v| v.to_string()).unwrap_or_else(|| "unknown".to_string())
                    ),
                ));
            }
            Ok((status, content_type, Some(_))) => checks.push(DoctorCheck::fail(
                "runtime status",
                format!("HTTP {} content-type {}", status, content_type),
            )),
            Ok((status, content_type, None)) => checks.push(DoctorCheck::fail(
                "runtime status",
                format!(
                    "HTTP {} non-JSON response content-type {}",
                    status, content_type
                ),
            )),
            Err(e) => checks.push(DoctorCheck::fail("runtime status", e)),
        }

        match http_get_json_status(server_url, "/openapi.json").await {
            Ok((status, _content_type, Some(value))) if (200..300).contains(&status) => {
                let paths = value["paths"].as_object();
                let op_count: usize = paths
                    .map(|p| {
                        p.values()
                            .map(|m| m.as_object().map(|o| o.len()).unwrap_or(0))
                            .sum()
                    })
                    .unwrap_or(0);
                let forbidden = [
                    "/api/pairing/create",
                    "/api/pairing/enroll",
                    "/api/tokens/create",
                    "/api/agent-tokens/create",
                    "/api/users/create",
                ];
                let leaked: Vec<&str> = forbidden
                    .iter()
                    .copied()
                    .filter(|p| paths.is_some_and(|paths| paths.contains_key(*p)))
                    .collect();
                if leaked.is_empty() {
                    checks.push(DoctorCheck::pass(
                        "openapi",
                        format!(
                            "reachable; operation_count={}; management/enrollment absent",
                            op_count
                        ),
                    ));
                } else {
                    checks.push(DoctorCheck::fail(
                        "openapi",
                        format!("management/enrollment paths exposed: {}", leaked.join(", ")),
                    ));
                }
            }
            Ok((status, content_type, None)) => checks.push(DoctorCheck::fail(
                "openapi",
                format!(
                    "HTTP {} non-JSON response content-type {}",
                    status, content_type
                ),
            )),
            Ok((status, content_type, Some(_))) => checks.push(DoctorCheck::fail(
                "openapi",
                format!("HTTP {} content-type {}", status, content_type),
            )),
            Err(e) => checks.push(DoctorCheck::fail("openapi", e)),
        }

        if let Some(token) = preferred_token {
            match http_post_json_status(
                server_url,
                "/api/tools/call",
                Some(token),
                json!({"tool":"list_agents","params":{}}),
            )
            .await
            {
                Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                    let count = value
                        .pointer("/output/agents")
                        .and_then(Value::as_array)
                        .map(|a| a.len())
                        .unwrap_or(0);
                    checks.push(DoctorCheck::pass(
                        "agent visibility",
                        format!("agents.count={}", count),
                    ));
                }
                Ok((status, content_type, _)) => checks.push(DoctorCheck::warn(
                    "agent visibility",
                    format!("HTTP {} content-type {}", status, content_type),
                )),
                Err(e) => checks.push(DoctorCheck::warn("agent visibility", e)),
            }
            match http_post_json_status(server_url, "/api/projects/list", Some(token), json!({}))
                .await
            {
                Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                    let count = value
                        .pointer("/output/projects")
                        .and_then(Value::as_array)
                        .map(|a| a.len())
                        .unwrap_or(0);
                    checks.push(DoctorCheck::pass(
                        "projects",
                        format!("projects.count={}", count),
                    ));
                }
                Ok((status, content_type, _)) => checks.push(DoctorCheck::warn(
                    "projects",
                    format!("HTTP {} content-type {}", status, content_type),
                )),
                Err(e) => checks.push(DoctorCheck::warn("projects", e)),
            }

            // Basic remote shell roundtrip: run `printf webcodex-doctor-ok`
            // through the run_shell tool on the requested project and verify
            // the marker comes back. Requires --project. Non-strict: a failure
            // is a WARN (the project/agent may be offline). Never prints
            // command output beyond the marker check.
            if let Some(project) = opts.project.as_deref() {
                match http_post_json_status(
                    server_url,
                    "/api/tools/call",
                    Some(token),
                    json!({"tool":"run_shell","params":{"project":project,"command":"printf webcodex-doctor-ok"}}),
                )
                .await
                {
                    Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                        let stdout = value
                            .pointer("/output/stdout")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let exit_code = value
                            .pointer("/output/exit_code")
                            .and_then(Value::as_i64);
                        if stdout.contains("webcodex-doctor-ok") && exit_code == Some(0) {
                            checks.push(DoctorCheck::pass(
                                "shell roundtrip",
                                format!("project '{}' roundtrip ok", project),
                            ));
                        } else {
                            checks.push(DoctorCheck::warn(
                                "shell roundtrip",
                                format!(
                                    "project '{}' returned exit_code={:?} without the expected marker",
                                    project, exit_code
                                ),
                            ));
                        }
                    }
                    Ok((status, content_type, _)) => checks.push(DoctorCheck::warn(
                        "shell roundtrip",
                        format!("HTTP {} content-type {}", status, content_type),
                    )),
                    Err(e) => checks.push(DoctorCheck::warn("shell roundtrip", e)),
                }
            } else {
                checks.push(DoctorCheck::warn(
                    "shell roundtrip",
                    "--project not provided; skipped remote shell roundtrip",
                ));
            }
        } else {
            checks.push(DoctorCheck::warn(
                "tokened checks",
                "no user/bootstrap token provided; skipped agents/projects",
            ));
        }

        if let Some(token) = agent_token.as_deref() {
            match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({}))
                .await
            {
                Ok((status, _, _)) if status == 401 || status == 403 => {
                    checks.push(DoctorCheck::pass(
                        "agent token boundary",
                        "agent token cannot call runtime status",
                    ))
                }
                Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
                    "agent token boundary",
                    format!("unexpected HTTP {} content-type {}", status, content_type),
                )),
                Err(e) => checks.push(DoctorCheck::warn("agent token boundary", e)),
            }
        }
        if let Some(token) = user_token.as_deref() {
            match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({}))
                .await
            {
                Ok((status, _, _)) if (200..300).contains(&status) => checks.push(
                    DoctorCheck::pass("user token boundary", "user token can call runtime status"),
                ),
                Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
                    "user token boundary",
                    format!("HTTP {} content-type {}", status, content_type),
                )),
                Err(e) => checks.push(DoctorCheck::warn("user token boundary", e)),
            }
        }
    } else {
        checks.push(DoctorCheck::warn(
            "server checks",
            "--server-url not provided; skipped HTTP/OpenAPI checks",
        ));
    }

    if opts.quic {
        checks.extend(run_quic_doctor_checks(&opts, preferred_token).await);
    }

    let has_fail = checks.iter().any(|c| c.status == "FAIL");
    if opts.json {
        let summary = json!({
            "ok": !has_fail,
            "strict": opts.strict,
            "checks": checks.iter().map(|c| {
                json!({"name": c.name, "status": c.status, "detail": c.detail})
            }).collect::<Vec<_>>(),
        });
        Ok((
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?,
            has_fail,
        ))
    } else {
        let mut out = String::new();
        out.push_str("WebCodex doctor:\n\n");
        for check in &checks {
            out.push_str(&format!(
                "{} {:<22} {}\n",
                check.status, check.name, check.detail
            ));
        }
        Ok((out, has_fail))
    }
}

/// Run `setup single-user`:
/// 1. Create the user (tolerate an "already exists" JSON error and continue).
/// 2. Create a personal API token for GPT Actions with the GPT scopes.
/// 3. Create an agent token bound to `--client-id` with the agent scopes.
/// 4. Save the returned plaintext tokens to 0600 files under `--output-dir`.
/// 5. Print a concise summary (token prefixes only) or machine JSON.
async fn run_setup_single_user(opts: SetupSingleUserOptions) -> Result<String, String> {
    let admin_opts = AdminOptions {
        server_url: opts.server_url.clone(),
        token: opts.token.clone(),
        token_env: None,
        credential: None,
        credential_env: None,
        token_file: opts.token_file.clone(),
        json: opts.json,
    };
    let bootstrap = resolve_token(&admin_opts, "WEBCODEX_TOKEN")?;
    let server_url = admin_opts.server_url.trim_end_matches('/').to_string();

    // AdminOptions carrying the bootstrap token. `build_admin_request` resolves
    // the token from these options; setup then issues the call with the same
    // bootstrap token (never printed).
    let call_opts = AdminOptions {
        server_url: opts.server_url.clone(),
        token: Some(bootstrap.clone()),
        token_env: None,
        credential: None,
        credential_env: None,
        token_file: None,
        json: opts.json,
    };

    // 1. Create user (tolerate already-exists).
    let mut user_body = json!({
        "username": opts.username,
        "role": opts.role,
    });
    if let Some(display_name) = &opts.display_name {
        user_body["display_name"] = json!(display_name);
    }
    let user_result = post_json_authed(ApiCall {
        server_url: &server_url,
        token: &bootstrap,
        path: "/api/users/create",
        body: user_body,
    })
    .await;
    let user_already_existed = match &user_result {
        Ok(_) => false,
        Err(e) if e.contains("already exists") => true,
        Err(e) => return Err(e.clone()),
    };

    // 2. Create GPT Actions personal API token.
    let gpt_create = AdminCliCommand::TokensCreate(
        call_opts.clone(),
        admin_cli::TokenCreateArgs {
            username: opts.username.clone(),
            name: Some(opts.gpt_token_name.clone()),
            scopes: SETUP_GPT_SCOPES.iter().map(|s| s.to_string()).collect(),
        },
    );
    let gpt_req = build_admin_request(&gpt_create)
        .map_err(|e| format!("internal: failed to build tokens create: {}", e))?;
    let gpt_resp = post_json_authed(ApiCall {
        server_url: &server_url,
        token: &bootstrap,
        path: gpt_req.path,
        body: gpt_req.body,
    })
    .await?;
    let user_token = gpt_resp
        .get("token")
        .and_then(Value::as_str)
        .ok_or_else(|| "tokens create response missing plaintext token".to_string())?
        .to_string();

    // 3. Create agent token bound to client_id.
    let agent_create = AdminCliCommand::AgentTokensCreate(
        call_opts.clone(),
        admin_cli::AgentTokenCreateArgs {
            username: opts.username.clone(),
            client_id: opts.client_id.clone(),
            name: Some(opts.agent_token_name.clone()),
            scopes: SETUP_AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
        },
    );
    let agent_req = build_admin_request(&agent_create)
        .map_err(|e| format!("internal: failed to build agent-tokens create: {}", e))?;
    let agent_resp = post_json_authed(ApiCall {
        server_url: &server_url,
        token: &bootstrap,
        path: agent_req.path,
        body: agent_req.body,
    })
    .await?;
    let agent_token = agent_resp
        .get("token")
        .and_then(Value::as_str)
        .ok_or_else(|| "agent-tokens create response missing plaintext token".to_string())?
        .to_string();

    // 4. Persist tokens to 0600 files.
    let user_token_path = opts.output_dir.join("webcodex-user-token");
    let agent_token_path = opts.output_dir.join("webcodex-agent-token");
    write_secret_file(&user_token_path, &format!("{}\n", user_token))?;
    write_secret_file(&agent_token_path, &format!("{}\n", agent_token))?;

    // 5. Summary. Never print full tokens or the bootstrap token.
    if opts.json {
        let summary = json!({
            "username": opts.username,
            "client_id": opts.client_id,
            "role": opts.role,
            "user_already_existed": user_already_existed,
            "gpt_token_name": opts.gpt_token_name,
            "agent_token_name": opts.agent_token_name,
            "user_token_prefix": token_prefix(&user_token),
            "agent_token_prefix": token_prefix(&agent_token),
            "user_token_path": user_token_path.to_string_lossy(),
            "agent_token_path": agent_token_path.to_string_lossy(),
        });
        serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
    } else {
        let mut out = String::new();
        out.push_str("Setup complete.\n\n");
        out.push_str(&format!("  username:           {}\n", opts.username));
        out.push_str(&format!("  client_id:          {}\n", opts.client_id));
        if user_already_existed {
            out.push_str("  user:               already existed (reused)\n");
        }
        out.push_str(&format!(
            "  gpt token prefix:   {}\n",
            token_prefix(&user_token)
        ));
        out.push_str(&format!(
            "  agent token prefix: {}\n",
            token_prefix(&agent_token)
        ));
        out.push_str(&format!(
            "  user token file:    {}\n",
            user_token_path.display()
        ));
        out.push_str(&format!(
            "  agent token file:   {}\n",
            agent_token_path.display()
        ));
        out.push_str("\nNext steps:\n");
        out.push_str(&format!(
            "  - GPT Actions: use the personal API token in {} as the Bearer token.\n",
            user_token_path.display()
        ));
        out.push_str(&format!(
            "  - Agent: run `webcodex-cli agent init --server-url {} --token <wc_agent_token> --client-id {} --owner {} ...` using the agent token in {}.\n",
            server_url, opts.client_id, opts.username, agent_token_path.display()
        ));
        Ok(out)
    }
}

fn run_server_init(opts: ServerInitOptions) -> Result<String, String> {
    let token = generate_bootstrap_token();
    let env_content = render_server_env(&opts, &token);
    write_text_file(&opts.env_file, &env_content, opts.overwrite, true)?;
    if opts.output_stdout {
        return Ok(env_content);
    }
    if opts.json {
        let summary = json!({
            "env_file": opts.env_file.to_string_lossy(),
            "listen": opts.listen,
            "data_dir": opts.data_dir.to_string_lossy(),
            "public_url": opts.public_url,
            "token_prefix": token_prefix(&token),
            "wrote_env_file": true,
            "next_steps": [
                "install service",
                "start service",
                "run server status",
                "configure HTTPS/public URL separately if using GPT Actions"
            ],
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    out.push_str("Server bootstrap initialized.\n\n");
    out.push_str(&format!("  env file:     {}\n", opts.env_file.display()));
    out.push_str(&format!("  listen:       {}\n", opts.listen));
    out.push_str(&format!("  data dir:     {}\n", opts.data_dir.display()));
    if let Some(public_url) = &opts.public_url {
        out.push_str(&format!("  public URL:   {}\n", public_url.trim()));
    } else {
        out.push_str("  public URL:   not configured\n");
    }
    out.push_str(&format!("  token prefix: {}\n", token_prefix(&token)));
    out.push_str("\nNext steps:\n");
    out.push_str("  - Install the service: `webcodex-cli server install-service ...`\n");
    out.push_str(
        "  - Start it: `sudo systemctl daemon-reload && sudo systemctl enable --now webcodex`\n",
    );
    out.push_str("  - Check it: `webcodex-cli server status ...`\n");
    out.push_str("  - For GPT Actions, configure a public HTTPS URL separately.\n");
    out.push_str("\nNo user API tokens or agent tokens were created.\n");
    Ok(out)
}

fn run_server_install_service(opts: ServerInstallServiceOptions) -> Result<String, String> {
    let unit = render_systemd_unit(&opts);
    let writes_file = !opts.dry_run && !opts.output_stdout;
    if writes_file {
        if opts.service_file.exists() && !opts.overwrite {
            return Err(format!(
                "{} already exists; pass --overwrite to replace it",
                opts.service_file.display()
            ));
        }
        if !is_systemd_platform() {
            return Err(
                "systemd was not detected; use --dry-run or --output - to render the unit"
                    .to_string(),
            );
        }
        write_text_file(&opts.service_file, &unit, opts.overwrite, false)?;
    }
    if opts.output_stdout || opts.dry_run {
        if opts.json {
            let summary = json!({
                "service_file": opts.service_file.to_string_lossy(),
                "env_file": opts.env_file.to_string_lossy(),
                "bin": opts.bin.to_string_lossy(),
                "dry_run": true,
                "unit": unit,
            });
            return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
        }
        return Ok(unit);
    }
    if opts.json {
        let summary = json!({
            "service_file": opts.service_file.to_string_lossy(),
            "env_file": opts.env_file.to_string_lossy(),
            "bin": opts.bin.to_string_lossy(),
            "wrote_service_file": true,
            "next_steps": [
                "sudo systemctl daemon-reload",
                "sudo systemctl enable --now webcodex",
                "sudo systemctl status webcodex"
            ],
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    out.push_str("Service unit installed.\n\n");
    out.push_str(&format!(
        "  service file: {}\n",
        opts.service_file.display()
    ));
    out.push_str(&format!("  env file:     {}\n", opts.env_file.display()));
    out.push_str(&format!("  binary:       {}\n", opts.bin.display()));
    out.push_str("\nNext steps:\n");
    out.push_str("  - sudo systemctl daemon-reload\n");
    out.push_str("  - sudo systemctl enable --now webcodex\n");
    out.push_str("  - sudo systemctl status webcodex\n");
    Ok(out)
}

fn run_agent_install_service(opts: AgentInstallServiceOptions) -> Result<String, String> {
    let unit = render_agent_systemd_unit(&opts);
    let writes_file = !opts.dry_run && !opts.output_stdout;
    if writes_file {
        if opts.service_file.exists() && !opts.overwrite {
            return Err(format!(
                "{} already exists; pass --overwrite to replace it",
                opts.service_file.display()
            ));
        }
        if !is_systemd_platform() {
            return Err(
                "systemd was not detected; use --dry-run or --output - to render the unit"
                    .to_string(),
            );
        }
        write_text_file(&opts.service_file, &unit, opts.overwrite, false)?;
    }
    if opts.output_stdout || opts.dry_run {
        if opts.json {
            let summary = json!({
                "service_file": opts.service_file.to_string_lossy(),
                "config": opts.config.to_string_lossy(),
                "bin": opts.bin.to_string_lossy(),
                "dry_run": true,
                "unit": unit,
            });
            return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
        }
        return Ok(unit);
    }
    if opts.json {
        let summary = json!({
            "service_file": opts.service_file.to_string_lossy(),
            "config": opts.config.to_string_lossy(),
            "bin": opts.bin.to_string_lossy(),
            "wrote_service_file": true,
            "next_steps": [
                "sudo systemctl daemon-reload",
                "sudo systemctl enable --now webcodex-agent",
                "sudo systemctl status webcodex-agent"
            ],
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    out.push_str("Agent service unit installed.\n\n");
    out.push_str(&format!(
        "  service file: {}\n",
        opts.service_file.display()
    ));
    out.push_str(&format!("  config:       {}\n", opts.config.display()));
    out.push_str(&format!("  binary:       {}\n", opts.bin.display()));
    out.push_str("\nNext steps:\n");
    out.push_str("  - sudo systemctl daemon-reload\n");
    out.push_str("  - sudo systemctl enable --now webcodex-agent\n");
    out.push_str("  - sudo systemctl status webcodex-agent\n");
    Ok(out)
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

#[derive(Debug, Clone)]
struct HttpStatusSummary {
    reachable: bool,
    status_code: Option<u16>,
    content_type: Option<String>,
    error: Option<String>,
    output: Option<Value>,
}

async fn fetch_runtime_status(url: &str, token: Option<&str>) -> Result<HttpStatusSummary, String> {
    let endpoint = format!("{}/api/runtime/status", url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let mut req = client.post(endpoint).json(&json!({}));
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    let resp = match req.send().await {
        Ok(resp) => resp,
        Err(e) => {
            return Ok(HttpStatusSummary {
                reachable: false,
                status_code: None,
                content_type: None,
                error: Some(format!("request failed: {}", e)),
                output: None,
            });
        }
    };
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    if !status.is_success() {
        return Ok(HttpStatusSummary {
            reachable: false,
            status_code: Some(status.as_u16()),
            content_type: Some(content_type),
            error: None,
            output: None,
        });
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "failed to parse JSON response: {} (content-type: {})",
            e, content_type
        )
    })?;
    let output = value.get("output").cloned().or(Some(value));
    Ok(HttpStatusSummary {
        reachable: true,
        status_code: Some(status.as_u16()),
        content_type: Some(content_type),
        error: None,
        output,
    })
}

#[derive(Debug, Clone, Deserialize)]
struct AgentStatusConfig {
    #[serde(default)]
    server_url: String,
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    projects_dir: Option<PathBuf>,
    #[serde(default)]
    policy: AgentStatusPolicy,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AgentStatusPolicy {
    #[serde(default)]
    allowed_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct AgentConfigMetadata {
    path: PathBuf,
    client_id: String,
    owner: Option<String>,
    display_name: Option<String>,
    transport: Option<String>,
    projects_dir: Option<PathBuf>,
    allowed_roots: Vec<PathBuf>,
    server_url: String,
}

fn read_agent_config_metadata(path: &Path) -> Result<AgentConfigMetadata, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read agent config {}: {}", path.display(), e))?;
    let cfg: AgentStatusConfig = toml::from_str(&content)
        .map_err(|e| format!("failed to parse agent config {}: {}", path.display(), e))?;
    Ok(AgentConfigMetadata {
        path: path.to_path_buf(),
        client_id: cfg.client_id,
        owner: cfg.owner,
        display_name: cfg.display_name,
        transport: cfg.transport,
        projects_dir: cfg.projects_dir,
        allowed_roots: cfg.policy.allowed_roots,
        server_url: cfg.server_url,
    })
}

fn allowed_roots_summary(roots: &[PathBuf]) -> String {
    if roots.is_empty() {
        "0 configured; agent runtime defaults to $HOME when allowed_roots is omitted".to_string()
    } else {
        format!(
            "{} configured: {}",
            roots.len(),
            roots
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn runtime_client_entry<'a>(output: &'a Value, client_id: &str) -> Option<&'a Value> {
    output
        .pointer("/agents/clients")
        .and_then(Value::as_array)
        .and_then(|clients| {
            clients
                .iter()
                .find(|client| client.get("client_id").and_then(Value::as_str) == Some(client_id))
        })
}

fn runtime_client_online(output: &Value, client_id: &str) -> Option<bool> {
    let entry = runtime_client_entry(output, client_id)?;
    entry.get("connected").and_then(Value::as_bool).or_else(|| {
        entry
            .get("status")
            .and_then(Value::as_str)
            .map(|s| s == "online")
    })
}

async fn run_agent_status(opts: AgentStatusOptions) -> Result<String, String> {
    let systemd = query_systemd_service_status("webcodex-agent.service");
    let metadata = read_agent_config_metadata(&opts.config)?;
    let effective_server_url = opts.server_url.clone().or_else(|| {
        let url = metadata.server_url.trim().to_string();
        if url.is_empty() {
            None
        } else {
            Some(url)
        }
    });
    let user_token = read_optional_token(&opts.user_token_file, "--user-token-file")?;
    let agent_token = read_optional_token(&opts.agent_token_file, "--agent-token-file")?;

    let mut runtime_http: Option<HttpStatusSummary> = None;
    let mut client_online: Option<bool> = None;
    if let (Some(server_url), Some(token)) =
        (effective_server_url.as_deref(), user_token.as_deref())
    {
        let http = fetch_runtime_status(server_url, Some(token)).await?;
        if let Some(output) = http.output.as_ref() {
            if !metadata.client_id.trim().is_empty() {
                client_online = runtime_client_online(output, &metadata.client_id);
            }
        }
        runtime_http = Some(http);
    }

    let mut agent_boundary_status: Option<&'static str> = None;
    let mut agent_boundary_detail: Option<String> = None;
    if let (Some(server_url), Some(token)) =
        (effective_server_url.as_deref(), agent_token.as_deref())
    {
        match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({})).await
        {
            Ok((status, content_type, _)) if status == 401 || status == 403 => {
                agent_boundary_status = Some("PASS");
                agent_boundary_detail =
                    Some("agent token cannot call /api/runtime/status".to_string());
                let _ = content_type;
            }
            Ok((status, content_type, _)) => {
                agent_boundary_status = Some("FAIL");
                agent_boundary_detail = Some(format!(
                    "unexpected HTTP {} content-type {}",
                    status, content_type
                ));
            }
            Err(e) => {
                agent_boundary_status = Some("FAIL");
                agent_boundary_detail = Some(e);
            }
        }
    }

    if opts.json {
        let summary = json!({
            "service": {
                "active": systemd.active,
                "enabled": systemd.enabled,
            },
            "config": {
                "path": metadata.path.to_string_lossy(),
                "client_id": metadata.client_id,
                "owner": metadata.owner,
                "display_name": metadata.display_name,
                "transport": metadata.transport,
                "projects_dir": metadata.projects_dir.map(|p| p.to_string_lossy().to_string()),
                "allowed_roots": {
                    "count": metadata.allowed_roots.len(),
                    "summary": allowed_roots_summary(&metadata.allowed_roots),
                    "paths": metadata.allowed_roots.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
                },
                "server_url": metadata.server_url,
            },
            "runtime": runtime_http.as_ref().map(|http| json!({
                "checked": true,
                "reachable": http.reachable,
                "status_code": http.status_code,
                "content_type": http.content_type,
                "error": http.error,
                "client_online": client_online,
            })).unwrap_or_else(|| json!({
                "checked": false,
                "reason": "requires server URL and --user-token-file",
            })),
            "agent_token_boundary": agent_boundary_status.map(|status| json!({
                "checked": true,
                "status": status,
                "detail": agent_boundary_detail,
            })).unwrap_or_else(|| json!({
                "checked": false,
                "reason": "requires server URL and --agent-token-file",
            })),
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }

    let mut out = String::new();
    out.push_str("Agent status:\n\n");
    out.push_str(&format!("  service active:       {}\n", systemd.active));
    out.push_str(&format!("  service enabled:      {}\n", systemd.enabled));
    out.push_str(&format!(
        "  config:               {}\n",
        metadata.path.display()
    ));
    out.push_str(&format!(
        "  client_id:            {}\n",
        if metadata.client_id.trim().is_empty() {
            "unknown"
        } else {
            metadata.client_id.as_str()
        }
    ));
    out.push_str(&format!(
        "  owner:                {}\n",
        metadata.owner.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "  display_name:         {}\n",
        metadata.display_name.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "  transport:            {}\n",
        metadata.transport.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "  projects_dir:         {}\n",
        metadata
            .projects_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "runtime default".to_string())
    ));
    out.push_str(&format!(
        "  allowed_roots:        {}\n",
        allowed_roots_summary(&metadata.allowed_roots)
    ));
    out.push_str(&format!(
        "  server_url:           {}\n",
        if metadata.server_url.trim().is_empty() {
            "unknown"
        } else {
            metadata.server_url.as_str()
        }
    ));
    match runtime_http {
        Some(http) => {
            out.push_str(&format!(
                "  runtime reachable:    {}\n",
                if http.reachable { "yes" } else { "no" }
            ));
            if let Some(code) = http.status_code {
                out.push_str(&format!("  runtime status:       {}\n", code));
            }
            if let Some(content_type) = &http.content_type {
                out.push_str(&format!("  runtime content-type: {}\n", content_type));
            }
            if let Some(error) = &http.error {
                out.push_str(&format!("  runtime error:        {}\n", error));
            }
            out.push_str(&format!(
                "  client online:        {}\n",
                client_online
                    .map(|online| if online { "yes" } else { "no" })
                    .unwrap_or("unknown")
            ));
        }
        None => out.push_str(
            "  runtime check:        skipped (requires server URL and --user-token-file)\n",
        ),
    }
    match agent_boundary_status {
        Some(status) => out.push_str(&format!(
            "  agent token boundary: {} ({})\n",
            status,
            agent_boundary_detail.unwrap_or_else(|| "unknown".to_string())
        )),
        None => out.push_str(
            "  agent token boundary: skipped (requires server URL and --agent-token-file)\n",
        ),
    }
    Ok(out)
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
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
                assert_eq!(
                    stdout,
                    format!("webcodex-cli {}\n", env!("CARGO_PKG_VERSION"))
                );
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
    fn client_enroll_parse_defaults() {
        let opts = parse_client_enroll(&args(&[
            "--server-url",
            "https://example.test",
            "--pairing-code",
            "wc_pair_fake",
            "--client-id",
            "alice-laptop",
        ]))
        .unwrap();
        let default_dir = default_client_output_dir();
        assert_eq!(opts.output_dir, default_dir);
        assert_eq!(opts.agent_config, opts.output_dir.join("agent.toml"));
        assert_eq!(opts.projects_dir, opts.output_dir.join("projects.d"));
        assert_eq!(opts.transport, TRANSPORT_WEBSOCKET);
        assert!(!opts.overwrite);
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
}

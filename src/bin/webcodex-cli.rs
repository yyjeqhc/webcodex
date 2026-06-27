//! `webcodex-cli` — standalone management/setup binary for WebCodex.
//!
//! Phase 5A. Provides users / tokens / agent-tokens management (reusing the
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
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
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
    AgentInit(AgentInitOptions),
    SetupSingleUser(SetupSingleUserOptions),
    ServerInit(ServerInitOptions),
    ServerInstallService(ServerInstallServiceOptions),
    ServerStatus(ServerStatusOptions),
    Exit {
        code: i32,
        stdout: String,
        stderr: String,
    },
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
struct ServerStatusOptions {
    url: String,
    env_file: Option<PathBuf>,
    env_file_explicit: bool,
    token_file: Option<PathBuf>,
    json: bool,
}

fn usage() -> &'static str {
    "Usage: webcodex-cli <COMMAND>\n\n\
     Management/setup commands for WebCodex.\n\n\
     Commands:\n\
       server init                                      Create server env bootstrap file\n\
       server install-service                           Generate/install a systemd unit\n\
       server status                                    Check service and runtime status\n\
       users create/list                                  Manage users\n\
       tokens create/list/revoke                          Manage personal API tokens\n\
       agent-tokens create/list/revoke                    Manage agent tokens\n\
       agent init                                         Generate an agent.toml config\n\
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
        "agent" => parse_agent_subcommand(&args[1..]),
        "setup" => parse_setup_subcommand(&args[1..]),
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

fn parse_agent_subcommand(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: "expected `agent init`\n".to_string(),
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
        "--help" | "-h" => CliAction::Exit {
            code: 0,
            stdout: usage().to_string(),
            stderr: String::new(),
        },
        other => CliAction::Exit {
            code: 2,
            stdout: String::new(),
            stderr: format!("unknown agent subcommand: {}\n", other),
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
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if !dir.is_absolute() {
            continue;
        }
        let candidate = dir.join("webcodex");
        if candidate.is_file() {
            return Some(candidate);
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SystemdStatus {
    active: String,
    enabled: String,
}

fn query_systemd_status() -> SystemdStatus {
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
        active: run_status(&["is-active", "webcodex.service"]),
        enabled: run_status(&["is-enabled", "webcodex.service"]),
    }
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

    #[test]
    fn agent_init_writes_valid_toml_and_refuses_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("agent.toml");
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://v4.example.test/",
            "--token",
            "wc_agent_fake_test_token",
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
            "wc_agent_fake_test_token",
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
            "wc_agent_fake_stdout_token",
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
        assert_eq!(content.matches("wc_agent_fake_stdout_token").count(), 1);
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
            "wc_agent_fake_perms_token",
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
        std::fs::write(&token_file, "wc_agent_fake_file_token\n").unwrap();
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
        assert!(content.contains("wc_agent_fake_file_token"));

        let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_AGENT_TOKEN", "wc_agent_fake_env_token");
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
        assert!(content.contains("wc_agent_fake_env_token"));
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
                "wc_agent_fake_home_token",
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
                    r#"{"success":true,"token":"wc_agent_fake_plaintext_67890","token_id":"at-1"}"#
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
        assert!(!summary.contains("wc_agent_fake_plaintext_67890"));
        assert!(!summary.contains(&bootstrap));
        // Prefixes are present.
        assert!(summary.contains("wc_user_"));
        assert!(summary.contains("wc_agent"));
        assert!(summary.contains("yyjeqhc"));

        // Files written with 0600 and contain the full one-time tokens.
        let user_token = std::fs::read_to_string(tmp.path().join("webcodex-user-token")).unwrap();
        assert_eq!(user_token.trim(), "wc_user_fake_plaintext_12345");
        let agent_token = std::fs::read_to_string(tmp.path().join("webcodex-agent-token")).unwrap();
        assert_eq!(agent_token.trim(), "wc_agent_fake_plaintext_67890");
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
                        r#"{"success":true,"token":"wc_agent_ae_fake_token","token_id":"at-1"}"#,
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

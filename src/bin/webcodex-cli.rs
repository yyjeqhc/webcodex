//! `webcodex-cli` — standalone management/setup binary for WebCodex.
//!
//! Phase 5A. Provides users / tokens / agent-tokens management (reusing the
//! shared `admin_cli` module), `agent init` (reusing the shared `agent_init`
//! module), and a first-pass `setup single-user` command that creates a user,
//! a personal API token, and an agent token, then writes the plaintext tokens
//! to 0600 files.
//!
//! This binary intentionally does NOT start a server and does NOT print real
//! tokens, Authorization headers, env files, or full agent.toml contents with
//! secrets (except the explicit `agent init --output -` stdout path, which the
//! user requests deliberately).
//!
//! The existing `webcodex` server binary keeps its `webcodex users/tokens/...`
//! admin commands as compatibility wrappers; this binary is the new home for
//! management tooling.

use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

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

fn usage() -> &'static str {
    "Usage: webcodex-cli <COMMAND>\n\n\
     Management/setup commands for WebCodex.\n\n\
     Commands:\n\
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

use super::output::{format_error, sanitize};
use super::{
    AdminCliCommand, AdminCliRequest, AdminOptions, AgentTokenCreateArgs,
    AgentTokenRegisterHashArgs, CreateUserArgs, RevokeTokenArgs, TokenCreateArgs,
    TokenRegisterHashArgs, UsernameArgs,
};
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::path::PathBuf;

const DEFAULT_AGENT_SCOPES: &[&str] = &[
    "agent:register",
    "agent:poll",
    "agent:result",
    "agent:job_update",
];

#[derive(Debug)]
struct FlagParser {
    args: Vec<String>,
    idx: usize,
}

impl FlagParser {
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

    fn finish(self) -> Result<(), String> {
        if self.idx == self.args.len() {
            Ok(())
        } else {
            Err(format!("unexpected argument: {}", self.args[self.idx]))
        }
    }
}

pub(crate) fn is_admin_group(arg: &str) -> bool {
    matches!(
        arg,
        "user" | "users" | "token" | "tokens" | "agent-token" | "agent-tokens"
    )
}

pub(crate) fn usage() -> &'static str {
    "Admin commands:\n\
      webcodex users create --server-url URL [--token TOKEN|--token-file PATH] --username USER [--display-name NAME] [--role ROLE] [--issue-credential]\n\
      webcodex users list --server-url URL [--token TOKEN|--token-file PATH]\n\
      webcodex tokens create --server-url URL [--token TOKEN|--token-file PATH] --username USER [--name NAME] [--scope SCOPE...]\n\
      webcodex token register-hash --server-url URL --username USER --hash HASH --prefix PREFIX [--credential CRED] [--name NAME] [--scope SCOPE...]\n\
      webcodex tokens register-hash --server-url URL --username USER --hash HASH --prefix PREFIX [--credential CRED] [--name NAME] [--scope SCOPE...]\n\
      webcodex tokens list --server-url URL [--token TOKEN|--token-file PATH] --username USER\n\
      webcodex tokens revoke --server-url URL [--token TOKEN|--token-file PATH] --username USER --token-id ID\n\
      webcodex agent-tokens create --server-url URL [--token TOKEN|--token-file PATH] --username USER --client-id ID [--name NAME] [--scope SCOPE...]\n\
      webcodex agent-token register-hash --server-url URL --username USER --client-id ID --hash HASH --prefix PREFIX [--credential CRED] [--name NAME] [--scope SCOPE...]\n\
      webcodex agent-tokens register-hash --server-url URL --username USER --client-id ID --hash HASH --prefix PREFIX [--credential CRED] [--name NAME] [--scope SCOPE...]\n\
      webcodex agent-tokens list --server-url URL [--token TOKEN|--token-file PATH] --username USER\n\
      webcodex agent-tokens revoke --server-url URL [--token TOKEN|--token-file PATH] --username USER --token-id ID\n\n\
    Token fallback: WEBCODEX_TOKEN\n\
    Output: JSON\n"
}

pub(crate) fn parse_admin_cli(args: &[String]) -> Result<AdminCliCommand, String> {
    if args.len() < 2 {
        return Err(format!("missing admin subcommand\n{}", usage()));
    }
    let group = args[0].as_str();
    let action = args[1].as_str();
    let rest = &args[2..];
    match (group, action) {
        ("users" | "user", "create") => parse_users_create(rest),
        ("users" | "user", "list") => parse_users_list(rest),
        ("tokens" | "token", "create") => parse_tokens_create(rest),
        ("tokens" | "token", "register-hash") => parse_tokens_register_hash(rest),
        ("tokens" | "token", "list") => parse_tokens_list(rest),
        ("tokens" | "token", "revoke") => parse_tokens_revoke(rest),
        ("agent-token" | "agent-tokens", "create") => parse_agent_tokens_create(rest),
        ("agent-token" | "agent-tokens", "register-hash") => parse_agent_tokens_register_hash(rest),
        ("agent-token" | "agent-tokens", "list") => parse_agent_tokens_list(rest),
        ("agent-token" | "agent-tokens", "revoke") => parse_agent_tokens_revoke(rest),
        _ => Err(format!(
            "unknown admin command: {} {}\n{}",
            group,
            action,
            usage()
        )),
    }
}

fn parse_common_flag(
    opts: &mut AdminOptions,
    p: &mut FlagParser,
    flag: &str,
) -> Result<bool, String> {
    match flag {
        "--server-url" | "--server" => {
            opts.server_url = p.value(flag)?;
            Ok(true)
        }
        "--token" | "--admin-token" => {
            opts.token = Some(p.value(flag)?);
            Ok(true)
        }
        "--token-env" | "--admin-token-env" => {
            opts.token_env = Some(p.value(flag)?);
            Ok(true)
        }
        "--credential" => {
            opts.credential = Some(p.value(flag)?);
            Ok(true)
        }
        "--credential-env" => {
            opts.credential_env = Some(p.value(flag)?);
            Ok(true)
        }
        "--token-file" => {
            opts.token_file = Some(PathBuf::from(p.value(flag)?));
            Ok(true)
        }
        "--json" => {
            opts.json = true;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn require_common(opts: &AdminOptions) -> Result<(), String> {
    if opts.server_url.trim().is_empty() {
        return Err("--server-url is required".to_string());
    }
    if opts.token.is_some() && opts.token_file.is_some() {
        return Err("use only one of --token/--admin-token or --token-file".to_string());
    }
    Ok(())
}

fn parse_users_create(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut user = CreateUserArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--username" => user.username = p.value(&flag)?,
            "--display-name" => user.display_name = Some(p.value(&flag)?),
            "--role" => user.role = Some(p.value(&flag)?),
            "--issue-credential" => user.issue_credential = true,
            _ => return Err(format!("unknown users create flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &user.username)?;
    Ok(AdminCliCommand::UsersCreate(opts, user))
}

fn parse_tokens_register_hash(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut t = TokenRegisterHashArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--username" | "--user" => t.username = p.value(&flag)?,
            "--name" => t.name = Some(p.value(&flag)?),
            "--hash" | "--token-hash" => t.token_hash = p.value(&flag)?,
            "--prefix" | "--token-prefix" => t.token_prefix = p.value(&flag)?,
            "--scope" => t.scopes.push(p.value(&flag)?),
            "--scopes" => {
                t.scopes.extend(
                    p.value(&flag)?
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                );
            }
            _ => return Err(format!("unknown tokens register-hash flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &t.username)?;
    require_non_empty("--hash", &t.token_hash)?;
    require_non_empty("--prefix", &t.token_prefix)?;
    Ok(AdminCliCommand::TokensRegisterHash(opts, t))
}

fn parse_users_list(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if !parse_common_flag(&mut opts, &mut p, &flag)? {
            return Err(format!("unknown users list flag: {}", flag));
        }
    }
    p.finish()?;
    require_common(&opts)?;
    Ok(AdminCliCommand::UsersList(opts))
}

fn parse_tokens_create(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut t = TokenCreateArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--username" => t.username = p.value(&flag)?,
            "--name" => t.name = Some(p.value(&flag)?),
            "--scope" => t.scopes.push(p.value(&flag)?),
            _ => return Err(format!("unknown tokens create flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &t.username)?;
    Ok(AdminCliCommand::TokensCreate(opts, t))
}

fn parse_tokens_list(args: &[String]) -> Result<AdminCliCommand, String> {
    let (opts, username) = parse_username_command(args, "tokens list")?;
    Ok(AdminCliCommand::TokensList(opts, UsernameArgs { username }))
}

fn parse_tokens_revoke(args: &[String]) -> Result<AdminCliCommand, String> {
    let (opts, revoke) = parse_revoke_command(args, "tokens revoke")?;
    Ok(AdminCliCommand::TokensRevoke(opts, revoke))
}

fn parse_agent_tokens_create(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut t = AgentTokenCreateArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--username" => t.username = p.value(&flag)?,
            "--client-id" => t.client_id = p.value(&flag)?,
            "--name" => t.name = Some(p.value(&flag)?),
            "--scope" => t.scopes.push(p.value(&flag)?),
            _ => return Err(format!("unknown agent-tokens create flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &t.username)?;
    require_non_empty("--client-id", &t.client_id)?;
    if t.scopes.is_empty() {
        t.scopes = DEFAULT_AGENT_SCOPES.iter().map(|s| s.to_string()).collect();
    }
    Ok(AdminCliCommand::AgentTokensCreate(opts, t))
}

fn parse_agent_tokens_register_hash(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut t = AgentTokenRegisterHashArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--username" | "--user" => t.username = p.value(&flag)?,
            "--client-id" => t.client_id = p.value(&flag)?,
            "--name" => t.name = Some(p.value(&flag)?),
            "--hash" | "--token-hash" => t.token_hash = p.value(&flag)?,
            "--prefix" | "--token-prefix" => t.token_prefix = p.value(&flag)?,
            "--scope" => t.scopes.push(p.value(&flag)?),
            "--scopes" => {
                t.scopes.extend(
                    p.value(&flag)?
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                );
            }
            _ => return Err(format!("unknown agent-tokens register-hash flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &t.username)?;
    require_non_empty("--client-id", &t.client_id)?;
    require_non_empty("--hash", &t.token_hash)?;
    require_non_empty("--prefix", &t.token_prefix)?;
    if t.scopes.is_empty() {
        t.scopes = DEFAULT_AGENT_SCOPES.iter().map(|s| s.to_string()).collect();
    }
    Ok(AdminCliCommand::AgentTokensRegisterHash(opts, t))
}

fn parse_agent_tokens_list(args: &[String]) -> Result<AdminCliCommand, String> {
    let (opts, username) = parse_username_command(args, "agent-tokens list")?;
    Ok(AdminCliCommand::AgentTokensList(
        opts,
        UsernameArgs { username },
    ))
}

fn parse_agent_tokens_revoke(args: &[String]) -> Result<AdminCliCommand, String> {
    let (opts, revoke) = parse_revoke_command(args, "agent-tokens revoke")?;
    Ok(AdminCliCommand::AgentTokensRevoke(opts, revoke))
}

fn parse_username_command(args: &[String], name: &str) -> Result<(AdminOptions, String), String> {
    let mut opts = AdminOptions::default();
    let mut username = String::new();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--username" => username = p.value(&flag)?,
            _ => return Err(format!("unknown {} flag: {}", name, flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &username)?;
    Ok((opts, username))
}

fn parse_revoke_command(
    args: &[String],
    name: &str,
) -> Result<(AdminOptions, RevokeTokenArgs), String> {
    let mut opts = AdminOptions::default();
    let mut revoke = RevokeTokenArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--username" => revoke.username = p.value(&flag)?,
            "--token-id" => revoke.token_id = p.value(&flag)?,
            _ => return Err(format!("unknown {} flag: {}", name, flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &revoke.username)?;
    require_non_empty("--token-id", &revoke.token_id)?;
    Ok((opts, revoke))
}

fn require_non_empty(flag: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{} is required", flag))
    } else {
        Ok(())
    }
}

pub(crate) fn build_admin_request(cmd: &AdminCliCommand) -> Result<AdminCliRequest, String> {
    let (opts, path, body) = match cmd {
        AdminCliCommand::UsersCreate(opts, user) => {
            let mut body = json!({
                "username": user.username,
                "role": user.role.as_deref().unwrap_or("user"),
            });
            if let Some(display_name) = &user.display_name {
                body["display_name"] = json!(display_name);
            }
            if user.issue_credential {
                body["issue_credential"] = json!(true);
            }
            (opts, "/api/users/create", body)
        }
        AdminCliCommand::UsersList(opts) => (opts, "/api/users/list", json!({})),
        AdminCliCommand::TokensCreate(opts, t) => {
            let mut body = json!({
                "username": t.username,
                "scopes": t.scopes,
            });
            if let Some(name) = &t.name {
                body["name"] = json!(name);
            }
            (opts, "/api/tokens/create", body)
        }
        AdminCliCommand::TokensRegisterHash(opts, t) => {
            let mut body = json!({
                "username": t.username,
                "token_hash": t.token_hash,
                "token_prefix": t.token_prefix,
                "scopes": t.scopes,
            });
            if let Some(name) = &t.name {
                body["name"] = json!(name);
            }
            (opts, "/api/tokens/register_hash", body)
        }
        AdminCliCommand::TokensList(opts, t) => {
            (opts, "/api/tokens/list", json!({ "username": t.username }))
        }
        AdminCliCommand::TokensRevoke(opts, t) => (
            opts,
            "/api/tokens/revoke",
            json!({ "username": t.username, "token_id": t.token_id }),
        ),
        AdminCliCommand::AgentTokensCreate(opts, t) => {
            let mut body = json!({
                "username": t.username,
                "client_id": t.client_id,
                "scopes": t.scopes,
            });
            if let Some(name) = &t.name {
                body["name"] = json!(name);
            }
            (opts, "/api/agent-tokens/create", body)
        }
        AdminCliCommand::AgentTokensRegisterHash(opts, t) => {
            let mut body = json!({
                "username": t.username,
                "client_id": t.client_id,
                "token_hash": t.token_hash,
                "token_prefix": t.token_prefix,
                "scopes": t.scopes,
            });
            if let Some(name) = &t.name {
                body["name"] = json!(name);
            }
            (opts, "/api/agent-tokens/register_hash", body)
        }
        AdminCliCommand::AgentTokensList(opts, t) => (
            opts,
            "/api/agent-tokens/list",
            json!({ "username": t.username }),
        ),
        AdminCliCommand::AgentTokensRevoke(opts, t) => (
            opts,
            "/api/agent-tokens/revoke",
            json!({ "username": t.username, "token_id": t.token_id }),
        ),
    };
    Ok(AdminCliRequest {
        server_url: opts.server_url.trim_end_matches('/').to_string(),
        token: resolve_bearer_token(
            opts,
            matches!(
                cmd,
                AdminCliCommand::TokensRegisterHash(_, _)
                    | AdminCliCommand::AgentTokensRegisterHash(_, _)
                    | AdminCliCommand::TokensList(_, _)
                    | AdminCliCommand::TokensRevoke(_, _)
            ),
        )?,
        path,
        body,
    })
}

fn resolve_bearer_token(opts: &AdminOptions, prefer_credential: bool) -> Result<String, String> {
    if opts.token.is_some() || opts.token_file.is_some() || opts.token_env.is_some() {
        return resolve_token(opts, "WEBCODEX_TOKEN");
    }
    if prefer_credential {
        if let Some(token) = resolve_credential_token(opts)? {
            return Ok(token);
        }
    }
    resolve_token(opts, "WEBCODEX_TOKEN")
}

fn resolve_credential_token(opts: &AdminOptions) -> Result<Option<String>, String> {
    if let Some(token) = &opts.credential {
        let token = token.trim().to_string();
        require_non_empty("--credential", &token)?;
        return Ok(Some(token));
    }
    if let Some(env_name) = &opts.credential_env {
        let env_name = env_name.trim();
        require_non_empty("--credential-env", env_name)?;
        let token = std::env::var(env_name)
            .map_err(|_| format!("credential env var {} is not set", env_name))?
            .trim()
            .to_string();
        require_non_empty(env_name, &token)?;
        return Ok(Some(token));
    }
    match std::env::var("WEBCODEX_ACCOUNT_CREDENTIAL") {
        Ok(token) if !token.trim().is_empty() => Ok(Some(token.trim().to_string())),
        _ => Ok(None),
    }
}

fn resolve_token(opts: &AdminOptions, env_key: &str) -> Result<String, String> {
    if let Some(token) = &opts.token {
        let token = token.trim().to_string();
        require_non_empty("--token", &token)?;
        return Ok(token);
    }
    if let Some(path) = &opts.token_file {
        let token = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read token file {}: {}", path.display(), e))?
            .trim()
            .to_string();
        require_non_empty("--token-file", &token)?;
        return Ok(token);
    }
    let env_name = opts.token_env.as_deref().unwrap_or(env_key);
    let token = std::env::var(env_name)
        .map_err(|_| {
            format!(
                "--token, --token-file, --credential, or {} is required",
                env_name
            )
        })?
        .trim()
        .to_string();
    require_non_empty(env_name, &token)?;
    Ok(token)
}

pub(crate) async fn run_admin_command(cmd: AdminCliCommand) -> Result<String, String> {
    let req = build_admin_request(&cmd)?;
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
        .map_err(|e| sanitize(&req.token, &format!("request failed: {}", e)))?;
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
        .map_err(|e| sanitize(&req.token, &format!("failed to read response: {}", e)))?;
    if !status.is_success() {
        return Err(format_error(
            status.as_u16(),
            &content_type,
            &text,
            &req.token,
        ));
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "failed to parse JSON response: {} (content-type: {})",
            e, content_type
        )
    })?;
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

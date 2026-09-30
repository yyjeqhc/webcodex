use super::output::{format_error, sanitize};
use super::{
    build_server_http_client, AdminCliCommand, AdminCliRequest, AdminOptions, CreateUserArgs,
    OAuthCreateArgs, OAuthRedirectUriArgs, OAuthScopesArgs, OAuthShowArgs, RevokeTokenArgs,
    RunnerTokenCreateArgs, RunnerTokenRegisterHashArgs, TokenCreateArgs, TokenRegisterHashArgs,
    UsernameArgs,
};
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::path::PathBuf;

const DEFAULT_RUNNER_SCOPES: &[&str] = &[
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

pub fn is_admin_group(arg: &str) -> bool {
    matches!(arg, "users" | "tokens" | "runner-tokens" | "oauth")
}

pub fn usage() -> &'static str {
    "Admin commands:\n\
      webcodex users create --server-url URL [--token TOKEN|--token-file PATH] --username USER [--display-name NAME] [--role ROLE] [--issue-credential]\n\
      webcodex users list --server-url URL [--token TOKEN|--token-file PATH]\n\
      webcodex tokens create --server-url URL [--token TOKEN|--token-file PATH] --username USER [--name NAME] [--scope SCOPE...]\n\
      webcodex tokens register-hash --server-url URL --username USER --hash HASH --prefix PREFIX [--credential CRED] [--name NAME] [--scope SCOPE...]\n\
      webcodex tokens list --server-url URL [--token TOKEN|--token-file PATH] --username USER\n\
      webcodex tokens revoke --server-url URL [--token TOKEN|--token-file PATH] --username USER --token-id ID\n\
      webcodex runner-tokens create --server-url URL [--token TOKEN|--token-file PATH] --username USER --client-id ID [--name NAME] [--scope SCOPE...]\n\
      webcodex runner-tokens register-hash --server-url URL --username USER --client-id ID --hash HASH --prefix PREFIX [--credential CRED] [--name NAME] [--scope SCOPE...]\n\
      webcodex runner-tokens list --server-url URL [--token TOKEN|--token-file PATH] --username USER\n\
      webcodex runner-tokens revoke --server-url URL [--token TOKEN|--token-file PATH] --username USER --token-id ID\n\
      webcodex oauth list --server-url URL [--token TOKEN|--token-file PATH]\n\
      webcodex oauth create --server-url URL [--token TOKEN|--token-file PATH] --name NAME --redirect-uri URI [--redirect-uri URI...] (--scope SCOPE... | --scopes A,B | --all-scopes)\n\
      webcodex oauth show --server-url URL [--token TOKEN|--token-file PATH] --client-id ID [--secret-file PATH] [--pat-file PATH]\n\
      webcodex oauth add-redirect-uri --server-url URL [--token TOKEN|--token-file PATH] --client-id ID --redirect-uri URI\n\
      webcodex oauth remove-redirect-uri --server-url URL [--token TOKEN|--token-file PATH] --client-id ID --redirect-uri URI\n\
      webcodex oauth update-scopes --server-url URL [--token TOKEN|--token-file PATH] --client-id ID (--scope SCOPE... | --scopes A,B | --all-scopes)\n\n\
    Token fallback: WEBCODEX_TOKEN\n\
    Proxy: standard HTTP_PROXY/HTTPS_PROXY/ALL_PROXY/NO_PROXY environment by default;\n\
           --proxy http://HOST:PORT overrides it, --no-system-proxy forces direct.\n\
    Output: JSON (`oauth show` prints a human connection panel)\n\
    Note: `oauth` manages the Server-side OAuth client records used by MCP clients.\n\
          Adding or removing a redirect_uri never revokes issued tokens; changing\n\
          scopes does, and reports reauthorization_required. The client secret is\n\
          returned only by `oauth create`; `oauth show` reads it from --secret-file.\n"
}

pub fn parse_admin_cli(args: &[String]) -> Result<AdminCliCommand, String> {
    if args.first().map(String::as_str) == Some("agent-tokens") {
        return Err(
            "webcodex agent-tokens was removed; use webcodex runner-tokens instead".to_string(),
        );
    }
    if args.len() < 2 {
        return Err(format!("missing admin subcommand\n{}", usage()));
    }
    let group = args[0].as_str();
    let action = args[1].as_str();
    let rest = &args[2..];
    match (group, action) {
        ("users", "create") => parse_users_create(rest),
        ("users", "list") => parse_users_list(rest),
        ("tokens", "create") => parse_tokens_create(rest),
        ("tokens", "register-hash") => parse_tokens_register_hash(rest),
        ("tokens", "list") => parse_tokens_list(rest),
        ("tokens", "revoke") => parse_tokens_revoke(rest),
        ("runner-tokens", "create") => parse_runner_tokens_create(rest),
        ("runner-tokens", "register-hash") => parse_runner_tokens_register_hash(rest),
        ("runner-tokens", "list") => parse_runner_tokens_list(rest),
        ("runner-tokens", "revoke") => parse_runner_tokens_revoke(rest),
        ("oauth", "list") => parse_oauth_list(rest),
        ("oauth", "create") => parse_oauth_create(rest),
        ("oauth", "show") => parse_oauth_show(rest),
        ("oauth", "add-redirect-uri") => parse_oauth_redirect_uri(rest, true),
        ("oauth", "remove-redirect-uri") => parse_oauth_redirect_uri(rest, false),
        ("oauth", "update-scopes") => parse_oauth_update_scopes(rest),
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
        "--server-url" => {
            opts.server_url = p.value(flag)?;
            Ok(true)
        }
        "--token" => {
            opts.token = Some(p.value(flag)?);
            Ok(true)
        }
        "--token-env" => {
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
        "--proxy" => {
            opts.server_http.proxy = Some(p.value(flag)?);
            Ok(true)
        }
        "--no-system-proxy" => {
            opts.server_http.no_system_proxy = true;
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
        return Err("use only one of --token or --token-file".to_string());
    }
    opts.server_http.validate()?;
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
            "--username" => t.username = p.value(&flag)?,
            "--name" => t.name = Some(p.value(&flag)?),
            "--hash" => t.token_hash = p.value(&flag)?,
            "--prefix" => t.token_prefix = p.value(&flag)?,
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

fn parse_runner_tokens_create(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut t = RunnerTokenCreateArgs::default();
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
            _ => return Err(format!("unknown runner-tokens create flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &t.username)?;
    require_non_empty("--client-id", &t.client_id)?;
    if t.scopes.is_empty() {
        t.scopes = DEFAULT_RUNNER_SCOPES
            .iter()
            .map(|s| s.to_string())
            .collect();
    }
    Ok(AdminCliCommand::RunnerTokensCreate(opts, t))
}

fn parse_runner_tokens_register_hash(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut t = RunnerTokenRegisterHashArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--username" => t.username = p.value(&flag)?,
            "--client-id" => t.client_id = p.value(&flag)?,
            "--name" => t.name = Some(p.value(&flag)?),
            "--hash" => t.token_hash = p.value(&flag)?,
            "--prefix" => t.token_prefix = p.value(&flag)?,
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
            _ => {
                return Err(format!(
                    "unknown runner-tokens register-hash flag: {}",
                    flag
                ))
            }
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &t.username)?;
    require_non_empty("--client-id", &t.client_id)?;
    require_non_empty("--hash", &t.token_hash)?;
    require_non_empty("--prefix", &t.token_prefix)?;
    if t.scopes.is_empty() {
        t.scopes = DEFAULT_RUNNER_SCOPES
            .iter()
            .map(|s| s.to_string())
            .collect();
    }
    Ok(AdminCliCommand::RunnerTokensRegisterHash(opts, t))
}

fn parse_runner_tokens_list(args: &[String]) -> Result<AdminCliCommand, String> {
    let (opts, username) = parse_username_command(args, "runner-tokens list")?;
    Ok(AdminCliCommand::RunnerTokensList(
        opts,
        UsernameArgs { username },
    ))
}

fn parse_runner_tokens_revoke(args: &[String]) -> Result<AdminCliCommand, String> {
    let (opts, revoke) = parse_revoke_command(args, "runner-tokens revoke")?;
    Ok(AdminCliCommand::RunnerTokensRevoke(opts, revoke))
}

fn parse_oauth_list(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if !parse_common_flag(&mut opts, &mut p, &flag)? {
            return Err(format!("unknown oauth list flag: {}", flag));
        }
    }
    p.finish()?;
    require_common(&opts)?;
    Ok(AdminCliCommand::OAuthClientsList(opts))
}

fn parse_oauth_create(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut create = OAuthCreateArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--name" => create.name = p.value(&flag)?,
            "--redirect-uri" => create.redirect_uris.push(p.value(&flag)?),
            "--scope" => create.scopes.push(p.value(&flag)?),
            "--scopes" => {
                create.scopes.extend(
                    p.value(&flag)?
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                );
            }
            "--all-scopes" => create.all_scopes = true,
            _ => return Err(format!("unknown oauth create flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--name", &create.name)?;
    if create.redirect_uris.is_empty() {
        return Err("oauth create requires at least one --redirect-uri".to_string());
    }
    for uri in &create.redirect_uris {
        require_non_empty("--redirect-uri", uri)?;
    }
    if create.all_scopes && !create.scopes.is_empty() {
        return Err("use only one of --all-scopes or --scope/--scopes".to_string());
    }
    if !create.all_scopes && create.scopes.is_empty() {
        return Err(
            "oauth create requires --scope SCOPE, --scopes A,B, or --all-scopes".to_string(),
        );
    }
    Ok(AdminCliCommand::OAuthCreate(opts, create))
}

fn parse_oauth_show(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut show = OAuthShowArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--client-id" => show.client_id = p.value(&flag)?,
            "--secret-file" => show.secret_file = Some(PathBuf::from(p.value(&flag)?)),
            "--pat-file" => show.pat_file = Some(PathBuf::from(p.value(&flag)?)),
            _ => return Err(format!("unknown oauth show flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--client-id", &show.client_id)?;
    Ok(AdminCliCommand::OAuthShow(opts, show))
}

fn parse_oauth_redirect_uri(args: &[String], add: bool) -> Result<AdminCliCommand, String> {
    let name = if add {
        "oauth add-redirect-uri"
    } else {
        "oauth remove-redirect-uri"
    };
    let mut opts = AdminOptions::default();
    let mut uri = OAuthRedirectUriArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--client-id" => uri.client_id = p.value(&flag)?,
            "--redirect-uri" => uri.redirect_uri = p.value(&flag)?,
            _ => return Err(format!("unknown {} flag: {}", name, flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--client-id", &uri.client_id)?;
    require_non_empty("--redirect-uri", &uri.redirect_uri)?;
    Ok(if add {
        AdminCliCommand::OAuthRedirectUriAdd(opts, uri)
    } else {
        AdminCliCommand::OAuthRedirectUriRemove(opts, uri)
    })
}

fn parse_oauth_update_scopes(args: &[String]) -> Result<AdminCliCommand, String> {
    let mut opts = AdminOptions::default();
    let mut scopes = OAuthScopesArgs::default();
    let mut p = FlagParser::new(args);
    while let Some(flag) = p.next() {
        if parse_common_flag(&mut opts, &mut p, &flag)? {
            continue;
        }
        match flag.as_str() {
            "--client-id" => scopes.client_id = p.value(&flag)?,
            "--scope" => scopes.scopes.push(p.value(&flag)?),
            "--scopes" => {
                scopes.scopes.extend(
                    p.value(&flag)?
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                );
            }
            "--all-scopes" => scopes.all_scopes = true,
            _ => return Err(format!("unknown oauth update-scopes flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--client-id", &scopes.client_id)?;
    if scopes.all_scopes && !scopes.scopes.is_empty() {
        return Err("use only one of --all-scopes or --scope/--scopes".to_string());
    }
    if !scopes.all_scopes && scopes.scopes.is_empty() {
        return Err(
            "oauth update-scopes requires --scope SCOPE, --scopes A,B, or --all-scopes".to_string(),
        );
    }
    Ok(AdminCliCommand::OAuthScopesUpdate(opts, scopes))
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

pub fn build_admin_request(cmd: &AdminCliCommand) -> Result<AdminCliRequest, String> {
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
        AdminCliCommand::RunnerTokensCreate(opts, t) => {
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
        AdminCliCommand::RunnerTokensRegisterHash(opts, t) => {
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
        AdminCliCommand::RunnerTokensList(opts, t) => (
            opts,
            "/api/agent-tokens/list",
            json!({ "username": t.username }),
        ),
        AdminCliCommand::RunnerTokensRevoke(opts, t) => (
            opts,
            "/api/agent-tokens/revoke",
            json!({ "username": t.username, "token_id": t.token_id }),
        ),
        AdminCliCommand::OAuthClientsList(opts) => (opts, "/api/oauth/clients/list", json!({})),
        AdminCliCommand::OAuthCreate(opts, t) => (
            opts,
            "/api/oauth/clients/create",
            json!({
                "name": t.name,
                "redirect_uris": t.redirect_uris,
                "allowed_scopes": t.scopes,
            }),
        ),
        AdminCliCommand::OAuthShow(opts, _) => (opts, "/api/oauth/clients/list", json!({})),
        AdminCliCommand::OAuthRedirectUriAdd(opts, t) => (
            opts,
            "/api/oauth/clients/add_redirect_uri",
            json!({ "client_id": t.client_id, "redirect_uri": t.redirect_uri }),
        ),
        AdminCliCommand::OAuthRedirectUriRemove(opts, t) => (
            opts,
            "/api/oauth/clients/remove_redirect_uri",
            json!({ "client_id": t.client_id, "redirect_uri": t.redirect_uri }),
        ),
        AdminCliCommand::OAuthScopesUpdate(opts, t) => (
            opts,
            "/api/oauth/clients/update_scopes",
            json!({ "client_id": t.client_id, "allowed_scopes": t.scopes }),
        ),
    };
    Ok(AdminCliRequest {
        server_url: opts.server_url.trim_end_matches('/').to_string(),
        server_http: opts.server_http.clone(),
        token: resolve_bearer_token(
            opts,
            matches!(
                cmd,
                AdminCliCommand::TokensRegisterHash(_, _)
                    | AdminCliCommand::RunnerTokensRegisterHash(_, _)
                    | AdminCliCommand::TokensList(_, _)
                    | AdminCliCommand::TokensRevoke(_, _)
            ),
        )?,
        path,
        body,
    })
}

/// Expand `--all-scopes` against the live Server's OAuth discovery document.
/// The Server remains the single source of truth for the grantable scope
/// registry; the CLI never ships a copy that could drift.
async fn resolve_oauth_all_scopes(cmd: AdminCliCommand) -> Result<AdminCliCommand, String> {
    enum ScopeSink {
        Update(AdminOptions, OAuthScopesArgs),
        Create(AdminOptions, OAuthCreateArgs),
    }
    let sink = match cmd {
        AdminCliCommand::OAuthScopesUpdate(opts, scopes) => ScopeSink::Update(opts, scopes),
        AdminCliCommand::OAuthCreate(opts, create) => ScopeSink::Create(opts, create),
        other => return Ok(other),
    };
    let (opts, all_scopes) = match &sink {
        ScopeSink::Update(opts, scopes) => (opts, scopes.all_scopes),
        ScopeSink::Create(opts, create) => (opts, create.all_scopes),
    };
    if !all_scopes {
        return Ok(match sink {
            ScopeSink::Update(opts, scopes) => AdminCliCommand::OAuthScopesUpdate(opts, scopes),
            ScopeSink::Create(opts, create) => AdminCliCommand::OAuthCreate(opts, create),
        });
    }
    let url = format!(
        "{}/.well-known/oauth-authorization-server",
        opts.server_url.trim_end_matches('/')
    );
    let client = build_server_http_client(&opts.server_http)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("failed to read Server OAuth discovery document: {}", e))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "failed to read Server OAuth discovery document: HTTP {}",
            status.as_u16()
        ));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("Server OAuth discovery document is not valid JSON: {}", e))?;
    let supported = value
        .get("scopes_supported")
        .and_then(Value::as_array)
        .ok_or_else(|| "Server OAuth discovery document omitted scopes_supported".to_string())?;
    // `scopes_supported` is the permission registry plus the `offline_access`
    // protocol scope. Only the permission registry may be registered as a
    // client's `allowed_scopes`, so the protocol scope is dropped here.
    let mut resolved: Vec<String> = supported
        .iter()
        .filter_map(Value::as_str)
        .filter(|scope| *scope != "offline_access")
        .map(str::to_string)
        .collect();
    resolved.sort();
    resolved.dedup();
    if resolved.is_empty() {
        return Err("Server OAuth discovery document advertised no grantable scopes".to_string());
    }
    Ok(match sink {
        ScopeSink::Update(opts, mut scopes) => {
            scopes.scopes = resolved;
            scopes.all_scopes = false;
            AdminCliCommand::OAuthScopesUpdate(opts, scopes)
        }
        ScopeSink::Create(opts, mut create) => {
            create.scopes = resolved;
            create.all_scopes = false;
            AdminCliCommand::OAuthCreate(opts, create)
        }
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

pub async fn run_admin_command(cmd: AdminCliCommand) -> Result<String, String> {
    let cmd = resolve_oauth_all_scopes(cmd).await?;
    let req = build_admin_request(&cmd)?;
    let url = format!("{}{}", req.server_url, req.path);
    let client = build_server_http_client(&req.server_http)?;
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
    if let AdminCliCommand::OAuthShow(_, show) = &cmd {
        return render_oauth_connection_panel(&req.server_url, show, &value);
    }
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

fn read_local_secret(path: &Option<PathBuf>, label: &str) -> Result<String, String> {
    let Some(path) = path else {
        return Ok(String::new());
    };
    std::fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .map_err(|e| format!("failed to read {label} from {}: {e}", path.display()))
}

/// Print the ChatGPT/MCP connection panel. The client secret and the WebCodex
/// PAT never come from the Server: the secret is create-only, and the PAT is an
/// operator credential. Both are read from local files when provided.
pub(crate) fn render_oauth_connection_panel(
    server_url: &str,
    show: &OAuthShowArgs,
    list_response: &Value,
) -> Result<String, String> {
    let clients = list_response
        .get("clients")
        .and_then(Value::as_array)
        .ok_or_else(|| "OAuth clients/list omitted clients".to_string())?;
    let client = clients
        .iter()
        .find(|client| {
            client.get("client_id").and_then(Value::as_str) == Some(show.client_id.as_str())
        })
        .ok_or_else(|| {
            format!(
                "OAuth client not found or not manageable by this token: {}",
                show.client_id
            )
        })?;
    let origin = server_url.trim_end_matches('/');
    let secret = read_local_secret(&show.secret_file, "client secret")?;
    let pat = read_local_secret(&show.pat_file, "WebCodex PAT")?;
    let allowed = client
        .get("allowed_scopes")
        .and_then(Value::as_array)
        .map(|scopes| {
            scopes
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let mut out = String::new();
    out.push_str("ChatGPT MCP connection\n\n");
    out.push_str(&format!("  MCP URL                  {origin}/mcp\n"));
    out.push_str("  Auth                     OAuth\n");
    out.push_str(&format!("  Client ID                {}\n", show.client_id));
    out.push_str(&format!(
        "  Client Secret            {}\n",
        if secret.is_empty() {
            "<not provided; pass --secret-file>"
        } else {
            &secret
        }
    ));
    out.push_str(
        "  Authorization            Sign in with the WebCodex PAT (not shared key / boot / runner token)\n",
    );
    out.push_str(&format!(
        "  WebCodex PAT             {}\n",
        if pat.is_empty() {
            "<not provided; pass --pat-file>"
        } else {
            &pat
        }
    ));
    out.push('\n');
    out.push_str(&format!("  Runtime Console          {origin}/runtime\n"));
    out.push_str(&format!(
        "  Authorization Endpoint   {origin}/oauth/authorize\n"
    ));
    out.push_str(&format!(
        "  Token Endpoint           {origin}/oauth/token\n"
    ));
    out.push_str("  Token Auth Method        client_secret_post\n");
    out.push_str(&format!(
        "  Scopes                   {}\n",
        if allowed.is_empty() {
            "<client has no scopes>"
        } else {
            &allowed
        }
    ));
    out.push_str(
        "  (protocol may also send offline_access; keep it out of the client allow-list)\n",
    );
    out.push('\n');
    out.push_str("  Redirect URI\n");
    for (index, uri) in client
        .get("redirect_uris")
        .and_then(Value::as_array)
        .map(|uris| uris.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(Value::as_str)
        .enumerate()
    {
        out.push_str(&format!("    {}) {}\n", index + 1, uri));
    }
    Ok(out)
}

use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::path::PathBuf;

const DEFAULT_AGENT_SCOPES: &[&str] = &[
    "agent:register",
    "agent:poll",
    "agent:result",
    "agent:job_update",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AdminCliCommand {
    UsersCreate(AdminOptions, CreateUserArgs),
    UsersList(AdminOptions),
    TokensCreate(AdminOptions, TokenCreateArgs),
    TokensList(AdminOptions, UsernameArgs),
    TokensRevoke(AdminOptions, RevokeTokenArgs),
    AgentTokensCreate(AdminOptions, AgentTokenCreateArgs),
    AgentTokensList(AdminOptions, UsernameArgs),
    AgentTokensRevoke(AdminOptions, RevokeTokenArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AdminOptions {
    pub(crate) server_url: String,
    pub(crate) token: Option<String>,
    pub(crate) token_file: Option<PathBuf>,
    pub(crate) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CreateUserArgs {
    pub(crate) username: String,
    pub(crate) display_name: Option<String>,
    pub(crate) role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TokenCreateArgs {
    pub(crate) username: String,
    pub(crate) name: Option<String>,
    pub(crate) scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct UsernameArgs {
    pub(crate) username: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RevokeTokenArgs {
    pub(crate) username: String,
    pub(crate) token_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AgentTokenCreateArgs {
    pub(crate) username: String,
    pub(crate) client_id: String,
    pub(crate) name: Option<String>,
    pub(crate) scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdminCliRequest {
    pub(crate) server_url: String,
    pub(crate) token: String,
    pub(crate) path: &'static str,
    pub(crate) body: Value,
}

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
    matches!(arg, "users" | "tokens" | "agent-tokens")
}

pub(crate) fn usage() -> &'static str {
    "Admin commands:\n\
      webcodex users create --server-url URL [--token TOKEN|--token-file PATH] --username USER [--display-name NAME] [--role ROLE]\n\
      webcodex users list --server-url URL [--token TOKEN|--token-file PATH]\n\
      webcodex tokens create --server-url URL [--token TOKEN|--token-file PATH] --username USER [--name NAME] [--scope SCOPE...]\n\
      webcodex tokens list --server-url URL [--token TOKEN|--token-file PATH] --username USER\n\
      webcodex tokens revoke --server-url URL [--token TOKEN|--token-file PATH] --username USER --token-id ID\n\
      webcodex agent-tokens create --server-url URL [--token TOKEN|--token-file PATH] --username USER --client-id ID [--name NAME] [--scope SCOPE...]\n\
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
        ("users", "create") => parse_users_create(rest),
        ("users", "list") => parse_users_list(rest),
        ("tokens", "create") => parse_tokens_create(rest),
        ("tokens", "list") => parse_tokens_list(rest),
        ("tokens", "revoke") => parse_tokens_revoke(rest),
        ("agent-tokens", "create") => parse_agent_tokens_create(rest),
        ("agent-tokens", "list") => parse_agent_tokens_list(rest),
        ("agent-tokens", "revoke") => parse_agent_tokens_revoke(rest),
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
        return Err("use only one of --token or --token-file".to_string());
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
            _ => return Err(format!("unknown users create flag: {}", flag)),
        }
    }
    p.finish()?;
    require_common(&opts)?;
    require_non_empty("--username", &user.username)?;
    Ok(AdminCliCommand::UsersCreate(opts, user))
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
        token: resolve_token(opts, "WEBCODEX_TOKEN")?,
        path,
        body,
    })
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
    let token = std::env::var(env_key)
        .map_err(|_| format!("--token, --token-file, or {} is required", env_key))?
        .trim()
        .to_string();
    require_non_empty(env_key, &token)?;
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

fn format_error(status: u16, content_type: &str, body: &str, token: &str) -> String {
    if content_type
        .split(';')
        .next()
        .is_some_and(|ct| ct.trim().eq_ignore_ascii_case("application/json"))
    {
        if let Ok(value) = serde_json::from_str::<Value>(body) {
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                return sanitize(
                    token,
                    &format!("request failed: HTTP {}: {}", status, error),
                );
            }
            return sanitize(
                token,
                &format!("request failed: HTTP {}: {}", status, value),
            );
        }
    }
    format!(
        "request failed: HTTP {} (content-type: {})",
        status, content_type
    )
}

fn sanitize(token: &str, message: &str) -> String {
    if token.is_empty() {
        message.to_string()
    } else {
        message.replace(token, "[redacted]")
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

    fn request(values: &[&str]) -> AdminCliRequest {
        let cmd = parse_admin_cli(&args(values)).unwrap();
        build_admin_request(&cmd).unwrap()
    }

    #[test]
    fn users_create_builds_request_path_and_body() {
        let req = request(&[
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
        ]);
        assert_eq!(req.server_url, "https://example.test");
        assert_eq!(req.path, "/api/users/create");
        assert_eq!(req.body["username"], "alice");
        assert_eq!(req.body["display_name"], "Alice");
        assert_eq!(req.body["role"], "user");
    }

    #[test]
    fn tokens_create_builds_repeated_scopes() {
        let req = request(&[
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
        ]);
        assert_eq!(req.path, "/api/tokens/create");
        assert_eq!(req.body["username"], "alice");
        assert_eq!(req.body["name"], "chatgpt-action");
        assert_eq!(req.body["scopes"], json!(["runtime:read", "project:write"]));
    }

    #[test]
    fn agent_tokens_create_defaults_agent_scopes() {
        let req = request(&[
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
        ]);
        assert_eq!(req.path, "/api/agent-tokens/create");
        assert_eq!(req.body["username"], "alice");
        assert_eq!(req.body["client_id"], "alice-laptop");
        assert_eq!(
            req.body["scopes"],
            json!([
                "agent:register",
                "agent:poll",
                "agent:result",
                "agent:job_update"
            ])
        );
    }

    #[test]
    fn agent_tokens_create_supports_explicit_scopes() {
        let req = request(&[
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
            "--scope",
            "agent:register",
            "--scope",
            "agent:poll",
        ]);
        assert_eq!(req.body["scopes"], json!(["agent:register", "agent:poll"]));
    }

    #[test]
    fn list_and_revoke_commands_build_expected_requests() {
        let list = request(&[
            "tokens",
            "list",
            "--server-url",
            "https://example.test",
            "--token",
            "fake-admin",
            "--username",
            "alice",
        ]);
        assert_eq!(list.path, "/api/tokens/list");
        assert_eq!(list.body, json!({"username": "alice"}));

        let revoke = request(&[
            "agent-tokens",
            "revoke",
            "--server-url",
            "https://example.test",
            "--token",
            "fake-admin",
            "--username",
            "alice",
            "--token-id",
            "tok-1",
        ]);
        assert_eq!(revoke.path, "/api/agent-tokens/revoke");
        assert_eq!(
            revoke.body,
            json!({"username": "alice", "token_id": "tok-1"})
        );
    }

    #[test]
    fn token_file_is_read() {
        let tmp = tempfile::tempdir().unwrap();
        let token_file = tmp.path().join("token");
        std::fs::write(&token_file, "fake-file-token\n").unwrap();
        let cmd = parse_admin_cli(&args(&[
            "users",
            "list",
            "--server-url",
            "https://example.test",
            "--token-file",
            token_file.to_str().unwrap(),
        ]))
        .unwrap();
        let req = build_admin_request(&cmd).unwrap();
        assert_eq!(req.token, "fake-file-token");
    }

    #[test]
    fn env_token_fallback_is_used() {
        let _guard = crate::config::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_TOKEN", "fake-env-token");
        let cmd = parse_admin_cli(&args(&[
            "users",
            "list",
            "--server-url",
            "https://example.test",
        ]))
        .unwrap();
        let req = build_admin_request(&cmd).unwrap();
        assert_eq!(req.token, "fake-env-token");
        std::env::remove_var("WEBCODEX_TOKEN");
    }

    #[test]
    fn auth_token_is_not_printed_in_error_output() {
        let msg = format_error(
            500,
            "application/json",
            r#"{"error":"bad fake-secret-token"}"#,
            "fake-secret-token",
        );
        assert!(!msg.contains("fake-secret-token"));
        assert!(msg.contains("[redacted]"));
    }

    #[test]
    fn non_json_error_reports_status_and_content_type_without_body() {
        let body = "<html>".repeat(1000);
        let msg = format_error(502, "text/html; charset=utf-8", &body, "fake-admin");
        assert_eq!(
            msg,
            "request failed: HTTP 502 (content-type: text/html; charset=utf-8)"
        );
        assert!(!msg.contains("<html>"));
    }

    #[tokio::test]
    async fn token_create_output_includes_plaintext_once_from_fake_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            let request_lower = request.to_ascii_lowercase();
            assert!(request.starts_with("POST /api/tokens/create "));
            assert!(request_lower.contains("authorization: bearer fake-admin"));
            assert!(request.contains(r#""scopes":["runtime:read"]"#));
            let body = r#"{"success":true,"token":"wc_fake_plaintext_once","token_id":"tok-1"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let cmd = parse_admin_cli(&args(&[
            "tokens",
            "create",
            "--server-url",
            &format!("http://{}", addr),
            "--token",
            "fake-admin",
            "--username",
            "alice",
            "--scope",
            "runtime:read",
        ]))
        .unwrap();
        let output = run_admin_command(cmd).await.unwrap();
        assert_eq!(output.matches("wc_fake_plaintext_once").count(), 1);
        handle.join().unwrap();
    }
}

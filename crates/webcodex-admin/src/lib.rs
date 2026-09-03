use serde_json::Value;
use std::path::PathBuf;

mod commands;
mod http;
mod output;
#[cfg(test)]
mod tests;

pub use commands::{
    build_admin_request, is_admin_group, parse_admin_cli, run_admin_command, usage,
};
pub use http::{build_server_http_client, ServerHttpOptions};

/// Mutex serializing tests that mutate process-wide environment variables.
/// It is exported because tests in both consuming packages use the same
/// dependency instance.
pub static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminCliCommand {
    UsersCreate(AdminOptions, CreateUserArgs),
    UsersList(AdminOptions),
    TokensCreate(AdminOptions, TokenCreateArgs),
    TokensRegisterHash(AdminOptions, TokenRegisterHashArgs),
    TokensList(AdminOptions, UsernameArgs),
    TokensRevoke(AdminOptions, RevokeTokenArgs),
    RunnerTokensCreate(AdminOptions, RunnerTokenCreateArgs),
    RunnerTokensRegisterHash(AdminOptions, RunnerTokenRegisterHashArgs),
    RunnerTokensList(AdminOptions, UsernameArgs),
    RunnerTokensRevoke(AdminOptions, RevokeTokenArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminOptions {
    pub server_url: String,
    pub server_http: ServerHttpOptions,
    pub token: Option<String>,
    pub token_env: Option<String>,
    pub credential: Option<String>,
    pub credential_env: Option<String>,
    pub token_file: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CreateUserArgs {
    pub username: String,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub issue_credential: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenCreateArgs {
    pub username: String,
    pub name: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenRegisterHashArgs {
    pub username: String,
    pub name: Option<String>,
    pub token_hash: String,
    pub token_prefix: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsernameArgs {
    pub username: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RevokeTokenArgs {
    pub username: String,
    pub token_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunnerTokenCreateArgs {
    pub username: String,
    pub client_id: String,
    pub name: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunnerTokenRegisterHashArgs {
    pub username: String,
    pub client_id: String,
    pub name: Option<String>,
    pub token_hash: String,
    pub token_prefix: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminCliRequest {
    pub server_url: String,
    pub server_http: ServerHttpOptions,
    pub token: String,
    pub path: &'static str,
    pub body: Value,
}

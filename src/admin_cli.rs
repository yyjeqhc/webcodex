use serde_json::Value;
use std::path::PathBuf;

#[path = "admin_cli/commands.rs"]
mod commands;
#[path = "admin_cli/output.rs"]
mod output;
#[cfg(test)]
#[path = "admin_cli/tests.rs"]
mod tests;

#[allow(unused_imports)]
pub(crate) use commands::{
    build_admin_request, is_admin_group, parse_admin_cli, run_admin_command, usage,
};

/// Test-only mutex serializing tests that mutate process-wide environment
/// variables. Lives here so the `admin_cli` module is self-contained and can
/// be inlined into the standalone `webcodex-cli` binary as well as compiled
/// inside the main `webcodex` crate. Other test modules acquire it via
/// `crate::admin_cli::TEST_ENV_LOCK`.
#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AdminCliCommand {
    UsersCreate(AdminOptions, CreateUserArgs),
    UsersList(AdminOptions),
    TokensCreate(AdminOptions, TokenCreateArgs),
    TokensRegisterHash(AdminOptions, TokenRegisterHashArgs),
    TokensList(AdminOptions, UsernameArgs),
    TokensRevoke(AdminOptions, RevokeTokenArgs),
    AgentTokensCreate(AdminOptions, AgentTokenCreateArgs),
    AgentTokensRegisterHash(AdminOptions, AgentTokenRegisterHashArgs),
    AgentTokensList(AdminOptions, UsernameArgs),
    AgentTokensRevoke(AdminOptions, RevokeTokenArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AdminOptions {
    pub(crate) server_url: String,
    pub(crate) token: Option<String>,
    pub(crate) token_env: Option<String>,
    pub(crate) credential: Option<String>,
    pub(crate) credential_env: Option<String>,
    pub(crate) token_file: Option<PathBuf>,
    pub(crate) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CreateUserArgs {
    pub(crate) username: String,
    pub(crate) display_name: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) issue_credential: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TokenCreateArgs {
    pub(crate) username: String,
    pub(crate) name: Option<String>,
    pub(crate) scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TokenRegisterHashArgs {
    pub(crate) username: String,
    pub(crate) name: Option<String>,
    pub(crate) token_hash: String,
    pub(crate) token_prefix: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AgentTokenRegisterHashArgs {
    pub(crate) username: String,
    pub(crate) client_id: String,
    pub(crate) name: Option<String>,
    pub(crate) token_hash: String,
    pub(crate) token_prefix: String,
    pub(crate) scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdminCliRequest {
    pub(crate) server_url: String,
    pub(crate) token: String,
    pub(crate) path: &'static str,
    pub(crate) body: Value,
}

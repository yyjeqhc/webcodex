use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::admin_cli::AdminOptions;
use crate::TokenGenerateOptions;

pub(crate) fn generate_bootstrap_token() -> String {
    format!(
        "wc_boot_{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

pub(crate) fn generate_local_api_token() -> String {
    format!(
        "wc_pat_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

pub(crate) fn generate_local_agent_token() -> String {
    format!(
        "wc_agent_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

pub(crate) fn hash_local_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn local_token_prefix(token: &str) -> String {
    token[..token.len().min(16)].to_string()
}

pub(crate) fn render_token_generate(opts: TokenGenerateOptions) -> String {
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

/// Resolve the bootstrap token for setup/admin commands. Order:
/// `--token` > `--token-file` > `WEBCODEX_TOKEN`. Errors never echo the token.
pub(crate) fn resolve_token(opts: &AdminOptions, env_key: &str) -> Result<String, String> {
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
pub(crate) fn token_prefix(token: &str) -> String {
    let take = token.chars().take(8).collect::<String>();
    if token.chars().count() > 8 {
        format!("{}…", take)
    } else {
        take
    }
}

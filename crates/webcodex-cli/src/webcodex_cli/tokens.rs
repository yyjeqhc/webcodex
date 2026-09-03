use sha2::{Digest, Sha256};
use uuid::Uuid;

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

pub(crate) fn generate_local_runner_token() -> String {
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
    let token = if opts.kind == "runner" {
        generate_local_runner_token()
    } else {
        generate_local_api_token()
    };
    let hash = hash_local_token(&token);
    format!(
        "Token:\n{}\n\nHash:\nsha256:{}\n\nPrefix:\n{}\n\n\
This token was generated offline and is not registered with a WebCodex server.\n\
It cannot authenticate until registered through the managed credential flow.\n\
For the hosted shared-key flow, use `webcodex connect`.\n",
        token,
        hash,
        local_token_prefix(&token)
    )
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

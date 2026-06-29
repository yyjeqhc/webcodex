//! Personal Access Token (PAT) and agent token generation, hashing, and
//! validation utilities.
//!
//! These functions are specific to the PAT / agent token / account credential
//! lifecycle. They do **not** perform database lookups — that responsibility
//! belongs to the middleware and `authenticate_bearer`.

use sha2::{Digest, Sha256};

/// Random component length (hex characters) for generated personal API tokens.
/// Two uuid v4 simple values concatenated = 64 hex chars = 256 bits of entropy.
const TOKEN_RANDOM_HEX_LEN: usize = 64;

/// Hash a plaintext token with SHA-256 and return a lowercase hex digest.
///
/// This is the **single** place token hashing is performed; all token lookups
/// go through [`Database::get_api_key_by_hash`] using this digest. The digest
/// is compared by exact SQLite equality on the indexed `key_hash` column rather
/// than a byte-wise comparison in Rust, so a timing leak of the hash does not
/// reveal the secret. We do not currently use a keyed/HMAC hash (no server
/// secret is configured for hashing); this is acceptable for self-hosted use
/// and keeps the dependency surface small. Upgrading to a keyed hash is a
/// drop-in change here if a `WEBCODEX_TOKEN_HASH_KEY` is added later.
pub(crate) fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate a fresh personal API token. Format: `wc_pat_<random>` where
/// `<random>` is 256 bits of hex-encoded randomness. The plaintext token is
/// returned **only** here (at creation time) and is never persisted; only its
/// SHA-256 hash is stored.
pub(crate) fn generate_api_token() -> String {
    let mut random = String::with_capacity(TOKEN_RANDOM_HEX_LEN);
    while random.len() < TOKEN_RANDOM_HEX_LEN {
        random.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    random.truncate(TOKEN_RANDOM_HEX_LEN);
    format!("wc_pat_{}", random)
}

/// Generate a fresh Phase 3 agent token. Format: `wc_agent_<random>` where
/// `<random>` is 256 bits of hex-encoded randomness. The distinct `wc_agent_`
/// prefix makes an agent token immediately recognizable in `token_prefix`
/// displays and logs so operators can tell it apart from a personal API token.
/// The plaintext token is returned **only** here (at creation time) and is
/// never persisted; only its SHA-256 hash is stored.
pub(crate) fn generate_agent_token() -> String {
    let mut random = String::with_capacity(TOKEN_RANDOM_HEX_LEN);
    while random.len() < TOKEN_RANDOM_HEX_LEN {
        random.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    random.truncate(TOKEN_RANDOM_HEX_LEN);
    format!("wc_agent_{}", random)
}

/// Generate a fresh account credential. Format: `wc_acct_<random>`.
pub(crate) fn generate_account_credential() -> String {
    let mut random = String::with_capacity(TOKEN_RANDOM_HEX_LEN);
    while random.len() < TOKEN_RANDOM_HEX_LEN {
        random.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    random.truncate(TOKEN_RANDOM_HEX_LEN);
    format!("wc_acct_{}", random)
}

/// Return a short, display-safe prefix of a token (the first 16 characters,
/// including the `wc_pat_` / `wc_agent_` kind marker). Used for listing tokens
/// without revealing the secret.
pub(crate) fn token_prefix(token: &str) -> String {
    let end = token.len().min(16);
    token[..end].to_string()
}

/// Validate an `allowed_client_id` for an agent token. Applies the same safe-id
/// rules as `client_id`: non-empty, bounded length, ASCII alphanumeric, `-`,
/// `_`, and `.` only. Returns the trimmed value on success.
pub(crate) fn validate_allowed_client_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("allowed_client_id cannot be empty".to_string());
    }
    if value.chars().count() > 80 {
        return Err("allowed_client_id is too long; maximum is 80 characters".to_string());
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(
            "allowed_client_id may only contain ASCII letters, digits, '-', '_', and '.'"
                .to_string(),
        );
    }
    Ok(value.to_string())
}

/// Validate a username against the Phase 2 rules: non-empty, bounded length,
/// only lowercase ASCII letters, digits, `_`, and `-`. No slash, no `..`, no
/// whitespace, no uppercase. Returns the trimmed username on success.
pub(crate) fn validate_username(username: &str) -> Result<String, String> {
    let username = username.trim();
    if username.is_empty() {
        return Err("username cannot be empty".to_string());
    }
    if username.chars().count() > 64 {
        return Err("username is too long; maximum is 64 characters".to_string());
    }
    if username.contains("..") {
        return Err("username cannot contain '..'".to_string());
    }
    for ch in username.chars() {
        let ok = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if !ok {
            return Err(format!(
                "username contains an invalid character '{}'; only lowercase letters, digits, '_' and '-' are allowed",
                ch
            ));
        }
    }
    Ok(username.to_string())
}

/// Validate a role string. Must be `"admin"` or `"user"`.
pub(crate) fn validate_role(role: &str) -> Result<String, String> {
    let role = role.trim();
    match role {
        "admin" | "user" => Ok(role.to_string()),
        _ => Err(format!("role must be 'admin' or 'user', got '{}'", role)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_agent_token_uses_wc_agent_prefix() {
        let token = generate_agent_token();
        assert!(token.starts_with("wc_agent_"));
        assert!(token.len() > "wc_agent_".len() + 32);
    }

    #[test]
    fn generate_api_token_uses_wc_pat_prefix() {
        let token = generate_api_token();
        assert!(token.starts_with("wc_pat_"));
        assert!(token.len() > "wc_pat_".len() + 32);
    }

    #[test]
    fn generate_account_credential_uses_expected_format() {
        let token = generate_account_credential();
        assert!(token.starts_with("wc_acct_"));
        assert_eq!(token.len(), "wc_acct_".len() + 64);
        assert!(token["wc_acct_".len()..]
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn token_prefix_for_agent_token_shows_prefix() {
        let token = generate_agent_token();
        let prefix = token_prefix(&token);
        assert!(prefix.starts_with("wc_agent_"));
        assert_ne!(prefix, token);
        assert_eq!(prefix.len(), 16);
    }

    #[test]
    fn validate_allowed_client_id_accepts_valid_ids() {
        assert_eq!(
            validate_allowed_client_id("alice-laptop").unwrap(),
            "alice-laptop"
        );
        assert_eq!(
            validate_allowed_client_id("my_agent.1").unwrap(),
            "my_agent.1"
        );
    }

    #[test]
    fn validate_allowed_client_id_rejects_invalid_ids() {
        assert!(validate_allowed_client_id("").is_err());
        assert!(validate_allowed_client_id("bad/client").is_err());
        assert!(validate_allowed_client_id("bad client").is_err());
        assert!(validate_allowed_client_id("bad\x00client").is_err());
        assert!(validate_allowed_client_id(&"x".repeat(81)).is_err());
        // Uppercase ASCII letters are allowed, matching the existing client_id
        // validation rules (validate_id uses is_ascii_alphanumeric).
        assert!(validate_allowed_client_id("UPPER").is_ok());
    }

    #[test]
    fn validate_username_accepts_valid() {
        assert_eq!(validate_username("alice").unwrap(), "alice");
        assert_eq!(validate_username("bob_123").unwrap(), "bob_123");
    }

    #[test]
    fn validate_username_rejects_invalid() {
        assert!(validate_username("").is_err());
        assert!(validate_username("Alice").is_err());
        assert!(validate_username("../etc").is_err());
        assert!(validate_username(&"x".repeat(65)).is_err());
    }

    #[test]
    fn validate_role_accepts_known_roles() {
        assert_eq!(validate_role("admin").unwrap(), "admin");
        assert_eq!(validate_role("user").unwrap(), "user");
    }

    #[test]
    fn validate_role_rejects_unknown() {
        assert!(validate_role("superuser").is_err());
        assert!(validate_role("").is_err());
    }

    #[test]
    fn hash_token_is_deterministic() {
        let a = hash_token("test-token");
        let b = hash_token("test-token");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn hash_token_differs_for_different_inputs() {
        let a = hash_token("token-a");
        let b = hash_token("token-b");
        assert_ne!(a, b);
    }
}

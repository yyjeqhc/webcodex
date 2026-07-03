use crate::models::ApiKeyRecord;
use serde_json::{json, Value};

/// Agent token metadata returned by list/revoke. Never includes `key_hash` or
/// the plaintext token. Includes the Phase 3 `kind` and `allowed_client_id`.
pub(super) fn agent_token_summary(key: &ApiKeyRecord) -> Value {
    json!({
        "id": key.id,
        "user_id": key.user_id,
        "name": key.name,
        "token_prefix": key.key_prefix,
        "kind": key.kind(),
        "allowed_client_id": key.allowed_client_id,
        "scopes": key.scopes_vec(),
        "created_at": key.created_at,
        "last_used_at": key.last_used_at,
        "expires_at": key.expires_at,
        "revoked_at": key.revoked_at,
    })
}

pub(super) fn normalize_token_hash(value: &str) -> Result<String, String> {
    let raw = value
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or_else(|| value.trim());
    if raw.len() != 64 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("token_hash must be sha256:<64 hex> or bare 64 hex".to_string());
    }
    Ok(raw.to_ascii_lowercase())
}

pub(super) fn validate_agent_prefix(value: &str) -> Result<String, String> {
    let value = value.trim();
    if !value.starts_with("wc_agent_") {
        return Err("token_prefix must start with wc_agent_".to_string());
    }
    if value.len() <= "wc_agent_".len() || value.len() > 32 {
        return Err("token_prefix length is invalid".to_string());
    }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("token_prefix contains invalid characters".to_string());
    }
    Ok(value.to_string())
}

pub(super) fn is_unique_constraint_error(e: &anyhow::Error) -> bool {
    e.to_string()
        .to_ascii_lowercase()
        .contains("unique constraint failed")
}

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

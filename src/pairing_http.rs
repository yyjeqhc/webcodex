//! Single-use pairing-code lifecycle for Runner enrollment. Operational
//! authorization is independent of MCP tool discovery.

#[cfg(test)]
mod http_tests;
mod runner_capabilities;
pub(crate) use runner_capabilities::{grant_runner_capabilities, runner_capability_authorization};

use crate::auth::{
    clean_token_name, generate_agent_token, generate_api_token, hash_token, scopes_to_string,
    token_prefix, validate_allowed_client_id, validate_username, AuthContext,
};
use crate::db::PairingConsumeResult;
use crate::json_error;
use crate::management_http::{auth_context, database, parse_json, JsonApiError, JsonApiResult};
use crate::models::{
    ApiKeyRecord, PairingCodeRecord, UserRecord, TOKEN_KIND_AGENT, TOKEN_KIND_USER,
};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use webcodex_core::authority::{SCOPE_CODING_AGENT_RUN, SCOPE_SSH_LOCAL};

const DEFAULT_TTL_SECS: i64 = 600;
const MIN_TTL_SECS: i64 = 60;
const MAX_TTL_SECS: i64 = 3600;

use webcodex_core::authority::{
    profiles::LOCAL_USER as ENROLL_USER_SCOPES, AGENT_SCOPES as ENROLL_AGENT_SCOPES,
};
#[derive(Debug, Deserialize)]
pub(crate) struct PairingCreateRequest {
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Optional. Empty issues an unbound code that any device can claim.
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub ttl_secs: Option<i64>,
    #[serde(default)]
    pub user_token_name: Option<String>,
    #[serde(default)]
    pub agent_token_name: Option<String>,
    #[serde(default)]
    pub overwrite_existing_user: bool,
    /// Admin-controlled issuance, never accepted as an enrollment-time upgrade.
    #[serde(default)]
    pub runner_capabilities: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PairingEnrollRequest {
    pub pairing_code: String,
    pub client_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub transport: Option<String>,
}

fn enrollment_user_scopes(runner_capabilities: bool) -> Vec<String> {
    let mut scopes: Vec<_> = ENROLL_USER_SCOPES
        .iter()
        .map(|scope| scope.to_string())
        .collect();
    if runner_capabilities {
        scopes.extend([
            SCOPE_SSH_LOCAL.to_owned(),
            SCOPE_CODING_AGENT_RUN.to_owned(),
        ]);
    }
    scopes
}

fn clean_display_name(value: Option<String>) -> Result<Option<String>, String> {
    let value = value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(v) = value.as_ref() {
        if v.chars().count() > 128 {
            return Err("display_name is too long".to_string());
        }
    }
    Ok(value)
}

fn resolve_pairing_agent_token_name(
    value: Option<String>,
    client_id: &str,
) -> Result<Option<String>, String> {
    // For an unbound code, defer the default until enrollment knows the claimed
    // client_id. Otherwise the stored fallback would be the meaningless string
    // " agent". Explicit names and bound-code defaults remain unchanged.
    if client_id.is_empty() && value.as_deref().is_none_or(|name| name.trim().is_empty()) {
        return Ok(None);
    }
    clean_token_name(value, &format!("{} agent", client_id)).map(Some)
}

fn generate_pairing_code() -> String {
    format!(
        "wc_pair_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn user_summary(user: &UserRecord) -> Value {
    json!({
        "id": user.id,
        "username": user.username,
        "display_name": user.display_name,
        "role": user.role,
        "disabled": user.is_disabled(),
    })
}

#[handler]
pub(crate) async fn pairing_create(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: PairingCreateRequest = parse_json(req).await?;

    let auth = auth_context(depot)?;
    if auth.is_agent_token() || !auth.is_admin_caller() {
        return Err(JsonApiError::new(
            StatusCode::FORBIDDEN,
            "admin or bootstrap auth required",
        ));
    }

    let username = validate_username(&body.username).map_err(JsonApiError::bad_request)?;
    // An empty client_id issues an unbound code: the device names itself when
    // it redeems, so the same name does not have to be agreed on both sides
    // before anyone can log in.
    let client_id = if body.client_id.trim().is_empty() {
        String::new()
    } else {
        validate_allowed_client_id(&body.client_id).map_err(JsonApiError::bad_request)?
    };
    let display_name = clean_display_name(body.display_name).map_err(JsonApiError::bad_request)?;
    let ttl_secs = body.ttl_secs.unwrap_or(DEFAULT_TTL_SECS);
    if !(MIN_TTL_SECS..=MAX_TTL_SECS).contains(&ttl_secs) {
        return Err(JsonApiError::bad_request(format!(
            "ttl_secs must be between {} and {}",
            MIN_TTL_SECS, MAX_TTL_SECS
        )));
    }
    let user_token_name = clean_token_name(body.user_token_name, "chatgpt-action")
        .map_err(JsonApiError::bad_request)?;
    let agent_token_name = resolve_pairing_agent_token_name(body.agent_token_name, &client_id)
        .map_err(JsonApiError::bad_request)?;

    let db = database(depot)?;

    let now = chrono::Utc::now().timestamp();
    let user = match db
        .get_user_by_username(&username)
        .map_err(JsonApiError::internal)?
    {
        Some(user) => {
            if user.is_disabled() {
                return Err(JsonApiError::new(StatusCode::FORBIDDEN, "user is disabled"));
            }
            let _ = body.overwrite_existing_user;
            user
        }
        None => {
            let user = UserRecord {
                id: uuid::Uuid::new_v4().to_string(),
                username: username.clone(),
                created_at: now,
                disabled: 0,
                display_name: display_name.clone(),
                role: "user".to_string(),
                disabled_at: None,
                updated_at: Some(now),
            };
            db.create_user(&user).map_err(JsonApiError::internal)?;
            user
        }
    };

    let pairing_code = generate_pairing_code();
    let expires_at = now + ttl_secs;
    let record = PairingCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash: hash_token(&pairing_code),
        user_id: user.id.clone(),
        username: user.username.clone(),
        client_id: client_id.clone(),
        created_at: now,
        expires_at,
        used_at: None,
        user_token_name: Some(user_token_name),
        agent_token_name,
        runner_capabilities: body.runner_capabilities,
    };
    db.insert_pairing_code(&record)
        .map_err(JsonApiError::internal)?;

    Ok(Json(json!({
        "success": true,
        "pairing_code": pairing_code,
        "expires_at": expires_at,
        "username": user.username,
        "client_id": client_id,
        "user": user_summary(&user),
        "runner_capabilities": record.runner_capabilities,
    })))
}

#[handler]
pub(crate) async fn pairing_enroll(req: &mut Request, depot: &mut Depot) -> JsonApiResult {
    let body: PairingEnrollRequest = parse_json(req).await?;
    let pairing_code = body.pairing_code.trim();
    if pairing_code.is_empty() {
        return Err(JsonApiError::new(
            StatusCode::BAD_REQUEST,
            "pairing_code is required",
        ));
    }
    let client_id =
        validate_allowed_client_id(&body.client_id).map_err(JsonApiError::bad_request)?;
    if let Some(transport) = body.transport.as_deref() {
        if !matches!(transport, "websocket" | "polling" | "quic" | "auto") {
            return Err(JsonApiError::new(
                StatusCode::BAD_REQUEST,
                "transport must be websocket, polling, quic, or auto",
            ));
        }
    }
    let _display_name = clean_display_name(body.display_name).map_err(JsonApiError::bad_request)?;
    let db = database(depot)?;
    let now = chrono::Utc::now().timestamp();
    let code_hash = hash_token(pairing_code);
    let record = match db
        .consume_pairing_code(&code_hash, &client_id, now)
        .map_err(JsonApiError::internal)?
    {
        PairingConsumeResult::Consumed(record) => record,
        PairingConsumeResult::ClientMismatch(_) => {
            return Err(JsonApiError::new(
                StatusCode::FORBIDDEN,
                "client_id does not match",
            ));
        }
        PairingConsumeResult::NotFound
        | PairingConsumeResult::AlreadyUsed(_)
        | PairingConsumeResult::Expired(_) => {
            return Err(JsonApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid or expired pairing code",
            ));
        }
    };

    let user = match db
        .get_user_by_id(&record.user_id)
        .map_err(JsonApiError::internal)?
    {
        Some(user) if !user.is_disabled() => user,
        _ => {
            return Err(JsonApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid or expired pairing code",
            ));
        }
    };

    let user_token = generate_api_token();
    let agent_token = generate_agent_token();
    let user_scopes = enrollment_user_scopes(record.runner_capabilities);
    let user_key = ApiKeyRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        name: record
            .user_token_name
            .clone()
            .unwrap_or_else(|| "chatgpt-action".to_string()),
        key_prefix: token_prefix(&user_token),
        created_at: now,
        last_used_at: None,
        revoked_at: None,
        scopes: scopes_to_string(&user_scopes),
        expires_at: None,
        kind: TOKEN_KIND_USER.to_string(),
        allowed_client_id: None,
    };
    let agent_key = ApiKeyRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        name: record
            .agent_token_name
            .clone()
            .unwrap_or_else(|| format!("{} agent", client_id)),
        key_prefix: token_prefix(&agent_token),
        created_at: now,
        last_used_at: None,
        revoked_at: None,
        scopes: scopes_to_string(
            &ENROLL_AGENT_SCOPES
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        ),
        expires_at: None,
        kind: TOKEN_KIND_AGENT.to_string(),
        allowed_client_id: Some(client_id.clone()),
    };
    db.insert_api_key(&user_key, &hash_token(&user_token))
        .map_err(JsonApiError::internal)?;
    db.insert_api_key(&agent_key, &hash_token(&agent_token))
        .map_err(JsonApiError::internal)?;

    Ok(Json(json!({
        "success": true,
        "username": user.username,
        "client_id": client_id,
        "user_token": user_token,
        "agent_token": agent_token,
        "user_token_prefix": user_key.key_prefix,
        "agent_token_prefix": agent_key.key_prefix,
        "user_token_id": user_key.id,
        "agent_token_id": agent_key.id,
        "user_token_scopes": user_scopes,
        "agent_token_scopes": ENROLL_AGENT_SCOPES,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::Database;
    use salvo::prelude::affix_state;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::{Router, Service};
    use std::sync::Arc;
    use webcodex_core::authority::{
        SCOPE_AGENT_JOB_UPDATE, SCOPE_AGENT_POLL, SCOPE_AGENT_REGISTER, SCOPE_AGENT_RESULT,
        SCOPE_JOB_RUN, SCOPE_PROJECT_READ, SCOPE_PROJECT_WRITE, SCOPE_RUNNER_MANAGE,
        SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE,
    };

    fn test_db() -> Database {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("webcodex.db");
        std::mem::forget(tmp);
        Database::open(&path).unwrap()
    }

    #[test]
    fn pairing_create_stores_hash_only() {
        let db = test_db();
        let now = chrono::Utc::now().timestamp();
        let user = UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        let code = "wc_pair_test_secret";
        db.insert_pairing_code(&PairingCodeRecord {
            runner_capabilities: false,
            id: "p-1".to_string(),
            code_hash: hash_token(code),
            user_id: user.id,
            username: "alice".to_string(),
            client_id: "alice-laptop".to_string(),
            created_at: now,
            expires_at: now + 600,
            used_at: None,
            user_token_name: None,
            agent_token_name: None,
        })
        .unwrap();
        let conn = db.conn_for_tests();
        let stored: String = conn
            .query_row(
                "SELECT code_hash FROM pairing_codes WHERE id = 'p-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, hash_token(code));
        assert_ne!(stored, code);
    }

    #[test]
    fn pairing_enroll_consumes_once() {
        let db = test_db();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        let code_hash = hash_token("wc_pair_once");
        db.insert_pairing_code(&PairingCodeRecord {
            runner_capabilities: false,
            id: "p-1".to_string(),
            code_hash: code_hash.clone(),
            user_id: "u-1".to_string(),
            username: "alice".to_string(),
            client_id: "alice-laptop".to_string(),
            created_at: now,
            expires_at: now + 600,
            used_at: None,
            user_token_name: None,
            agent_token_name: None,
        })
        .unwrap();
        let first = db
            .consume_pairing_code(&code_hash, "alice-laptop", now + 1)
            .unwrap();
        match first {
            PairingConsumeResult::Consumed(record) => assert_eq!(record.used_at, Some(now + 1)),
            other => panic!("expected consumed, got {other:?}"),
        }
        let second = db
            .consume_pairing_code(&code_hash, "alice-laptop", now + 2)
            .unwrap();
        match second {
            PairingConsumeResult::AlreadyUsed(record) => assert_eq!(record.used_at, Some(now + 1)),
            other => panic!("expected already used, got {other:?}"),
        }
    }

    #[test]
    fn pairing_expired_and_wrong_client_are_rejected_by_consume() {
        let db = test_db();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        for (id, code, expires_at) in [
            ("p-exp", "wc_pair_exp", now - 1),
            ("p-wrong", "wc_pair_wrong", now + 600),
        ] {
            db.insert_pairing_code(&PairingCodeRecord {
                runner_capabilities: false,
                id: id.to_string(),
                code_hash: hash_token(code),
                user_id: "u-1".to_string(),
                username: "alice".to_string(),
                client_id: "alice-laptop".to_string(),
                created_at: now,
                expires_at,
                used_at: None,
                user_token_name: None,
                agent_token_name: None,
            })
            .unwrap();
        }
        let expired = db
            .consume_pairing_code(&hash_token("wc_pair_exp"), "alice-laptop", now)
            .unwrap();
        assert!(matches!(expired, PairingConsumeResult::Expired(_)));
        let wrong = db
            .consume_pairing_code(&hash_token("wc_pair_wrong"), "other", now)
            .unwrap();
        assert!(matches!(wrong, PairingConsumeResult::ClientMismatch(_)));
    }

    #[test]
    fn unbound_pairing_code_is_claimed_by_whichever_device_redeems_it() {
        // A code created without a client_id lets the device name itself, so
        // the same name no longer has to be agreed on both sides before anyone
        // can log in. The claiming device is recorded on the consumed record.
        let db = test_db();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        db.insert_pairing_code(&PairingCodeRecord {
            runner_capabilities: false,
            id: "p-open".to_string(),
            code_hash: hash_token("wc_pair_open"),
            user_id: "u-1".to_string(),
            username: "alice".to_string(),
            client_id: String::new(),
            created_at: now,
            expires_at: now + 600,
            used_at: None,
            user_token_name: None,
            agent_token_name: None,
        })
        .unwrap();

        let claimed = db
            .consume_pairing_code(&hash_token("wc_pair_open"), "some-laptop", now)
            .unwrap();
        match claimed {
            PairingConsumeResult::Consumed(record) => {
                assert_eq!(record.client_id, "some-laptop", "claiming device recorded");
                assert_eq!(record.username, "alice");
            }
            other => panic!("expected the unbound code to be consumed, got {other:?}"),
        }

        // Still one-time: a second device cannot reuse it.
        let reused = db
            .consume_pairing_code(&hash_token("wc_pair_open"), "other-laptop", now)
            .unwrap();
        assert!(matches!(reused, PairingConsumeResult::AlreadyUsed(_)));
    }

    #[test]
    fn a_bound_pairing_code_still_rejects_a_different_device() {
        let db = test_db();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        db.insert_pairing_code(&PairingCodeRecord {
            runner_capabilities: false,
            id: "p-bound".to_string(),
            code_hash: hash_token("wc_pair_bound"),
            user_id: "u-1".to_string(),
            username: "alice".to_string(),
            client_id: "alice-laptop".to_string(),
            created_at: now,
            expires_at: now + 600,
            used_at: None,
            user_token_name: None,
            agent_token_name: None,
        })
        .unwrap();

        let mismatched = db
            .consume_pairing_code(&hash_token("wc_pair_bound"), "someone-else", now)
            .unwrap();
        assert!(matches!(
            mismatched,
            PairingConsumeResult::ClientMismatch(_)
        ));
        let matched = db
            .consume_pairing_code(&hash_token("wc_pair_bound"), "alice-laptop", now)
            .unwrap();
        assert!(matches!(matched, PairingConsumeResult::Consumed(_)));
    }

    #[tokio::test]
    async fn pairing_enroll_returns_expected_token_kinds_and_scopes() {
        let db = Arc::new(test_db());
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        let code = "wc_pair_endpoint_test";
        db.insert_pairing_code(&PairingCodeRecord {
            runner_capabilities: false,
            id: "p-1".to_string(),
            code_hash: hash_token(code),
            user_id: "u-1".to_string(),
            username: "alice".to_string(),
            client_id: "alice-laptop".to_string(),
            created_at: now,
            expires_at: now + 600,
            used_at: None,
            user_token_name: Some("chatgpt-action".to_string()),
            agent_token_name: Some("alice-laptop agent".to_string()),
        })
        .unwrap();
        let service = Service::new(
            Router::new()
                .hoop(affix_state::inject(db.clone()))
                .push(Router::with_path("api/pairing/enroll").post(pairing_enroll)),
        );
        let mut resp = TestClient::post("http://localhost/api/pairing/enroll")
            .json(&json!({
                "pairing_code": code,
                "client_id": "alice-laptop"
            }))
            .send(&service)
            .await;
        assert_eq!(resp.status_code.unwrap(), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert!(body["user_token"].as_str().unwrap().starts_with("wc_pat_"));
        assert!(body["agent_token"]
            .as_str()
            .unwrap()
            .starts_with("wc_agent_"));
        assert_eq!(
            body["user_token_scopes"],
            json!([
                SCOPE_RUNTIME_READ,
                SCOPE_RUNNER_MANAGE,
                SCOPE_SESSION_COLLABORATE,
                SCOPE_PROJECT_READ,
                SCOPE_PROJECT_WRITE,
                SCOPE_JOB_RUN
            ])
        );
        assert_eq!(
            body["agent_token_scopes"],
            json!([
                SCOPE_AGENT_REGISTER,
                SCOPE_AGENT_POLL,
                SCOPE_AGENT_RESULT,
                SCOPE_AGENT_JOB_UPDATE
            ])
        );
        let user_key = db
            .get_api_key_by_hash(&hash_token(body["user_token"].as_str().unwrap()))
            .unwrap()
            .unwrap();
        let agent_key = db
            .get_api_key_by_hash(&hash_token(body["agent_token"].as_str().unwrap()))
            .unwrap()
            .unwrap();
        assert_eq!(user_key.kind(), TOKEN_KIND_USER);
        assert_eq!(agent_key.kind(), TOKEN_KIND_AGENT);
        assert_eq!(agent_key.allowed_client_id(), Some("alice-laptop"));
    }

    #[test]
    fn unbound_pairing_defers_the_default_agent_token_name_until_claim() {
        assert_eq!(resolve_pairing_agent_token_name(None, "").unwrap(), None);
        assert_eq!(
            resolve_pairing_agent_token_name(Some("  ".to_string()), "").unwrap(),
            None
        );
        assert_eq!(
            resolve_pairing_agent_token_name(Some("custom".to_string()), "").unwrap(),
            Some("custom".to_string())
        );
        assert_eq!(
            resolve_pairing_agent_token_name(None, "alice-laptop").unwrap(),
            Some("alice-laptop agent".to_string())
        );
    }
}

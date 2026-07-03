use super::*;
use crate::auth::hash_token;
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;
use std::path::PathBuf;
use std::sync::Arc;

/// Minimal `Config` for tests (token sets whether auth is enabled).
fn test_config(token: Option<&str>) -> Arc<crate::Config> {
    Arc::new(crate::Config {
        addr: "127.0.0.1:0".to_string(),
        data_dir: PathBuf::from("./data"),
        token: token.map(str::to_string),
        enable_ssh: false,
        max_text_size: 2 * 1024 * 1024,
        max_file_size: 100 * 1024 * 1024,
        codex: crate::CodexConfig::default(),
        oauth2: crate::OAuth2Config::default(),
    })
}

fn test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
    let tmp = tempfile::tempdir().unwrap();
    let db = crate::Database::open(&tmp.path().join("test.db")).unwrap();
    (tmp, Arc::new(db))
}

/// Build a router mirroring the production Phase 2 user/token management
/// wiring: Config + Database injected, AuthMiddleware enforced.
#[salvo::handler]
async fn echo_runtime_ok(res: &mut Response) {
    res.render(Json(json!({"ok": true})));
}

fn build_router(config: Arc<crate::Config>, db: Arc<crate::Database>) -> Router {
    Router::new()
        .hoop(affix_state::inject(config))
        .hoop(affix_state::inject(db))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("users/create").post(users_create))
                .push(Router::with_path("users/list").post(users_list))
                .push(Router::with_path("users/me").post(users_me))
                .push(Router::with_path("tokens/create").post(tokens_create))
                .push(Router::with_path("tokens/register_hash").post(tokens_register_hash))
                .push(Router::with_path("tokens/list").post(tokens_list))
                .push(Router::with_path("tokens/revoke").post(tokens_revoke))
                .push(Router::with_path("runtime/status").post(echo_runtime_ok)),
        )
}

fn effective_status(resp: &Response) -> StatusCode {
    resp.status_code.unwrap_or(StatusCode::OK)
}

/// Bootstrap helper: create a user directly via the DB so tests can mint
/// tokens for them.
fn seed_user(db: &crate::Database, username: &str, role: &str) -> UserRecord {
    let now = chrono::Utc::now().timestamp();
    let user = UserRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.to_string(),
        created_at: now,
        disabled: 0,
        display_name: None,
        role: role.to_string(),
        disabled_at: None,
        updated_at: Some(now),
    };
    db.create_user(&user).unwrap();
    user
}

// =========================================================================
// createUser
// =========================================================================

#[tokio::test]
async fn http_users_create_requires_auth() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db.clone()));

    let resp = TestClient::post("http://localhost/api/users/create")
        .json(&json!({"username": "alice"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    // Unauthorized responses must be JSON, not HTML.
    assert!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("application/json"),
        "unauthorized response must be JSON"
    );
}

#[tokio::test]
async fn http_users_create_rejects_non_admin_token() {
    // A normal user's personal token must not be able to create users.
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    // Mint a token for alice via the bootstrap path.
    let service = Service::new(build_router(config.clone(), db.clone()));
    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
        .send(&service)
        .await;
    let body: Value = resp.take_json().await.unwrap();
    let alice_token = body["token"].as_str().unwrap().to_string();

    // alice (non-admin) tries to create a user -> forbidden.
    let resp = TestClient::post("http://localhost/api/users/create")
        .bearer_auth(&alice_token)
        .json(&json!({"username": "bob"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn http_users_create_with_bootstrap_creates_user() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = TestClient::post("http://localhost/api/users/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "display_name": "Alice", "role": "user"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["user"]["username"], "alice");
    assert_eq!(body["user"]["display_name"], "Alice");
    assert_eq!(body["user"]["role"], "user");
    assert_eq!(body["user"]["disabled"], false);
}

#[tokio::test]
async fn http_users_create_issue_credential_returns_plaintext_once_and_stores_hash_only() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = TestClient::post("http://localhost/api/users/create")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "display_name": "Alice",
            "role": "user",
            "issue_credential": true,
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    let credential = body["account_credential"].as_str().unwrap();
    assert!(credential.starts_with("wc_acct_"));
    assert_eq!(credential.len(), "wc_acct_".len() + 64);
    assert_eq!(body["account_credential_prefix"], &credential[..16]);

    let hash = hash_token(credential);
    let stored = db.get_account_credential_by_hash(&hash).unwrap().unwrap();
    assert_eq!(stored.credential_prefix, &credential[..16]);
    {
        let conn = db.conn_for_tests();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM account_credentials WHERE credential_hash = ?1",
                rusqlite::params![credential],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "plaintext account credential must never be stored"
        );
    }

    let mut resp = TestClient::post("http://localhost/api/users/me")
        .bearer_auth(credential)
        .json(&json!({}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["auth"]["kind"], "account");
    assert_eq!(body["auth"]["username"], "alice");
    assert_eq!(body["user"]["username"], "alice");
}

#[tokio::test]
async fn http_users_create_rejects_duplicate_username() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let req = json!({"username": "alice"});
    let first = TestClient::post("http://localhost/api/users/create")
        .bearer_auth("secret")
        .json(&req)
        .send(&service)
        .await;
    assert_eq!(effective_status(&first), StatusCode::OK);
    let second = TestClient::post("http://localhost/api/users/create")
        .bearer_auth("secret")
        .json(&req)
        .send(&service)
        .await;
    assert_eq!(effective_status(&second), StatusCode::CONFLICT);
}

#[tokio::test]
async fn http_users_create_rejects_invalid_username() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    for bad in ["", "Alice", "ali/ce", "ali..ce", "ali ce", &"a".repeat(65)] {
        let resp = TestClient::post("http://localhost/api/users/create")
            .bearer_auth("secret")
            .json(&json!({"username": bad}))
            .send(&service)
            .await;
        assert_eq!(
            effective_status(&resp),
            StatusCode::BAD_REQUEST,
            "username {:?} should be rejected",
            bad
        );
    }
}

#[tokio::test]
async fn http_users_create_rejects_invalid_role() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let resp = TestClient::post("http://localhost/api/users/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "role": "superuser"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_users_list_requires_admin() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config.clone(), db.clone()));

    // Bootstrap can list.
    let mut resp = TestClient::post("http://localhost/api/users/list")
        .bearer_auth("secret")
        .json(&json!({}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert!(body["users"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u["username"] == "alice"));

    // Normal user cannot list.
    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
        .send(&service)
        .await;
    let alice_token = resp.take_json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = TestClient::post("http://localhost/api/users/list")
        .bearer_auth(&alice_token)
        .json(&json!({}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
}

// =========================================================================
// createApiToken
// =========================================================================

#[tokio::test]
async fn http_tokens_create_returns_plaintext_once_and_authenticates() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "name": "laptop",
            "scopes": ["runtime:read", "project:read", "project:write", "job:run"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    let token = body["token"].as_str().unwrap().to_string();
    // Token format check.
    assert!(
        token.starts_with("wc_pat_"),
        "token must use wc_pat_ prefix"
    );
    assert!(token.len() > "wc_pat_".len() + 32);
    // Prefix returned for display must not equal the full token.
    let prefix = body["token_prefix"].as_str().unwrap();
    assert!(prefix.starts_with("wc_pat_"));
    assert_ne!(prefix, token);
    assert_eq!(body["name"], "laptop");
    assert_eq!(body["username"], "alice");
    let scopes = body["scopes"].as_array().unwrap();
    assert_eq!(
        scopes
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["runtime:read", "project:read", "project:write", "job:run"]
    );

    // The DB must store the hash, not the plaintext token.
    let hash = hash_token(&token);
    let stored = db.get_api_key_by_hash(&hash).unwrap().unwrap();
    assert_eq!(stored.name, "laptop");
    // Confirm no row stores the plaintext token verbatim. Scope the
    // connection guard so the Mutex is released before the next HTTP
    // request (which acquires the same lock via AuthMiddleware).
    {
        let conn = db.conn_for_tests();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM api_keys WHERE key_hash = ?1",
                rusqlite::params![token],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "plaintext token must never be stored as key_hash");
    }

    // The personal token authenticates and resolves to alice.
    let mut resp = TestClient::post("http://localhost/api/tokens/list")
        .bearer_auth(&token)
        .json(&json!({"username": "alice"}))
        .send(&service)
        .await;
    assert_eq!(
        effective_status(&resp),
        StatusCode::OK,
        "personal token must authenticate"
    );
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["username"], "alice");
    assert_eq!(body["count"], 1);
}

#[tokio::test]
async fn http_tokens_create_rejects_wrong_token() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db));
    let resp = TestClient::post("http://localhost/api/tokens/list")
        .bearer_auth("wc_pat_deadbeef")
        .json(&json!({"username": "alice"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_tokens_create_non_admin_cannot_target_other_user() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    seed_user(&db, "bob", "user");
    let service = Service::new(build_router(config.clone(), db.clone()));
    // Mint a token for alice.
    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
        .send(&service)
        .await;
    let alice_token = resp.take_json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    // alice tries to create a token for bob -> forbidden.
    let resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth(&alice_token)
        .json(&json!({"username": "bob"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn http_tokens_create_non_admin_cannot_grant_admin_scope() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config.clone(), db));
    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
        .send(&service)
        .await;
    let alice_token = resp.take_json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth(&alice_token)
        .json(&json!({"username": "alice", "scopes": ["admin"]}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn http_tokens_register_hash_bootstrap_and_account_credential_self_management() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    seed_user(&db, "bob", "user");
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = TestClient::post("http://localhost/api/users/create")
        .bearer_auth("secret")
        .json(&json!({"username": "carol", "issue_credential": true}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let carol_credential = resp.take_json::<Value>().await.unwrap()["account_credential"]
        .as_str()
        .unwrap()
        .to_string();

    let local_token = crate::auth::generate_api_token();
    let local_hash = hash_token(&local_token);
    let local_prefix = crate::auth::token_prefix(&local_token);
    let mut resp = TestClient::post("http://localhost/api/tokens/register_hash")
        .bearer_auth(&carol_credential)
        .json(&json!({
            "username": "carol",
            "name": "gpt-action",
            "token_hash": format!("sha256:{}", local_hash),
            "token_prefix": local_prefix,
            "scopes": ["runtime:read", "project:read", "project:write", "job:run"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["token"]["name"], "gpt-action");
    assert_eq!(
        body["token"]["token_prefix"],
        crate::auth::token_prefix(&local_token)
    );
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(!serialized.contains(&local_token));
    assert!(!serialized.contains(&local_hash));

    let resp = TestClient::post("http://localhost/api/runtime/status")
        .bearer_auth(&local_token)
        .json(&json!({}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);

    let other_token = crate::auth::generate_api_token();
    let resp = TestClient::post("http://localhost/api/tokens/register_hash")
        .bearer_auth(&carol_credential)
        .json(&json!({
            "username": "bob",
            "token_hash": hash_token(&other_token),
            "token_prefix": crate::auth::token_prefix(&other_token),
            "scopes": ["runtime:read"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);

    let admin_token = crate::auth::generate_api_token();
    let resp = TestClient::post("http://localhost/api/tokens/register_hash")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "token_hash": hash_token(&admin_token),
            "token_prefix": crate::auth::token_prefix(&admin_token),
            "scopes": ["runtime:read"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
}

#[tokio::test]
async fn http_tokens_register_hash_rejects_existing_account_credential_hash() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db.clone()));
    let mut resp = TestClient::post("http://localhost/api/users/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "issue_credential": true}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let credential = resp.take_json::<Value>().await.unwrap()["account_credential"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = TestClient::post("http://localhost/api/tokens/register_hash")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "token_hash": hash_token(&credential),
            "token_prefix": "wc_pat_conflict",
            "scopes": ["runtime:read"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::CONFLICT);
}

#[tokio::test]
async fn http_tokens_register_hash_validates_hash_prefix_scope_and_duplicate() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db.clone()));

    let token = crate::auth::generate_api_token();
    let hash = hash_token(&token);
    let prefix = crate::auth::token_prefix(&token);
    for (field, mut body) in [
        (
            "bad hash",
            json!({"username":"alice","token_hash":"not-a-hash","token_prefix":prefix,"scopes":["runtime:read"]}),
        ),
        (
            "plaintext token field",
            json!({"username":"alice","token":"wc_pat_plaintext","token_hash":hash,"token_prefix":prefix,"scopes":["runtime:read"]}),
        ),
        (
            "bad prefix",
            json!({"username":"alice","token_hash":hash,"token_prefix":"wc_agent_bad","scopes":["runtime:read"]}),
        ),
        (
            "admin scope",
            json!({"username":"alice","token_hash":hash,"token_prefix":prefix,"scopes":["admin"]}),
        ),
    ] {
        let bearer = if field == "admin scope" {
            let user_token = crate::auth::generate_api_token();
            let record = ApiKeyRecord {
                id: uuid::Uuid::new_v4().to_string(),
                user_id: db.get_user_by_username("alice").unwrap().unwrap().id,
                name: "self".to_string(),
                key_prefix: crate::auth::token_prefix(&user_token),
                created_at: chrono::Utc::now().timestamp(),
                last_used_at: None,
                revoked_at: None,
                scopes: "runtime:read".to_string(),
                expires_at: None,
                kind: crate::models::TOKEN_KIND_USER.to_string(),
                allowed_client_id: None,
            };
            db.insert_api_key(&record, &hash_token(&user_token))
                .unwrap();
            user_token
        } else {
            "secret".to_string()
        };
        let resp = TestClient::post("http://localhost/api/tokens/register_hash")
            .bearer_auth(&bearer)
            .json(&body)
            .send(&service)
            .await;
        assert!(
            matches!(
                effective_status(&resp),
                StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN
            ),
            "{} should fail",
            field
        );
        body["name"] = json!("unused");
    }

    let resp = TestClient::post("http://localhost/api/tokens/register_hash")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "token_hash": hash,
            "token_prefix": prefix,
            "scopes": ["runtime:read"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let dup = TestClient::post("http://localhost/api/tokens/register_hash")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "token_hash": hash,
            "token_prefix": prefix,
            "scopes": ["runtime:read"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&dup), StatusCode::CONFLICT);
}

#[tokio::test]
async fn http_tokens_create_rejects_unknown_scope() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db));
    let resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "scopes": ["runtime:read", "bogus:scope"]}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
}

// =========================================================================
// listApiTokens
// =========================================================================

#[tokio::test]
async fn http_tokens_list_never_returns_hash_or_plaintext() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db));
    // Create two tokens, capturing their plaintext values so we can prove
    // the list response never echoes them. The short `token_prefix`
    // (which legitimately starts with `wc_pat_`) is allowed to appear.
    let mut plaintext_tokens = Vec::new();
    for name in ["a", "b"] {
        let mut resp = TestClient::post("http://localhost/api/tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "name": name, "scopes": ["runtime:read"]}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        plaintext_tokens.push(body["token"].as_str().unwrap().to_string());
    }
    let mut resp = TestClient::post("http://localhost/api/tokens/list")
        .bearer_auth("secret")
        .json(&json!({"username": "alice"}))
        .send(&service)
        .await;
    let body: Value = resp.take_json().await.unwrap();
    let tokens = body["tokens"].as_array().unwrap();
    assert_eq!(tokens.len(), 2);
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(
        !serialized.contains("token_hash"),
        "list response must never include token_hash"
    );
    assert!(
        !serialized.contains("key_hash"),
        "list response must never include key_hash"
    );
    // No full plaintext token may appear in the response. The
    // `token_prefix` (first 16 chars) is allowed.
    for plaintext in &plaintext_tokens {
        assert!(
            !serialized.contains(plaintext),
            "list response must never include the full plaintext token"
        );
    }
    // Each token entry exposes metadata only.
    for t in tokens {
        assert!(t["token_prefix"].is_string());
        assert!(t["scopes"].is_array());
        assert!(t["created_at"].is_number());
        assert!(t.get("key_hash").is_none());
        assert!(t.get("token").is_none());
    }
}

// =========================================================================
// revokeApiToken
// =========================================================================

#[tokio::test]
async fn http_tokens_revoke_works_and_token_no_longer_authenticates() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db));

    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
        .send(&service)
        .await;
    let body: Value = resp.take_json().await.unwrap();
    let token = body["token"].as_str().unwrap().to_string();
    let token_id = body["token_id"].as_str().unwrap().to_string();

    // Token works before revoke.
    let resp = TestClient::post("http://localhost/api/tokens/list")
        .bearer_auth(&token)
        .json(&json!({"username": "alice"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);

    // Revoke as bootstrap.
    let mut resp = TestClient::post("http://localhost/api/tokens/revoke")
        .bearer_auth("secret")
        .json(&json!({"token_id": token_id, "username": "alice"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert!(body["token"]["revoked_at"].is_number());
    // Revoke response must not leak the plaintext token. The
    // `token_prefix` (first 16 chars, starts with `wc_pat_`) is allowed;
    // only the full secret must never appear.
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(!serialized.contains(&token));

    // Idempotent revoke.
    let resp = TestClient::post("http://localhost/api/tokens/revoke")
        .bearer_auth("secret")
        .json(&json!({"token_id": token_id, "username": "alice"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);

    // Revoked token no longer authenticates.
    let resp = TestClient::post("http://localhost/api/tokens/list")
        .bearer_auth(&token)
        .json(&json!({"username": "alice"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_tokens_revoke_user_cannot_revoke_others_token() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    seed_user(&db, "bob", "user");
    let service = Service::new(build_router(config.clone(), db));
    // Create a token for bob.
    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "bob", "scopes": ["runtime:read"]}))
        .send(&service)
        .await;
    let bob_body: Value = resp.take_json().await.unwrap();
    let bob_token_id = bob_body["token_id"].as_str().unwrap().to_string();
    // Mint a token for alice.
    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
        .send(&service)
        .await;
    let alice_token = resp.take_json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    // alice targets her own username but passes bob's token_id. The
    // ownership check must reject this.
    let resp = TestClient::post("http://localhost/api/tokens/revoke")
        .bearer_auth(&alice_token)
        .json(&json!({"token_id": bob_token_id, "username": "alice"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
}

// =========================================================================
// Unauthorized responses are JSON
// =========================================================================

#[tokio::test]
async fn http_unauthorized_responses_are_json() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    for path in [
        "/api/users/create",
        "/api/users/list",
        "/api/tokens/create",
        "/api/tokens/list",
        "/api/tokens/revoke",
    ] {
        let resp = TestClient::post(&format!("http://localhost{}", path))
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(
            effective_status(&resp),
            StatusCode::UNAUTHORIZED,
            "{}",
            path
        );
        let ct = resp.headers().get("content-type").unwrap();
        assert!(
            ct.to_str().unwrap().contains("application/json"),
            "{} unauthorized response must be JSON, got {:?}",
            path,
            ct
        );
    }
}

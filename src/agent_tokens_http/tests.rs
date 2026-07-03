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

/// Build a router mirroring the production agent-token management wiring.
fn build_router(config: Arc<crate::Config>, db: Arc<crate::Database>) -> Router {
    Router::new()
        .hoop(affix_state::inject(config))
        .hoop(affix_state::inject(db))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("agent-tokens/create").post(agent_tokens_create))
                .push(
                    Router::with_path("agent-tokens/register_hash")
                        .post(agent_tokens_register_hash),
                )
                .push(Router::with_path("agent-tokens/list").post(agent_tokens_list))
                .push(Router::with_path("agent-tokens/revoke").post(agent_tokens_revoke)),
        )
}

fn effective_status(resp: &Response) -> StatusCode {
    resp.status_code.unwrap_or(StatusCode::OK)
}

/// Bootstrap helper: create a user directly via the DB so tests can mint
/// tokens for them.
fn seed_user(db: &crate::Database, username: &str, role: &str) -> crate::models::UserRecord {
    let now = chrono::Utc::now().timestamp();
    let user = crate::models::UserRecord {
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

fn seed_account_credential(db: &crate::Database, user: &crate::models::UserRecord) -> String {
    let plaintext = crate::auth::generate_account_credential();
    let now = chrono::Utc::now().timestamp();
    let record = crate::models::AccountCredentialRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        credential_prefix: crate::auth::token_prefix(&plaintext),
        created_at: now,
        last_used_at: None,
        revoked_at: None,
    };
    db.insert_account_credential(&record, &hash_token(&plaintext))
        .unwrap();
    plaintext
}

fn register_hash_body(token: &str, username: &str, client_id: &str) -> Value {
    json!({
        "username": username,
        "client_id": client_id,
        "name": client_id,
        "token_hash": format!("sha256:{}", hash_token(token)),
        "token_prefix": crate::auth::token_prefix(token),
        "scopes": [
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update"
        ],
    })
}

fn build_transport_router(
    config: Arc<crate::Config>,
    db: Arc<crate::Database>,
) -> (Router, Arc<crate::shell_client::ShellClientRegistry>) {
    let registry = Arc::new(crate::shell_client::ShellClientRegistry::default());
    let router = Router::new()
        .hoop(affix_state::inject(config))
        .hoop(affix_state::inject(db))
        .hoop(affix_state::inject(registry.clone()))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(
                    Router::with_path("agent-tokens/register_hash")
                        .post(agent_tokens_register_hash),
                )
                .push(
                    Router::with_path("shell/agent/register")
                        .post(crate::shell_client::shell_agent_register),
                )
                .push(Router::with_path("runtime/status").post(crate::runtime_http::runtime_status))
                .push(Router::with_path("tools/list").post(crate::runtime_http::tools_list))
                .push(Router::with_path("projects/list").post(crate::runtime_http::projects_list))
                .push(Router::with_path("tokens/list").post(crate::users_http::tokens_list)),
        )
        .push(
            Router::with_path("mcp")
                .hoop(crate::AuthMiddleware)
                .post(crate::mcp::mcp_post),
        );
    (router, registry)
}

/// Create a Phase 2 user token for `username` by calling the existing
/// tokens/create endpoint via the bootstrap path, returning the plaintext.
async fn mint_user_token(service: &salvo::Service, username: &str, scopes: Vec<String>) -> String {
    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": username, "scopes": scopes}))
        .send(service)
        .await;
    resp.take_json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string()
}

// =========================================================================
// createAgentToken
// =========================================================================

#[tokio::test]
async fn http_agent_tokens_create_requires_auth() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    let resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_agent_tokens_create_bootstrap_creates_for_anyone() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db.clone()));
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "client_id": "alice-laptop",
            "name": "alice laptop agent",
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    let token = body["token"].as_str().unwrap().to_string();
    assert!(
        token.starts_with("wc_agent_"),
        "agent token must use wc_agent_ prefix"
    );
    assert!(token.len() > "wc_agent_".len() + 32);
    let prefix = body["token_prefix"].as_str().unwrap();
    assert!(prefix.starts_with("wc_agent_"));
    assert_ne!(prefix, token);
    assert_eq!(body["kind"], "agent");
    assert_eq!(body["username"], "alice");
    assert_eq!(body["allowed_client_id"], "alice-laptop");
    // Default scopes are all agent transport scopes.
    let scopes = body["scopes"].as_array().unwrap();
    let scope_strs: Vec<&str> = scopes.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(
        scope_strs,
        vec![
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update"
        ]
    );

    // The DB must store the hash, not the plaintext token.
    let hash = hash_token(&token);
    let stored = db.get_api_key_by_hash(&hash).unwrap().unwrap();
    assert_eq!(stored.name, "alice laptop agent");
    assert!(stored.is_agent_token());
    assert_eq!(stored.allowed_client_id(), Some("alice-laptop"));
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
}

#[tokio::test]
async fn http_agent_tokens_create_user_creates_for_self() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    // Need the tokens/create endpoint to mint a user token for alice.
    let router = Router::new()
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                .push(Router::with_path("agent-tokens/create").post(agent_tokens_create)),
        );
    let service = Service::new(router);
    let alice_token = mint_user_token(&service, "alice", vec!["runtime:read".to_string()]).await;
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth(&alice_token)
        .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["kind"], "agent");
    assert_eq!(body["allowed_client_id"], "alice-laptop");
}

#[tokio::test]
async fn http_agent_tokens_create_user_cannot_create_for_other() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    seed_user(&db, "bob", "user");
    let router = Router::new()
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                .push(Router::with_path("agent-tokens/create").post(agent_tokens_create)),
        );
    let service = Service::new(router);
    let alice_token = mint_user_token(&service, "alice", vec!["runtime:read".to_string()]).await;
    let resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth(&alice_token)
        .json(&json!({"username": "bob", "client_id": "bob-laptop"}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn http_agent_tokens_create_requires_client_id() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db));
    let resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "client_id": ""}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_agent_tokens_create_validates_client_id() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db));
    for bad in ["bad/client", "bad client", "bad\x00client"] {
        let resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({"username": "alice", "client_id": bad}))
            .send(&service)
            .await;
        assert_eq!(
            effective_status(&resp),
            StatusCode::BAD_REQUEST,
            "client_id {:?} should be rejected",
            bad
        );
    }
}

#[tokio::test]
async fn http_agent_tokens_create_rejects_non_agent_scopes() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db));
    let resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "client_id": "alice-laptop",
            "scopes": ["runtime:read", "agent:register"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_agent_tokens_create_rejects_admin_scope_on_agent_token() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db));
    let resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "client_id": "alice-laptop",
            "scopes": ["admin"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
}

// =========================================================================
// registerAgentTokenHash
// =========================================================================

#[tokio::test]
async fn http_agent_tokens_register_hash_bootstrap_registers_for_any_user() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db.clone()));
    let token = crate::auth::generate_agent_token();
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth("secret")
        .json(&register_hash_body(&token, "alice", "alice-laptop"))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["token"]["name"], "alice-laptop");
    assert_eq!(body["token"]["allowed_client_id"], "alice-laptop");
    assert_eq!(
        body["token"]["token_prefix"],
        crate::auth::token_prefix(&token)
    );
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(!serialized.contains(&token));
    assert!(!serialized.contains(&hash_token(&token)));

    let stored = db
        .get_api_key_by_hash(&hash_token(&token))
        .unwrap()
        .unwrap();
    assert!(stored.is_agent_token());
    assert_eq!(stored.allowed_client_id(), Some("alice-laptop"));
    assert_eq!(
        stored.scopes_vec(),
        AGENT_SCOPES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn http_agent_tokens_register_hash_account_credential_self_only() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let alice = seed_user(&db, "alice", "user");
    seed_user(&db, "bob", "user");
    let alice_credential = seed_account_credential(&db, &alice);
    let service = Service::new(build_router(config, db));

    let alice_agent = crate::auth::generate_agent_token();
    let resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth(&alice_credential)
        .json(&register_hash_body(&alice_agent, "alice", "alice-laptop"))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);

    let bob_agent = crate::auth::generate_agent_token();
    let resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth(&alice_credential)
        .json(&register_hash_body(&bob_agent, "bob", "bob-laptop"))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn http_agent_tokens_register_hash_accepts_user_token_for_self() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let router = Router::new()
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                .push(
                    Router::with_path("agent-tokens/register_hash")
                        .post(agent_tokens_register_hash),
                ),
        );
    let service = Service::new(router);
    let alice_token = mint_user_token(&service, "alice", vec!["runtime:read".to_string()]).await;
    let agent_token = crate::auth::generate_agent_token();
    let resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth(&alice_token)
        .json(&register_hash_body(&agent_token, "alice", "alice-laptop"))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
}

#[tokio::test]
async fn http_agent_tokens_register_hash_validates_hash_prefix_scope_and_duplicate() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db.clone()));
    let token = crate::auth::generate_agent_token();
    let hash = hash_token(&token);
    let prefix = crate::auth::token_prefix(&token);
    for (field, body) in [
        (
            "bad hash",
            json!({"username":"alice","client_id":"alice-laptop","token_hash":"not-a-hash","token_prefix":prefix,"scopes":["agent:register"]}),
        ),
        (
            "plaintext token field",
            json!({"username":"alice","client_id":"alice-laptop","token":"wc_agent_plaintext","token_hash":hash,"token_prefix":prefix,"scopes":["agent:register"]}),
        ),
        (
            "bad prefix",
            json!({"username":"alice","client_id":"alice-laptop","token_hash":hash,"token_prefix":"wc_pat_bad","scopes":["agent:register"]}),
        ),
        (
            "bad client_id",
            json!({"username":"alice","client_id":"bad/client","token_hash":hash,"token_prefix":prefix,"scopes":["agent:register"]}),
        ),
        (
            "non-agent scope",
            json!({"username":"alice","client_id":"alice-laptop","token_hash":hash,"token_prefix":prefix,"scopes":["runtime:read"]}),
        ),
    ] {
        let resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
            .bearer_auth("secret")
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(
            effective_status(&resp),
            StatusCode::BAD_REQUEST,
            "{} should fail",
            field
        );
    }

    let resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth("secret")
        .json(&register_hash_body(&token, "alice", "alice-laptop"))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let dup = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth("secret")
        .json(&register_hash_body(&token, "alice", "alice-laptop"))
        .send(&service)
        .await;
    assert_eq!(effective_status(&dup), StatusCode::CONFLICT);
}

#[tokio::test]
async fn http_agent_tokens_register_hash_rejects_account_credential_hash_conflict() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let alice = seed_user(&db, "alice", "user");
    let credential = seed_account_credential(&db, &alice);
    let service = Service::new(build_router(config, db));
    let resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth("secret")
        .json(&json!({
            "username": "alice",
            "client_id": "alice-laptop",
            "token_hash": hash_token(&credential),
            "token_prefix": "wc_agent_conf",
            "scopes": ["agent:register"],
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::CONFLICT);
}

#[tokio::test]
async fn http_agent_tokens_register_hash_rejects_disabled_user() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let alice = seed_user(&db, "alice", "user");
    db.set_user_disabled(&alice.id, true, chrono::Utc::now().timestamp())
        .unwrap();
    let service = Service::new(build_router(config, db));
    let token = crate::auth::generate_agent_token();
    let resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth("secret")
        .json(&register_hash_body(&token, "alice", "alice-laptop"))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn http_agent_tokens_register_hash_enforces_transport_and_client_id_binding() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let (router, _registry) = build_transport_router(config, db);
    let service = Service::new(router);
    let token = crate::auth::generate_agent_token();
    let resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth("secret")
        .json(&register_hash_body(&token, "alice", "alice-laptop"))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);

    let mut resp = TestClient::post("http://localhost/api/shell/agent/register")
        .bearer_auth(&token)
        .json(&json!({
            "client_id": "alice-laptop",
            "agent_instance_id": "inst-1",
            "owner": "alice",
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["client"]["client_id"], "alice-laptop");
    assert_eq!(body["client"]["owner"], "alice");

    let resp = TestClient::post("http://localhost/api/shell/agent/register")
        .bearer_auth(&token)
        .json(&json!({
            "client_id": "other-laptop",
            "agent_instance_id": "inst-2",
            "owner": "alice",
        }))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);

    for path in [
        "/api/runtime/status",
        "/api/tools/list",
        "/api/projects/list",
        "/api/tokens/list",
        "/mcp",
    ] {
        let body = if path == "/api/tokens/list" {
            json!({"username": "alice"})
        } else if path == "/mcp" {
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}})
        } else {
            json!({})
        };
        let resp = TestClient::post(&format!("http://localhost{}", path))
            .bearer_auth(&token)
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(
            effective_status(&resp),
            StatusCode::FORBIDDEN,
            "agent token must not call {}",
            path
        );
    }
}

#[tokio::test]
async fn http_agent_tokens_register_hash_list_and_revoke_do_not_return_secrets() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db.clone()));
    let token = crate::auth::generate_agent_token();
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/register_hash")
        .bearer_auth("secret")
        .json(&register_hash_body(&token, "alice", "alice-laptop"))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let token_id = resp.take_json::<Value>().await.unwrap()["token"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut resp = TestClient::post("http://localhost/api/agent-tokens/list")
        .bearer_auth("secret")
        .json(&json!({"username": "alice"}))
        .send(&service)
        .await;
    let list_body: Value = resp.take_json().await.unwrap();
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/revoke")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "token_id": token_id}))
        .send(&service)
        .await;
    let revoke_body: Value = resp.take_json().await.unwrap();
    for body in [list_body, revoke_body] {
        let serialized = serde_json::to_string(&body).unwrap();
        assert!(!serialized.contains(&token));
        assert!(!serialized.contains(&hash_token(&token)));
        assert!(!serialized.contains("key_hash"));
        assert!(!serialized.contains("token_hash"));
    }
}

// =========================================================================
// listAgentTokens
// =========================================================================

#[tokio::test]
async fn http_agent_tokens_list_never_returns_hash_or_plaintext() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db));
    // Create two agent tokens, capturing their plaintext values.
    let mut plaintext_tokens = Vec::new();
    for name in ["a", "b"] {
        let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
            .bearer_auth("secret")
            .json(&json!({
                "username": "alice",
                "client_id": "alice-laptop",
                "name": name,
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        plaintext_tokens.push(body["token"].as_str().unwrap().to_string());
    }
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/list")
        .bearer_auth("secret")
        .json(&json!({"username": "alice"}))
        .send(&service)
        .await;
    let body: Value = resp.take_json().await.unwrap();
    let tokens = body["tokens"].as_array().unwrap();
    assert_eq!(tokens.len(), 2);
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(!serialized.contains("token_hash"));
    assert!(!serialized.contains("key_hash"));
    for plaintext in &plaintext_tokens {
        assert!(
            !serialized.contains(plaintext),
            "list response must never include the full plaintext token"
        );
    }
    for t in tokens {
        assert_eq!(t["kind"], "agent");
        assert!(t["allowed_client_id"].is_string());
        assert!(t.get("key_hash").is_none());
        assert!(t.get("token").is_none());
    }
}

#[tokio::test]
async fn http_agent_tokens_list_does_not_include_user_tokens() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let router = Router::new()
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                .push(Router::with_path("agent-tokens/create").post(agent_tokens_create))
                .push(Router::with_path("agent-tokens/list").post(agent_tokens_list)),
        );
    let service = Service::new(router);
    // Create a user token and an agent token for alice.
    let _user_token = mint_user_token(&service, "alice", vec!["runtime:read".to_string()]).await;
    let _ = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
        .send(&service)
        .await;
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/list")
        .bearer_auth("secret")
        .json(&json!({"username": "alice"}))
        .send(&service)
        .await;
    let body: Value = resp.take_json().await.unwrap();
    let tokens = body["tokens"].as_array().unwrap();
    assert_eq!(tokens.len(), 1, "agent token list must exclude user tokens");
    assert_eq!(tokens[0]["kind"], "agent");
}

// =========================================================================
// revokeAgentToken
// =========================================================================

#[tokio::test]
async fn http_agent_tokens_revoke_works() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let service = Service::new(build_router(config, db.clone()));
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
        .send(&service)
        .await;
    let body: Value = resp.take_json().await.unwrap();
    let token = body["token"].as_str().unwrap().to_string();
    let token_id = body["token_id"].as_str().unwrap().to_string();
    // Revoke as bootstrap.
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/revoke")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "token_id": token_id}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert!(body["token"]["revoked_at"].is_number());
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(!serialized.contains(&token));
    // Revoked agent token no longer authenticates.
    let hash = hash_token(&token);
    assert!(db.get_api_key_by_hash(&hash).unwrap().is_none());
}

#[tokio::test]
async fn http_agent_tokens_revoke_user_cannot_revoke_others() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    seed_user(&db, "bob", "user");
    let router = Router::new()
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                .push(Router::with_path("agent-tokens/create").post(agent_tokens_create))
                .push(Router::with_path("agent-tokens/revoke").post(agent_tokens_revoke)),
        );
    let service = Service::new(router);
    // Create an agent token for bob.
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "bob", "client_id": "bob-laptop"}))
        .send(&service)
        .await;
    let bob_token_id = resp.take_json::<Value>().await.unwrap()["token_id"]
        .as_str()
        .unwrap()
        .to_string();
    // alice mints a user token and tries to revoke bob's agent token.
    let alice_token = mint_user_token(&service, "alice", vec!["runtime:read".to_string()]).await;
    let resp = TestClient::post("http://localhost/api/agent-tokens/revoke")
        .bearer_auth(&alice_token)
        .json(&json!({"username": "alice", "token_id": bob_token_id}))
        .send(&service)
        .await;
    // alice targets her own username but passes bob's token_id: the
    // ownership check must reject this.
    assert_eq!(effective_status(&resp), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn http_agent_tokens_revoke_rejects_user_token_id() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let router = Router::new()
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("tokens/create").post(crate::users_http::tokens_create))
                .push(Router::with_path("agent-tokens/revoke").post(agent_tokens_revoke)),
        );
    let service = Service::new(router);
    // Create a user token for alice, capture its id via tokens/list.
    let mut resp = TestClient::post("http://localhost/api/tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "scopes": ["runtime:read"]}))
        .send(&service)
        .await;
    let user_token_id = resp.take_json::<Value>().await.unwrap()["token_id"]
        .as_str()
        .unwrap()
        .to_string();
    // Attempt to revoke it via the agent-tokens endpoint: must be rejected
    // because it is not an agent token.
    let resp = TestClient::post("http://localhost/api/agent-tokens/revoke")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "token_id": user_token_id}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
}

// =========================================================================
// Agent token cannot call management endpoints
// =========================================================================

/// Phase 3 no-secret guard: the agent_token_summary serialization must
/// never include `key_hash` or a `token` field. This is a unit-level
/// guard complementing the HTTP-level list/revoke tests.
#[test]
fn agent_token_summary_never_includes_hash_or_plaintext() {
    let key = ApiKeyRecord {
        id: "k-1".to_string(),
        user_id: "u-1".to_string(),
        name: "agent".to_string(),
        key_prefix: "wc_agent_pre".to_string(),
        created_at: 1,
        last_used_at: None,
        revoked_at: None,
        scopes: "agent:register".to_string(),
        expires_at: None,
        kind: "agent".to_string(),
        allowed_client_id: Some("laptop".to_string()),
    };
    let summary = agent_token_summary(&key);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert!(!serialized.contains("key_hash"));
    assert!(!serialized.contains("token_hash"));
    assert!(
        summary.get("token").is_none(),
        "summary must not include a plaintext token field"
    );
    assert!(
        summary.get("key_hash").is_none(),
        "summary must not include key_hash"
    );
    assert_eq!(summary["kind"], "agent");
    assert_eq!(summary["allowed_client_id"], "laptop");
}

#[tokio::test]
async fn http_agent_token_cannot_call_management_endpoints() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    seed_user(&db, "alice", "user");
    let router = Router::new()
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .push(
            Router::with_path("api")
                .hoop(crate::AuthMiddleware)
                .push(Router::with_path("agent-tokens/create").post(agent_tokens_create))
                .push(
                    Router::with_path("agent-tokens/register_hash")
                        .post(agent_tokens_register_hash),
                )
                .push(Router::with_path("agent-tokens/list").post(agent_tokens_list))
                .push(Router::with_path("agent-tokens/revoke").post(agent_tokens_revoke)),
        );
    let service = Service::new(router);
    // Bootstrap creates an agent token for alice.
    let mut resp = TestClient::post("http://localhost/api/agent-tokens/create")
        .bearer_auth("secret")
        .json(&json!({"username": "alice", "client_id": "alice-laptop"}))
        .send(&service)
        .await;
    let agent_token = resp.take_json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    // The agent token must not be able to call create/list/revoke.
    for path in [
        "/api/agent-tokens/create",
        "/api/agent-tokens/register_hash",
        "/api/agent-tokens/list",
        "/api/agent-tokens/revoke",
    ] {
        let body = match path {
            "/api/agent-tokens/create" => json!({
                "username": "alice",
                "client_id": "alice-laptop",
            }),
            "/api/agent-tokens/register_hash" => {
                let token = crate::auth::generate_agent_token();
                register_hash_body(&token, "alice", "alice-laptop")
            }
            "/api/agent-tokens/list" => json!({"username": "alice"}),
            "/api/agent-tokens/revoke" => json!({
                "username": "alice",
                "token_id": "00000000-0000-0000-0000-000000000000",
            }),
            _ => unreachable!(),
        };
        let resp = TestClient::post(&format!("http://localhost{}", path))
            .bearer_auth(&agent_token)
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(
            effective_status(&resp),
            StatusCode::FORBIDDEN,
            "agent token must not be able to call {}",
            path
        );
    }
}

// =========================================================================
// Unauthorized responses are JSON
// =========================================================================

#[tokio::test]
async fn http_agent_tokens_unauthorized_responses_are_json() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let service = Service::new(build_router(config, db));
    for path in [
        "/api/agent-tokens/create",
        "/api/agent-tokens/register_hash",
        "/api/agent-tokens/list",
        "/api/agent-tokens/revoke",
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
            "{}",
            path
        );
    }
}

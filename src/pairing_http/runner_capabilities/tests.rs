use super::*;
use crate::auth::{AuthContext, AuthKind};
use salvo::affix_state;
use salvo::test::{ResponseExt, TestClient};
use salvo::{Router, Service};

#[tokio::test]
async fn operator_grant_reads_canonical_runner_owner_and_rejects_wrong_or_offline_target() {
    use std::sync::Arc;
    for (owner, online, target, expected) in [
        ("alice", true, "target", StatusCode::OK),
        ("bob", true, "target", StatusCode::FORBIDDEN),
        ("alice", false, "target", StatusCode::CONFLICT),
        ("alice", true, "missing", StatusCode::CONFLICT),
    ] {
        let (_dir, db, mut key) = fixture();
        key.id = "live-key".into();
        key.expires_at = None;
        let hash = hash_token("live-fixture-token");
        db.insert_api_key(&key, &hash).unwrap();
        let registry = Arc::new(crate::runner_http::RunnerRegistry::default());
        for (client_id, runner_owner) in [("target", owner), ("peer", "alice")] {
            // These are registration wire keys, deliberately unchanged by #579.
            let registration = serde_json::from_value(json!({
                "client_id":client_id, "agent_instance_id":"inst",
                "agent_protocol_generation":2, "owner":runner_owner,
                "capabilities":{"shell":true}
            }))
            .unwrap();
            registry
                .register(crate::test_support::current_runner_registration(
                    registration,
                ))
                .await
                .unwrap();
        }
        if !online {
            registry.reconcile_disconnect("target", "inst").await;
        }
        let runtime = Arc::new(crate::tool_runtime::ToolRuntime::new(
            registry,
            Arc::new(crate::tool_runtime::RuntimeInfo::default()),
        ));
        let mut auth = AuthContext::new(AuthKind::Bootstrap);
        auth.is_bootstrap = true;
        auth.scopes = vec![crate::auth::SCOPE_ADMIN.into()];
        let service = Service::new(
            Router::new()
                .hoop(affix_state::inject(auth))
                .hoop(affix_state::inject(Arc::new(db)))
                .hoop(affix_state::inject(runtime))
                .push(Router::with_path("grant").post(grant_runner_capabilities)),
        );
        let mut response = TestClient::post("http://localhost/grant")
            .json(&json!({"client_id":target,"user_token_hash":hash}))
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap_or(StatusCode::OK), expected);
        if expected == StatusCode::OK {
            let body: Value = response.take_json().await.unwrap();
            assert_eq!(body["changed"], true);
        }
    }
}

#[tokio::test]
async fn capability_status_observes_scopes_without_runner_or_ssh_inventory_or_grant() {
    for (coding, ssh) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut auth = AuthContext::new(AuthKind::ApiToken);
        auth.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.into()];
        if coding {
            auth.scopes.push(SCOPE_CODING_AGENT_RUN.into());
        }
        if ssh {
            auth.scopes.push(SCOPE_SSH_LOCAL.into());
        }
        // No Runtime/Runner/DB is present. Observation cannot contact SSH or mutate grants.
        let service = Service::new(
            Router::new()
                .hoop(affix_state::inject(auth))
                .push(Router::with_path("status").post(runner_capability_authorization)),
        );
        let mut response = TestClient::post("http://localhost/status")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        let value: Value = response.take_json().await.unwrap();
        assert_eq!(value, json!({"coding_agents":coding,"ssh_resources":ssh}));
    }
}

#[tokio::test]
async fn capability_status_rejects_transport_subjects_missing_scope_and_arbitrary_inputs() {
    for (kind, scopes) in [
        (AuthKind::AgentToken, vec![crate::auth::SCOPE_RUNTIME_READ]),
        (AuthKind::ApiToken, vec![]),
        (
            AuthKind::AccountCredential,
            vec![crate::auth::SCOPE_RUNTIME_READ],
        ),
    ] {
        let mut auth = AuthContext::new(kind);
        auth.scopes = scopes.into_iter().map(str::to_owned).collect();
        let service = Service::new(
            Router::new()
                .hoop(affix_state::inject(auth))
                .push(Router::with_path("status").post(runner_capability_authorization)),
        );
        let response = TestClient::post("http://localhost/status")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::FORBIDDEN);
    }
    let mut auth = AuthContext::new(AuthKind::ApiToken);
    auth.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.into()];
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(auth))
            .push(Router::with_path("status").post(runner_capability_authorization)),
    );
    for input in [
        json!({"scopes":["admin"]}),
        json!({"client_id":"other"}),
        json!({"token":"private-status-sentinel"}),
    ] {
        let mut response = TestClient::post("http://localhost/status")
            .json(&input)
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::BAD_REQUEST);
        assert!(!response
            .take_string()
            .await
            .unwrap()
            .contains("private-status-sentinel"));
    }
}

fn fixture() -> (tempfile::TempDir, Database, ApiKeyRecord) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("db.sqlite")).unwrap();
    db.create_user(&UserRecord {
        id: "user".into(),
        username: "alice".into(),
        created_at: 1,
        disabled: 0,
        display_name: None,
        role: "user".into(),
        disabled_at: None,
        updated_at: None,
    })
    .unwrap();
    let key = ApiKeyRecord {
        id: "key".into(),
        user_id: "user".into(),
        name: "Desktop".into(),
        key_prefix: "fixture".into(),
        created_at: 1,
        last_used_at: None,
        revoked_at: None,
        scopes: "runtime:read runner:manage project:read project:write job:run mcp:local".into(),
        expires_at: Some(1000),
        kind: TOKEN_KIND_USER.into(),
        allowed_client_id: None,
    };
    db.insert_api_key(&key, &hash_token("fixture-user-token"))
        .unwrap();
    (dir, db, key)
}

#[test]
fn operator_grant_is_idempotent_and_preserves_token_identity_expiry_and_existing_scopes() {
    let (_dir, db, before) = fixture();
    let hash = hash_token("fixture-user-token");
    assert_eq!(grant_for_owner(&db, &hash, "alice", 10).unwrap(), true);
    let after = db.get_api_key_by_hash(&hash).unwrap().unwrap();
    assert!(after.scopes.ends_with("ssh:local coding_agent:run"));
    assert!(after.scopes.starts_with(&before.scopes));
    let mut expected = serde_json::to_value(before).unwrap();
    let mut actual = serde_json::to_value(after).unwrap();
    expected.as_object_mut().unwrap().remove("scopes");
    actual.as_object_mut().unwrap().remove("scopes");
    assert_eq!(actual, expected);
    assert_eq!(grant_for_owner(&db, &hash, "alice", 10).unwrap(), false);
}

#[test]
fn operator_grant_rejects_another_runner_owner_and_nonlive_or_transport_keys() {
    let (_dir, db, original) = fixture();
    let hash = hash_token("fixture-user-token");
    assert_eq!(
        grant_for_owner(&db, &hash, "bob", 10),
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        grant_for_owner(&db, &hash, "alice", 1000),
        Err(StatusCode::FORBIDDEN)
    );
    for (index, mut key) in [
        {
            let mut key = original.clone();
            key.kind = TOKEN_KIND_AGENT.into();
            key
        },
        {
            let mut key = original.clone();
            key.allowed_client_id = Some("mini".into());
            key
        },
        {
            let mut key = original.clone();
            key.revoked_at = Some(5);
            key
        },
    ]
    .into_iter()
    .enumerate()
    {
        key.id = format!("other-{index}");
        let hash = hash_token(&format!("fixture-{index}"));
        db.insert_api_key(&key, &hash).unwrap();
        assert_eq!(
            grant_for_owner(&db, &hash, "alice", 10),
            Err(StatusCode::FORBIDDEN)
        );
    }
    assert_eq!(
        db.get_api_key_by_hash(&hash).unwrap().unwrap().scopes,
        original.scopes
    );
}

#[test]
fn operator_grant_scope_write_is_fenced_against_stale_or_revoked_keys() {
    let (_dir, db, key) = fixture();
    assert!(!db
        .compare_and_swap_user_key_scopes(&key.id, &key.user_id, "stale", "ssh:local", 10)
        .unwrap());
    db.revoke_api_key(&key.id, 10).unwrap();
    assert!(!db
        .compare_and_swap_user_key_scopes(&key.id, &key.user_id, &key.scopes, "ssh:local", 10)
        .unwrap());
    assert_eq!(
        grant_for_owner(&db, &hash_token("fixture-user-token"), "alice", 10),
        Err(StatusCode::FORBIDDEN)
    );
}

#[tokio::test]
async fn operator_grant_http_rejects_nonadmin_before_reading_or_mutating_credentials() {
    for kind in [
        AuthKind::ApiToken,
        AuthKind::AgentToken,
        AuthKind::SharedKey,
        AuthKind::OAuth2Token,
    ] {
        let mut auth = AuthContext::new(kind);
        auth.username = Some("alice".into());
        auth.scopes = vec![SCOPE_RUNTIME_READ.into(), SCOPE_RUNNER_MANAGE.into()];
        let service = Service::new(
            Router::new()
                .hoop(affix_state::inject(auth))
                .push(Router::with_path("grant").post(grant_runner_capabilities)),
        );
        let mut response = TestClient::post("http://localhost/grant")
            .json(&json!({"client_id":"mini", "user_token_hash":hash_token("fixture-user-token")}))
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::FORBIDDEN);
        let text = response.take_string().await.unwrap();
        assert!(!text.contains("fixture-user-token"));
    }
}

#[tokio::test]
async fn operator_grant_http_rejects_arbitrary_scopes_and_malformed_hash_without_echo() {
    let mut auth = AuthContext::new(AuthKind::Bootstrap);
    auth.is_bootstrap = true;
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(auth))
            .push(Router::with_path("grant").post(grant_runner_capabilities)),
    );
    for body in [
        json!({"client_id":"mini", "user_token_hash":hash_token("fixture-user-token"), "scopes":["admin"]}),
        json!({"client_id":"mini", "user_token_hash":"not-a-hash-secret-sentinel"}),
    ] {
        let mut response = TestClient::post("http://localhost/grant")
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::BAD_REQUEST);
        assert!(!response
            .take_string()
            .await
            .unwrap()
            .contains("secret-sentinel"));
    }
}

#[tokio::test]
async fn pairing_capabilities_follow_admin_issuance_not_enrollment_body() {
    for granted in [false, true] {
        let (_dir, db, _) = fixture();
        let now = chrono::Utc::now().timestamp();
        let code = "fixture-pairing-code";
        db.insert_pairing_code(&PairingCodeRecord {
            id: "pair".into(),
            code_hash: hash_token(code),
            user_id: "user".into(),
            username: "alice".into(),
            client_id: "mini".into(),
            created_at: now,
            expires_at: now + 600,
            used_at: None,
            user_token_name: None,
            agent_token_name: None,
            runner_capabilities: granted,
        })
        .unwrap();
        let db = std::sync::Arc::new(db);
        let service = Service::new(
            Router::new()
                .hoop(affix_state::inject(db.clone()))
                .push(Router::with_path("enroll").post(pairing_enroll)),
        );
        let mut response = TestClient::post("http://localhost/enroll")
            .json(&json!({"pairing_code":code,"client_id":"mini","runner_capabilities":!granted}))
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        let body: Value = response.take_json().await.unwrap();
        let scopes = body["user_token_scopes"].as_array().unwrap();
        for scope in [SCOPE_SSH_LOCAL, SCOPE_CODING_AGENT_RUN] {
            assert_eq!(scopes.contains(&json!(scope)), granted);
        }
        assert!(!body["agent_token_scopes"]
            .as_array()
            .unwrap()
            .contains(&json!(SCOPE_SSH_LOCAL)));
        let stored = db
            .get_api_key_by_hash(&hash_token(body["user_token"].as_str().unwrap()))
            .unwrap()
            .unwrap();
        assert_eq!(
            stored
                .scopes
                .split_ascii_whitespace()
                .any(|scope| scope == SCOPE_SSH_LOCAL),
            granted
        );
    }
}

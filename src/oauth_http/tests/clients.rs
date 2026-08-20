use super::*;

fn authorized_post_json(url: &str, body: String, token: &str) -> salvo::test::RequestBuilder {
    TestClient::post(url)
        .add_header("authorization", format!("Bearer {}", token), true)
        .add_header("content-type", "application/json", true)
        .body(body)
}
fn create_client_json(name: &str, redirect_uris: &[&str], scopes: Option<&[&str]>) -> String {
    let mut obj = serde_json::json!({
        "name": name,
        "redirect_uris": redirect_uris,
    });
    if let Some(s) = scopes {
        obj["allowed_scopes"] = serde_json::json!(s);
    }
    obj.to_string()
}
// -----------------------------------------------------------------------
// OAuth client management API tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn oauth_client_create_returns_client_secret_once() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("ChatGPT Action", &["https://example.com/callback"], None),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    let secret = body["client_secret"]
        .as_str()
        .expect("client_secret returned");
    assert!(secret.starts_with("wc_csec_"));
    assert!(body["client"]["client_id"]
        .as_str()
        .unwrap()
        .starts_with("wc_client_"));

    // list must NOT return the secret.
    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/list",
        "{}".to_string(),
        &token,
    )
    .send(&service)
    .await;
    let list_body: serde_json::Value = resp.take_json().await.unwrap();
    assert!(!list_body["clients"].as_array().unwrap().is_empty());
    let clients = list_body["clients"].as_array().unwrap();
    assert!(clients.iter().all(|c| c.get("client_secret").is_none()));
    assert!(clients
        .iter()
        .all(|c| c.get("client_secret_hash").is_none()));
}

#[tokio::test]
async fn managed_oauth_client_api_hides_and_cannot_mutate_project_share_clients() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let (project_client, _secret) = seed_project_share_client(
        &db,
        TEST_PROJECT_GRANT_ID,
        "https://client.example/callback",
    );
    let service = Service::new(build_router(config, db.clone()));

    let mut list = authorized_post_json(
        "http://localhost/api/oauth/clients/list",
        "{}".to_string(),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(list.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = list.take_json().await.unwrap();
    assert!(body["clients"]
        .as_array()
        .unwrap()
        .iter()
        .all(|client| client["client_id"] != project_client.client_id));

    let update = authorized_post_json(
        "http://localhost/api/oauth/clients/update_scopes",
        serde_json::json!({
            "client_id": project_client.client_id,
            "allowed_scopes": ["runtime:read"]
        })
        .to_string(),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(update.status_code, Some(StatusCode::NOT_FOUND));

    let revoke = authorized_post_json(
        "http://localhost/api/oauth/clients/revoke",
        serde_json::json!({"client_id": project_client.client_id}).to_string(),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(revoke.status_code, Some(StatusCode::NOT_FOUND));
    assert!(db
        .get_oauth_client_by_client_id(&project_client.client_id)
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn oauth_client_create_hashes_secret_only() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("Hashed", &["https://example.com/callback"], None),
        &token,
    )
    .send(&service)
    .await;
    let body: serde_json::Value = resp.take_json().await.unwrap();
    let secret = body["client_secret"].as_str().unwrap();
    let client_id = body["client"]["client_id"].as_str().unwrap();

    let stored_hash: String = {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT client_secret_hash FROM oauth_clients WHERE client_id = ?1",
            rusqlite::params![client_id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_ne!(stored_hash, secret);
    assert_eq!(stored_hash, hash_token(secret));
}

#[tokio::test]
async fn oauth_client_create_validates_redirect_uris() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));

    // not a URL
    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("Bad", &["not-a-url"], None),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

    // http non-loopback rejected
    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("Bad", &["http://example.com/cb"], None),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

    // http loopback accepted
    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json(
            "Local",
            &["http://127.0.0.1:3000/cb", "http://localhost:3000/cb"],
            None,
        ),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // empty array rejected
    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("Empty", &[], None),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
}

#[tokio::test]
async fn oauth_client_create_trims_redirect_uris_before_storing() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));

    // redirect_uris with leading/trailing whitespace, plus a trim-duplicate.
    let body = serde_json::json!({
        "name": "Trimmed",
        "redirect_uris": [
            "  https://example.com/callback  ",
            "https://example.com/callback",
            "\thttp://127.0.0.1:3000/cb\t",
        ],
    })
    .to_string();
    let mut resp = authorized_post_json("http://localhost/api/oauth/clients/create", body, &token)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    let client_id = body["client"]["client_id"].as_str().unwrap();
    let returned: Vec<String> = body["client"]["redirect_uris"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    // The trim-duplicate must collapse; whitespace must be stripped.
    assert_eq!(
        returned,
        vec![
            "https://example.com/callback".to_string(),
            "http://127.0.0.1:3000/cb".to_string(),
        ]
    );

    // Verify the stored record has trimmed, deduped values.
    let stored: String = {
        let conn = db.conn_for_tests();
        conn.query_row(
            "SELECT redirect_uris FROM oauth_clients WHERE client_id = ?1",
            rusqlite::params![client_id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        stored,
        "https://example.com/callback\nhttp://127.0.0.1:3000/cb"
    );
}

#[tokio::test]
async fn oauth_client_create_omitted_and_empty_scopes_use_exact_legacy_default() {
    const LEGACY_DEFAULT_SCOPES: &[&str] = &[
        "runtime:read",
        "project:read",
        "project:write",
        "job:run",
        "computer:read",
        "computer:control",
        "account:manage",
    ];
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));
    let cases: [(&str, Option<&[&str]>); 2] = [
        ("Omitted Default Scopes", None),
        ("Empty Default Scopes", Some(&[])),
    ];
    let non_legacy_supported = oauth_scopes_supported()
        .iter()
        .copied()
        .filter(|scope| !LEGACY_DEFAULT_SCOPES.contains(scope))
        .collect::<Vec<_>>();
    assert!(non_legacy_supported.contains(&"computer:launch"));
    assert!(non_legacy_supported.contains(&"computer:display_read"));
    assert!(non_legacy_supported.contains(&"computer:pointer_control"));
    assert!(non_legacy_supported.contains(&"computer:clipboard_read"));
    assert!(non_legacy_supported.contains(&"computer:clipboard_write"));

    for (name, requested_scopes) in cases {
        let mut resp = authorized_post_json(
            "http://localhost/api/oauth/clients/create",
            create_client_json(name, &["https://example.com/callback"], requested_scopes),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let body: serde_json::Value = resp.take_json().await.unwrap();
        let scopes = body["client"]["allowed_scopes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|scope| scope.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(scopes.as_slice(), LEGACY_DEFAULT_SCOPES);
        assert!(!scopes.contains(&"computer:launch"));
        // Any current or future supported permission outside the literal legacy
        // set must remain opt-in rather than silently joining this default.
        for scope in &non_legacy_supported {
            assert!(
                !scopes.contains(scope),
                "non-legacy scope {scope} entered default"
            );
        }
    }
}

#[tokio::test]
async fn oauth_client_create_accepts_explicit_computer_launch_scope() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json(
            "Launch Opt In",
            &["https://example.com/callback"],
            Some(&["computer:read", "computer:launch"]),
        ),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(
        body["client"]["allowed_scopes"],
        serde_json::json!(["computer:read", "computer:launch"])
    );
}

#[tokio::test]
async fn oauth_client_create_accepts_explicit_display_read_scope() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json(
            "Display Opt In",
            &["https://example.com/callback"],
            Some(&["computer:read", "computer:display_read"]),
        ),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(
        body["client"]["allowed_scopes"],
        serde_json::json!(["computer:read", "computer:display_read"])
    );
}

#[tokio::test]
async fn oauth_client_create_accepts_explicit_pointer_control_scope() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json(
            "Pointer Opt In",
            &["https://example.com/callback"],
            Some(&[
                "computer:read",
                "computer:display_read",
                "computer:control",
                "computer:pointer_control",
            ]),
        ),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(
        body["client"]["allowed_scopes"],
        serde_json::json!([
            "computer:read",
            "computer:control",
            "computer:display_read",
            "computer:pointer_control"
        ])
    );
}

#[tokio::test]
async fn oauth_client_create_accepts_explicit_clipboard_scopes() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json(
            "Clipboard Opt In",
            &["https://example.com/callback"],
            Some(&[
                "computer:read",
                "computer:control",
                "computer:clipboard_read",
                "computer:clipboard_write",
            ]),
        ),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(
        body["client"]["allowed_scopes"],
        serde_json::json!([
            "computer:read",
            "computer:control",
            "computer:clipboard_read",
            "computer:clipboard_write"
        ])
    );
}

#[tokio::test]
async fn oauth_client_update_scopes_adds_control_and_revokes_prior_grants() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read computer:read",
    );
    let (_access, access_plaintext) =
        seed_access_token(&db, &client, &user, "runtime:read computer:read");
    let (_refresh, refresh_plaintext) =
        seed_refresh_token(&db, &client, &user, "runtime:read computer:read");
    let (_code, code_plaintext) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read computer:read",
        None,
        None,
    );
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/update_scopes",
        serde_json::json!({
            "client_id": client.client_id,
            "allowed_scopes": ["computer:control", "computer:read", "runtime:read"]
        })
        .to_string(),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["changed"], true);
    assert_eq!(body["reauthorization_required"], true);
    assert_eq!(body["revoked_grants"]["access_tokens"], 1);
    assert_eq!(body["revoked_grants"]["refresh_tokens"], 1);
    assert_eq!(body["revoked_grants"]["authorization_codes"], 1);
    assert_eq!(
        body["client"]["allowed_scopes"],
        serde_json::json!(["runtime:read", "computer:read", "computer:control"])
    );
    assert!(body.get("client_secret").is_none());
    assert!(body["client"].get("client_secret_hash").is_none());

    let stored = db
        .get_oauth_client_by_client_id(&client.client_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.allowed_scopes,
        "runtime:read computer:read computer:control"
    );
    assert!(db
        .get_oauth_access_token_by_hash(&hash_token(&access_plaintext))
        .unwrap()
        .is_none());
    assert!(db
        .get_oauth_refresh_token_by_hash(&hash_token(&refresh_plaintext))
        .unwrap()
        .is_none());
    assert!(db
        .get_oauth_authorization_code_by_hash(&hash_token(&code_plaintext))
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn oauth_client_update_scopes_noop_preserves_existing_grants() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read computer:read",
    );
    let (_access, access_plaintext) =
        seed_access_token(&db, &client, &user, "runtime:read computer:read");
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/update_scopes",
        serde_json::json!({
            "client_id": client.client_id,
            "allowed_scopes": ["computer:read", "runtime:read"]
        })
        .to_string(),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["changed"], false);
    assert_eq!(body["reauthorization_required"], false);
    assert_eq!(body["revoked_grants"]["access_tokens"], 0);
    assert!(db
        .get_oauth_access_token_by_hash(&hash_token(&access_plaintext))
        .unwrap()
        .is_some());
}

#[test]
fn oauth_client_scope_update_stale_expected_scope_fails_without_revoking_grants() {
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read computer:read computer:control",
    );
    let (_access, access_plaintext) = seed_access_token(
        &db,
        &client,
        &user,
        "runtime:read computer:read computer:control",
    );

    let result = db
        .update_oauth_client_allowed_scopes_and_revoke_grants(
            &client.client_id,
            "runtime:read computer:read",
            "runtime:read computer:read",
            chrono::Utc::now().timestamp(),
        )
        .unwrap();
    assert!(result.is_none());

    let stored = db
        .get_oauth_client_by_client_id(&client.client_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.allowed_scopes,
        "runtime:read computer:read computer:control"
    );
    assert!(db
        .get_oauth_access_token_by_hash(&hash_token(&access_plaintext))
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn oauth_client_update_scopes_rejects_implicit_or_unknown_scope_expansion() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read computer:read",
    );
    let (_access, access_plaintext) =
        seed_access_token(&db, &client, &user, "runtime:read computer:read");
    let service = Service::new(build_router(config, db.clone()));

    for allowed_scopes in [
        serde_json::json!([]),
        serde_json::json!(["runtime:read", "bogus:scope"]),
    ] {
        let resp = authorized_post_json(
            "http://localhost/api/oauth/clients/update_scopes",
            serde_json::json!({
                "client_id": client.client_id,
                "allowed_scopes": allowed_scopes
            })
            .to_string(),
            &token,
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
    }

    let stored = db
        .get_oauth_client_by_client_id(&client.client_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.allowed_scopes, "runtime:read computer:read");
    assert!(db
        .get_oauth_access_token_by_hash(&hash_token(&access_plaintext))
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn oauth_client_update_scopes_is_first_party_only() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read computer:read computer:control account:manage",
    );
    let (_access, access_plaintext) = seed_access_token(
        &db,
        &client,
        &user,
        "runtime:read computer:read computer:control account:manage",
    );
    let service = Service::new(build_router(config, db.clone()));

    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/update_scopes",
        serde_json::json!({
            "client_id": client.client_id,
            "allowed_scopes": ["runtime:read"]
        })
        .to_string(),
        &access_plaintext,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));

    let stored = db
        .get_oauth_client_by_client_id(&client.client_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.allowed_scopes,
        "runtime:read computer:read computer:control account:manage"
    );
}

#[tokio::test]
async fn oauth_client_create_rejects_unknown_scopes() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));

    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json(
            "Bad Scopes",
            &["https://example.com/callback"],
            Some(&["runtime:read", "bogus:scope"]),
        ),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));

    // agent:poll and admin are rejected for OAuth delegation
    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json(
            "Agent Scope",
            &["https://example.com/callback"],
            Some(&["agent:poll"]),
        ),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
}

#[tokio::test]
async fn oauth_client_list_does_not_return_secret_hash() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));
    seed_client(&db, &user, "Seeded");

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/list",
        "{}".to_string(),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    let clients = body["clients"].as_array().unwrap();
    assert!(!clients.is_empty());
    for c in clients {
        assert!(c.get("client_secret_hash").is_none(), "secret hash leaked");
        assert!(c.get("client_secret").is_none(), "secret leaked");
    }
}

#[tokio::test]
async fn oauth_client_revoke_revokes_client() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("To Revoke", &["https://example.com/callback"], None),
        &token,
    )
    .send(&service)
    .await;
    let body: serde_json::Value = resp.take_json().await.unwrap();
    let client_id = body["client"]["client_id"].as_str().unwrap().to_string();

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/revoke",
        serde_json::json!({ "client_id": client_id }).to_string(),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);

    // Idempotent: revoke again still success.
    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/revoke",
        serde_json::json!({ "client_id": client_id }).to_string(),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // The revoked client is no longer returned by the active lookup.
    assert!(db
        .get_oauth_client_by_client_id(&client_id)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn oauth_client_revoke_revokes_related_tokens() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let token = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));

    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("Tokens", &["https://example.com/callback"], None),
        &token,
    )
    .send(&service)
    .await;
    let body: serde_json::Value = resp.take_json().await.unwrap();
    let client_id = body["client"]["client_id"].as_str().unwrap().to_string();
    // Build a temporary client record handle for seeding helpers.
    let client = db
        .list_oauth_clients()
        .unwrap()
        .into_iter()
        .find(|c| c.client_id == client_id)
        .unwrap();

    let (_at_rec, _at_plain) = seed_access_token(&db, &client, &user, "runtime:read");
    let (_rt_rec, _rt_plain) = seed_refresh_token(&db, &client, &user, "runtime:read");
    let (_ac_rec, _ac_plain) = seed_auth_code(
        &db,
        &client,
        &user,
        "https://example.com/callback",
        "runtime:read",
        Some("challenge-1"),
        Some("S256"),
    );

    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/revoke",
        serde_json::json!({ "client_id": client_id }).to_string(),
        &token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    let conn = db.conn_for_tests();
    let at_revoked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_access_tokens WHERE client_id = ?1 AND revoked_at IS NOT NULL",
                rusqlite::params![client_id],
                |row| row.get(0),
            )
            .unwrap();
    let rt_revoked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_refresh_tokens WHERE client_id = ?1 AND revoked_at IS NOT NULL",
                rusqlite::params![client_id],
                |row| row.get(0),
            )
            .unwrap();
    let ac_revoked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_authorization_codes WHERE client_id = ?1 AND revoked_at IS NOT NULL",
                rusqlite::params![client_id],
                |row| row.get(0),
            )
            .unwrap();
    assert!(at_revoked >= 1, "access token should be revoked");
    assert!(rt_revoked >= 1, "refresh token should be revoked");
    assert!(ac_revoked >= 1, "authorization code should be revoked");
}

#[tokio::test]
async fn oauth_client_management_rejects_oauth2_token() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let service = Service::new(build_router(config, db.clone()));
    let client = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read",
    );
    let (_at, oauth_access_token) = seed_access_token(&db, &client, &user, "runtime:read");

    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("Should Fail", &["https://example.com/callback"], None),
        &oauth_access_token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));

    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/list",
        "{}".to_string(),
        &oauth_access_token,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
}

#[tokio::test]
async fn oauth_client_management_allows_api_token_or_bootstrap() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let pat = seed_user_token(&db, &user);
    let service = Service::new(build_router(config, db.clone()));

    // PAT
    let resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("Via PAT", &["https://example.com/callback"], None),
        &pat,
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));

    // Bootstrap token
    let mut resp = authorized_post_json(
        "http://localhost/api/oauth/clients/create",
        create_client_json("Via Bootstrap", &["https://example.com/callback"], None),
        "bootstrap-token",
    )
    .send(&service)
    .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
}

use super::*;

fn authorize_url(client: &OAuthClientRecord, scopes: &str, challenge: &str) -> String {
    format!(
        "http://localhost/oauth/authorize?{}",
        form_body(&[
            ("response_type", "code"),
            ("client_id", &client.client_id),
            ("redirect_uri", "https://example.com/callback"),
            ("scope", scopes),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
        ])
    )
}

fn issued_code(resp: &Response) -> String {
    let location = location_header(resp).expect("OAuth redirect");
    url::Url::parse(&location)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "code")
        .expect("authorization code")
        .1
        .into_owned()
}

#[tokio::test]
async fn managed_oauth_authority_delegation_matrix() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let other = seed_user(&db, "bob");
    let own = seed_client_with_redirects_and_scopes(
        &db,
        &user,
        "https://example.com/callback",
        "runtime:read project:read project:write",
    );
    let foreign = seed_client_with_redirects_and_scopes(
        &db,
        &other,
        "https://example.com/callback",
        &own.allowed_scopes,
    );
    let service = Service::new(build_router(config, db.clone()));
    for (pat_scopes, requested, client, allowed, admin) in [
        ("runtime:read", "project:write", &own, false, false),
        (
            "runtime:read project:read",
            "runtime:read project:read offline_access",
            &own,
            true,
            false,
        ),
        (
            "admin",
            "runtime:read project:read project:write offline_access",
            &own,
            true,
            true,
        ),
        ("admin", "project:write", &foreign, true, false),
        ("admin", "admin", &own, false, false),
    ] {
        let pat = seed_user_token_with_scopes(&db, &user, pat_scopes);
        let before = auth_code_count(&db);
        let resp = TestClient::get(&authorize_url(client, requested, "challenge"))
            .bearer_auth(&pat)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FOUND));
        if allowed {
            let record = auth_code_by_plaintext(&db, &issued_code(&resp));
            assert_eq!(record.admin_authority, admin);
            assert_eq!(record.scopes, requested);
            assert!(!record.scopes.split_whitespace().any(|s| s == "admin"));
        } else {
            assert!(location_header(&resp)
                .unwrap()
                .contains("error=invalid_scope"));
            assert_eq!(auth_code_count(&db), before);
        }
    }
}

#[tokio::test]
async fn managed_oauth_authority_survives_pat_revocation_and_two_rotations() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    for admin in [false, true] {
        let config = test_config(oauth2_enabled());
        let (_tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let (client, secret) = seed_client(&db, &user, "owned");
        let pat =
            seed_user_token_with_scopes(&db, &user, if admin { "admin" } else { "runtime:read" });
        let service = Service::new(build_router(config.clone(), db.clone()));
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let resp = TestClient::get(&authorize_url(
            &client,
            "runtime:read",
            &pkce_s256_challenge(verifier),
        ))
        .bearer_auth(&pat)
        .send(&service)
        .await;
        let code = issued_code(&resp);
        assert_eq!(auth_code_by_plaintext(&db, &code).admin_authority, admin);
        let key = db.get_api_key_by_hash(&hash_token(&pat)).unwrap().unwrap();
        db.revoke_api_key(&key.id, chrono::Utc::now().timestamp())
            .unwrap();
        let mut resp = post_form(
            "http://localhost/oauth/token",
            form_body(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", "https://example.com/callback"),
                ("code_verifier", verifier),
                ("client_id", &client.client_id),
                ("client_secret", &secret),
            ]),
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let mut body: serde_json::Value = resp.take_json().await.unwrap();
        let original_access = body["access_token"].as_str().unwrap().to_string();
        for rotation in 0..=2 {
            assert_eq!(body["scope"], "runtime:read");
            let access = body["access_token"].as_str().unwrap();
            let refresh = body["refresh_token"].as_str().unwrap();
            let at = db
                .get_oauth_access_token_by_hash(&hash_token(access))
                .unwrap()
                .unwrap();
            let rt = db
                .get_oauth_refresh_token_by_hash(&hash_token(refresh))
                .unwrap()
                .unwrap();
            assert_eq!(at.admin_authority, admin);
            assert_eq!(rt.admin_authority, admin);
            assert_eq!(at.scopes, "runtime:read");
            assert_eq!(rt.scopes, "runtime:read");
            let ctx = OAuth2Verifier
                .verify(&config, Some(&db), access)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(ctx.has_scope(crate::auth::SCOPE_ADMIN), admin);
            if rotation < 2 {
                let mut resp = post_form(
                    "http://localhost/oauth/token",
                    form_body(&[
                        ("grant_type", "refresh_token"),
                        ("refresh_token", refresh),
                        ("client_id", &client.client_id),
                        ("client_secret", &secret),
                    ]),
                )
                .send(&service)
                .await;
                assert_eq!(resp.status_code, Some(StatusCode::OK));
                assert!(db
                    .get_oauth_refresh_token_by_hash(&rt.token_hash)
                    .unwrap()
                    .is_none());
                body = resp.take_json().await.unwrap();
            }
        }
        let access = body["access_token"].as_str().unwrap();
        let refresh = body["refresh_token"].as_str().unwrap();
        let resp = post_form(
            "http://localhost/oauth/revoke",
            form_body(&[
                ("token", &original_access),
                ("client_id", &client.client_id),
                ("client_secret", &secret),
            ]),
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        assert!(OAuth2Verifier
            .verify(&config, Some(&db), &original_access)
            .await
            .is_err());
        assert!(OAuth2Verifier
            .verify(&config, Some(&db), access)
            .await
            .unwrap()
            .is_some());
        let resp = TestClient::post("http://localhost/api/oauth/clients/list")
            .bearer_auth(access)
            .json(&serde_json::json!({}))
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
        let resp = post_form(
            "http://localhost/oauth/revoke",
            form_body(&[
                ("token", refresh),
                ("client_id", &client.client_id),
                ("client_secret", &secret),
            ]),
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::OK));
        let resp = post_form(
            "http://localhost/oauth/token",
            form_body(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("client_id", &client.client_id),
                ("client_secret", &secret),
            ]),
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        db.set_user_disabled(&user.id, true, chrono::Utc::now().timestamp())
            .unwrap();
        assert!(OAuth2Verifier
            .verify(&config, Some(&db), access)
            .await
            .is_err());
        db.set_user_disabled(&user.id, false, chrono::Utc::now().timestamp())
            .unwrap();
        db.revoke_oauth_client_by_client_id(&client.client_id, chrono::Utc::now().timestamp())
            .unwrap();
        assert!(OAuth2Verifier
            .verify(&config, Some(&db), access)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn oauth_admin_authority_rejects_foreign_owner_and_malformed_subjects() {
    let config = test_config(oauth2_enabled());
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let other = seed_user(&db, "bob");
    let (client, _) = seed_client(&db, &user, "owned");
    let (record, token) = seed_access_token(&db, &client, &user, "runtime:read");
    db.conn_for_tests()
        .execute("UPDATE users SET role = 'admin' WHERE id = ?1", [&user.id])
        .unwrap();
    let legacy = OAuth2Verifier
        .verify(&config, Some(&db), &token)
        .await
        .unwrap()
        .unwrap();
    assert!(!legacy.is_admin_caller());
    assert!(!legacy.has_scope(crate::auth::SCOPE_ADMIN));
    // Corrupt durable rows directly to exercise verifier defense in depth,
    // including combinations normal issuance never produces.
    for (kind, subject, user_id, shared_hash) in [
        (
            "managed_user",
            other.id.as_str(),
            Some(other.id.as_str()),
            None,
        ),
        ("managed_user", "malformed", Some(user.id.as_str()), None),
        (
            "managed_user",
            user.id.as_str(),
            Some(user.id.as_str()),
            Some("stray-hash"),
        ),
        ("shared_key", "hash", None, Some("hash")),
        ("project_share", "malformed", None, None),
        ("unknown", "malformed", None, None),
    ] {
        db.conn_for_tests().execute("UPDATE oauth_access_tokens SET admin_authority = 1, subject_kind = ?1, subject_id = ?2, user_id = ?3, shared_key_hash = ?4 WHERE id = ?5", rusqlite::params![kind, subject, user_id, shared_hash, record.id]).unwrap();
        assert!(OAuth2Verifier
            .verify(&config, Some(&db), &token)
            .await
            .is_err());
    }
}

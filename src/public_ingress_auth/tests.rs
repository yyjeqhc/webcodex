use super::*;
use crate::auth::{hash_token, AuthEnvGuard, OAuth2Verifier, TokenVerifier};
use crate::models::{ApiKeyRecord, OAuthAuthorizationCodeRecord, OAuthClientRecord, UserRecord};
use crate::test_support::{seed_oauth_client_named, seed_user, test_config_oauth2, test_db};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;
use sha2::{Digest, Sha256};
use webcodex_store::{PublicIngressEntrySpec, PublicIngressMode};

const ORIGIN: &str = "https://mcp.example.test";
const REDIRECT: &str = "https://example.com/callback";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn public_entry(
    db: &crate::Database,
    user: &UserRecord,
    mode: PublicIngressMode,
    origin: &str,
) -> PublicIngressEntry {
    let entry = db
        .prepare_public_ingress_entry(
            &PublicIngressEntrySpec {
                entry_id: id(),
                profile_id: id(),
                mode,
                origin: origin.to_string(),
                owner_user_id: user.id.clone(),
                process_generation: 1,
                server_instance_id: "test-server-instance".into(),
            },
            None,
        )
        .unwrap()
        .unwrap();
    assert!(db
        .set_public_ingress_admission(&entry.fence(), true)
        .unwrap());
    db.get_public_ingress_entry(&entry.entry_id)
        .unwrap()
        .unwrap()
}

fn config(entry: Option<&PublicIngressEntry>, auth_enabled: bool) -> Arc<crate::Config> {
    let mut config = (*test_config_oauth2(auth_enabled.then_some("private-bootstrap"))).clone();
    config.oauth2.issuer = entry.map(|entry| entry.origin.clone());
    config.oauth2.require_pkce = true;
    config.oauth2.shared_key_bridge_enabled = false;
    Arc::new(config)
}

#[handler]
async fn authority(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
    let config = crate::auth::get_config(depot).unwrap();
    if let Err((status, code, _)) = require_authority(req, depot, &config) {
        res.status_code(StatusCode::from_u16(status).unwrap());
        res.render(Json(serde_json::json!({"error":code})));
        ctrl.skip_rest();
    } else {
        ctrl.call_next(req, depot, res).await;
    }
}

#[handler]
async fn echo(depot: &mut Depot, res: &mut Response) {
    res.render(Json(
        serde_json::json!({"ok":true,"public":entry(depot).is_some()}),
    ));
}

fn service(
    db: Arc<crate::Database>,
    public: Option<PublicIngressEntry>,
    enabled: bool,
    sessions: Arc<crate::oauth_http::AuthorizeSessionStore>,
) -> Service {
    let mut router = Router::new()
        .hoop(affix_state::inject(config(public.as_ref(), enabled)))
        .hoop(affix_state::inject(db))
        .hoop(affix_state::inject(sessions));
    if let Some(public) = public {
        router = router.hoop(affix_state::inject(PublicIngressRequest(public)));
    }
    Service::new(
        router
            .hoop(authority)
            .push(Router::with_path("authority").get(echo))
            .push(
                Router::with_path("mcp")
                    .hoop(crate::AuthMiddleware)
                    .get(echo)
                    .post(echo),
            )
            .push(
                Router::with_path("oauth/token")
                    .hoop(ClientIngressGate::ClientId)
                    .post(crate::oauth_http::oauth_token),
            )
            .push(
                Router::with_path("oauth/authorize")
                    .hoop(ClientIngressGate::ClientId)
                    .get(crate::oauth_http::oauth_authorize),
            )
            .push(
                Router::with_path("oauth/authorize/login")
                    .hoop(ClientIngressGate::LoginReturnTo)
                    .post(crate::oauth_http::oauth_authorize_login),
            )
            .push(
                Router::with_path("oauth/authorize/consent")
                    .hoop(ClientIngressGate::ClientId)
                    .post(crate::oauth_http::oauth_authorize_consent),
            ),
    )
}

fn sessions() -> Arc<crate::oauth_http::AuthorizeSessionStore> {
    Arc::new(crate::oauth_http::AuthorizeSessionStore::new())
}

fn mint_key(db: &crate::Database, user: &UserRecord, agent: bool) -> String {
    let token = if agent {
        crate::auth::generate_agent_token()
    } else {
        crate::auth::generate_api_token()
    };
    let record = ApiKeyRecord {
        id: id(),
        user_id: user.id.clone(),
        name: "ingress credential fixture".into(),
        key_prefix: crate::auth::token_prefix(&token),
        created_at: chrono::Utc::now().timestamp(),
        last_used_at: None,
        revoked_at: None,
        scopes: "runtime:read project:read".into(),
        expires_at: None,
        kind: if agent {
            crate::models::TOKEN_KIND_AGENT
        } else {
            crate::models::TOKEN_KIND_USER
        }
        .into(),
        allowed_client_id: agent.then(|| "fixture-runner".into()),
    };
    db.insert_api_key(&record, &hash_token(&token)).unwrap();
    token
}

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter().copied())
        .finish()
}

fn post_form(url: &str, body: String) -> salvo::test::RequestBuilder {
    TestClient::post(url)
        .add_header("content-type", "application/x-www-form-urlencoded", true)
        .body(body)
}

fn bound_client(
    db: &crate::Database,
    user: &UserRecord,
    entry: &PublicIngressEntry,
) -> (OAuthClientRecord, String) {
    let (client, secret) = seed_oauth_client_named(db, user, "entry-bound client");
    assert!(db
        .bind_oauth_client_to_public_ingress(&client.client_id, &entry.fence())
        .unwrap());
    (client, secret)
}

fn code(
    db: &crate::Database,
    user: &UserRecord,
    client: &OAuthClientRecord,
    entry: Option<&PublicIngressEntry>,
) -> String {
    let token = crate::auth::generate_oauth_authorization_code();
    let record = OAuthAuthorizationCodeRecord {
        id: id(),
        code_hash: hash_token(&token),
        client_id: client.client_id.clone(),
        subject_kind: "managed_user".into(),
        subject_id: user.id.clone(),
        user_id: Some(user.id.clone()),
        redirect_uri: REDIRECT.into(),
        scopes: "runtime:read project:read".into(),
        resource: Some(format!("{ORIGIN}/mcp")),
        code_challenge: Some(URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER))),
        code_challenge_method: Some("S256".into()),
        shared_key_hash: None,
        created_at: chrono::Utc::now().timestamp(),
        expires_at: chrono::Utc::now().timestamp() + 300,
        used_at: None,
        revoked_at: None,
        admin_authority: false,
    };
    if let Some(entry) = entry {
        assert!(db
            .insert_public_ingress_oauth_authorization_code(
                &record,
                &record.code_hash,
                &entry.fence()
            )
            .unwrap());
    } else {
        db.insert_oauth_authorization_code(&record, &record.code_hash)
            .unwrap();
    }
    token
}

fn exchange_form(client: &OAuthClientRecord, secret: &str, code: &str) -> String {
    form(&[
        ("grant_type", "authorization_code"),
        ("client_id", &client.client_id),
        ("client_secret", secret),
        ("code", code),
        ("redirect_uri", REDIRECT),
        ("code_verifier", VERIFIER),
    ])
}

async fn exchange(
    service: &Service,
    url: &str,
    client: &OAuthClientRecord,
    secret: &str,
    code: &str,
) -> serde_json::Value {
    let mut response = post_form(url, exchange_form(client, secret, code))
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    response.take_json().await.unwrap()
}

#[tokio::test]
async fn public_ingress_authority_cannot_be_forged_by_request_headers() {
    let _env = AuthEnvGuard::auth_required();
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "owner");
    let entry = public_entry(&db, &user, PublicIngressMode::Named, ORIGIN);
    let public = service(db.clone(), Some(entry), true, sessions());
    let private = service(db, None, true, sessions());
    let response = TestClient::get(&format!("{ORIGIN}/authority"))
        .send(&public)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    for (host, origin, status) in [
        ("evil.example.test", ORIGIN, StatusCode::FORBIDDEN),
        ("localhost", ORIGIN, StatusCode::FORBIDDEN),
        (
            "mcp.example.test",
            "https://evil.example.test",
            StatusCode::FORBIDDEN,
        ),
        (
            "mcp.example.test",
            "http://mcp.example.test",
            StatusCode::FORBIDDEN,
        ),
        (
            "mcp.example.test",
            "https://mcp.example.test/path",
            StatusCode::FORBIDDEN,
        ),
        ("mcp.example.test", "null", StatusCode::BAD_REQUEST),
    ] {
        let response = TestClient::get(&format!("{ORIGIN}/authority"))
            .add_header("host", host, true)
            .add_header("origin", origin, true)
            .add_header("x-forwarded-host", "mcp.example.test", true)
            .add_header("forwarded", "host=mcp.example.test;proto=https", true)
            .send(&public)
            .await;
        assert_eq!(response.status_code, Some(status));
    }
    let mut response = TestClient::get("http://localhost/authority")
        .add_header("x-forwarded-host", "mcp.example.test", true)
        .add_header("x-forwarded-proto", "https", true)
        .add_header("x-webcodex-public-ingress", "true", true)
        .add_header("cf-connecting-ip", "198.51.100.4", true)
        .send(&private)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(
        response.take_json::<serde_json::Value>().await.unwrap()["public"],
        false
    );
    let response = TestClient::get("http://localhost/authority")
        .add_header("host", "mcp.example.test", true)
        .add_header("x-forwarded-host", "localhost", true)
        .send(&private)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::FORBIDDEN));
}

#[tokio::test]
async fn public_ingress_mcp_rejects_non_oauth_even_when_private_auth_is_open() {
    let env = AuthEnvGuard::auth_required();
    env.enable_direct_shared_key();
    env.enable_open_anonymous();
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "owner");
    let public = public_entry(&db, &user, PublicIngressMode::Named, ORIGIN);
    let account = crate::auth::generate_account_credential();
    db.insert_account_credential(
        &crate::models::AccountCredentialRecord {
            id: id(),
            user_id: user.id.clone(),
            credential_prefix: crate::auth::token_prefix(&account),
            created_at: chrono::Utc::now().timestamp(),
            last_used_at: None,
            revoked_at: None,
        },
        &hash_token(&account),
    )
    .unwrap();
    let pat = mint_key(&db, &user, false);
    let agent = mint_key(&db, &user, true);
    let tokens = [
        "private-bootstrap".to_string(),
        pat,
        agent,
        account,
        "arbitrary-shared-key".into(),
        format!("webcodex_{}", "a".repeat(64)),
        "wc_oat_unknown".into(),
    ];
    for enabled in [true, false] {
        let service = service(db.clone(), Some(public.clone()), enabled, sessions());
        for token in &tokens {
            let mut response = TestClient::get(&format!("{ORIGIN}/mcp"))
                .bearer_auth(token)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
            assert!(response
                .headers()
                .get("www-authenticate")
                .unwrap()
                .to_str()
                .unwrap()
                .contains(ORIGIN));
            assert!(!response.take_string().await.unwrap().contains(token));
        }
        let response = TestClient::get(&format!("{ORIGIN}/mcp"))
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
        let response = TestClient::get(&format!("{ORIGIN}/mcp?token=private-bootstrap"))
            .add_header("cookie", "webcodex_authorize_session=fake-session", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
    }
}

#[tokio::test]
async fn oauth_route_aliases_preserve_listener_audience_without_consuming_grants() {
    let _env = AuthEnvGuard::auth_required();
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "alias-owner");
    let entry = public_entry(&db, &user, PublicIngressMode::Named, ORIGIN);
    let (bound, bound_secret) = bound_client(&db, &user, &entry);
    let (unbound, unbound_secret) = seed_oauth_client_named(&db, &user, "private alias client");
    let public = service(db.clone(), Some(entry.clone()), true, sessions());
    let private = service(db.clone(), None, true, sessions());

    for path in [
        "/oauth/token",
        "/oauth/token/",
        "/oauth//token",
        "/oauth/%74oken",
    ] {
        for (
            client,
            secret,
            binding,
            denied_service,
            denied_origin,
            allowed_service,
            allowed_origin,
        ) in [
            (
                &bound,
                &bound_secret,
                Some(&entry),
                &private,
                "http://localhost",
                &public,
                ORIGIN,
            ),
            (
                &unbound,
                &unbound_secret,
                None,
                &public,
                ORIGIN,
                &private,
                "http://localhost",
            ),
        ] {
            let authorization_code = code(&db, &user, client, binding);
            let response = post_form(
                &format!("{denied_origin}{path}"),
                exchange_form(client, secret, &authorization_code),
            )
            .send(denied_service)
            .await;
            let consumed = db
                .get_oauth_authorization_code_by_hash(&hash_token(&authorization_code))
                .unwrap()
                .unwrap()
                .used_at
                .is_some();
            assert_eq!(
                (response.status_code, consumed),
                (Some(StatusCode::BAD_REQUEST), false),
                "code exchange changed audience at {denied_origin}{path}"
            );

            let tokens = exchange(
                allowed_service,
                &format!("{allowed_origin}/oauth/token"),
                client,
                secret,
                &authorization_code,
            )
            .await;
            let refresh = tokens["refresh_token"].as_str().unwrap();
            let response = post_form(
                &format!("{denied_origin}{path}"),
                form(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                    ("client_id", &client.client_id),
                    ("client_secret", secret),
                ]),
            )
            .send(denied_service)
            .await;
            let retained = db
                .get_oauth_refresh_token_by_hash(&hash_token(refresh))
                .unwrap();
            assert_eq!(
                (response.status_code, retained.is_some()),
                (Some(StatusCode::BAD_REQUEST), true),
                "refresh changed audience at {denied_origin}{path}"
            );
            assert!(retained.unwrap().last_used_at.is_none());
        }
    }
}

#[tokio::test]
async fn entry_bound_code_exchange_and_tokens_cannot_cross_private_listener() {
    let _env = AuthEnvGuard::auth_required();
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "owner");
    let entry = public_entry(&db, &user, PublicIngressMode::Named, ORIGIN);
    let (client, secret) = bound_client(&db, &user, &entry);
    let authorization_code = code(&db, &user, &client, Some(&entry));
    let public = service(db.clone(), Some(entry.clone()), true, sessions());
    let private = service(db.clone(), None, true, sessions());
    let response = post_form(
        "http://localhost/oauth/token",
        exchange_form(&client, &secret, &authorization_code),
    )
    .add_header("x-forwarded-host", "mcp.example.test", true)
    .send(&private)
    .await;
    assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
    assert!(db
        .get_oauth_authorization_code_by_hash(&hash_token(&authorization_code))
        .unwrap()
        .unwrap()
        .used_at
        .is_none());
    let tokens = exchange(
        &public,
        &format!("{ORIGIN}/oauth/token"),
        &client,
        &secret,
        &authorization_code,
    )
    .await;
    let access = tokens["access_token"].as_str().unwrap();
    let refresh = tokens["refresh_token"].as_str().unwrap();
    assert_eq!(tokens["scope"], "runtime:read project:read");
    let public_config = config(Some(&entry), true);
    assert!(OAuth2Verifier::verify_for_ingress(
        &public_config,
        Some(&db),
        access,
        Some(&entry.fence())
    )
    .await
    .unwrap()
    .is_some());
    assert!(OAuth2Verifier
        .verify(&public_config, Some(&db), access)
        .await
        .is_err());
    let response = TestClient::get(&format!("{ORIGIN}/mcp"))
        .bearer_auth(access)
        .send(&public)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let response = TestClient::get("http://localhost/mcp")
        .bearer_auth(access)
        .add_header("x-webcodex-public-ingress", "true", true)
        .send(&private)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
    let response = post_form(
        "http://localhost/oauth/token",
        form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", &client.client_id),
            ("client_secret", &secret),
        ]),
    )
    .send(&private)
    .await;
    assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
    assert!(db
        .get_oauth_refresh_token_by_hash(&hash_token(refresh))
        .unwrap()
        .is_some());
    // A private resource-bound OAuth grant still has the private listener audience.
    let (legacy_client, legacy_secret) = seed_oauth_client_named(&db, &user, "private client");
    let legacy_code = code(&db, &user, &legacy_client, None);
    let legacy_tokens = exchange(
        &private,
        "http://localhost/oauth/token",
        &legacy_client,
        &legacy_secret,
        &legacy_code,
    )
    .await;
    let legacy_access = legacy_tokens["access_token"].as_str().unwrap();
    let response = TestClient::get(&format!("{ORIGIN}/mcp"))
        .bearer_auth(legacy_access)
        .send(&public)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
    assert!(OAuth2Verifier
        .verify(&config(None, true), Some(&db), legacy_access)
        .await
        .unwrap()
        .is_some());
    assert!(db
        .set_public_ingress_admission(&entry.fence(), false)
        .unwrap());
    let response = TestClient::get(&format!("{ORIGIN}/mcp"))
        .bearer_auth(access)
        .send(&public)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
}

fn authorize_parameters(client: &OAuthClientRecord, decision: Option<&str>) -> String {
    let mut pairs = vec![
        ("response_type", "code"),
        ("client_id", client.client_id.as_str()),
        ("redirect_uri", REDIRECT),
        ("scope", "runtime:read"),
        ("state", "test-state"),
        ("code_challenge", "challenge"),
        ("code_challenge_method", "S256"),
    ];
    if let Some(decision) = decision {
        pairs.push(("decision", decision));
    }
    form(&pairs)
}

#[tokio::test]
async fn public_authorize_browser_session_is_bound_to_entry_and_epoch() {
    let _env = AuthEnvGuard::auth_required();
    let (_tmp, db) = test_db();
    let user = seed_user(&db, "owner");
    let first = public_entry(&db, &user, PublicIngressMode::Quick, ORIGIN);
    let (client, _) = bound_client(&db, &user, &first);
    let pat = mint_key(&db, &user, false);
    let sessions = sessions();
    let public = service(db.clone(), Some(first.clone()), true, sessions.clone());
    let return_to = format!("/oauth/authorize?{}", authorize_parameters(&client, None));
    let foreign_user = seed_user(&db, "foreign-user");
    let foreign_pat = mint_key(&db, &foreign_user, false);
    let response = post_form(
        &format!("{ORIGIN}/oauth/authorize/login"),
        form(&[("return_to", &return_to), ("token", &foreign_pat)]),
    )
    .send(&public)
    .await;
    assert_eq!(response.status_code, Some(StatusCode::FORBIDDEN));
    assert!(response.headers().get("set-cookie").is_none());
    let response = post_form(
        &format!("{ORIGIN}/oauth/authorize/login"),
        form(&[("return_to", &return_to), ("token", &pat)]),
    )
    .send(&public)
    .await;
    assert_eq!(response.status_code, Some(StatusCode::FOUND));
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("Secure"));
    assert!(!set_cookie.contains(&pat));
    let cookie = set_cookie.split(';').next().unwrap().to_string();
    let response = post_form(
        &format!("{ORIGIN}/oauth/authorize/consent"),
        authorize_parameters(&client, Some("allow")),
    )
    .add_header("cookie", &cookie, true)
    .send(&public)
    .await;
    assert_eq!(response.status_code, Some(StatusCode::FOUND));
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("code="));
    let (private_client, _) = seed_oauth_client_named(&db, &user, "private client");
    let private = service(db.clone(), None, true, sessions.clone());
    let response = post_form(
        "http://localhost/oauth/authorize/consent",
        authorize_parameters(&private_client, Some("allow")),
    )
    .add_header("cookie", &cookie, true)
    .send(&private)
    .await;
    assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
    let replacement = db
        .prepare_public_ingress_entry(
            &PublicIngressEntrySpec {
                entry_id: first.entry_id.clone(),
                profile_id: first.profile_id.clone(),
                mode: PublicIngressMode::Quick,
                origin: ORIGIN.into(),
                owner_user_id: user.id.clone(),
                process_generation: 2,
                server_instance_id: first.server_instance_id.clone(),
            },
            Some(first.runtime_revision),
        )
        .unwrap()
        .unwrap();
    assert_ne!(replacement.auth_epoch, first.auth_epoch);
    assert!(db
        .set_public_ingress_admission(&replacement.fence(), true)
        .unwrap());
    let changed_epoch = service(db.clone(), Some(replacement), true, sessions.clone());
    let response = post_form(
        &format!("{ORIGIN}/oauth/authorize/consent"),
        authorize_parameters(&client, Some("allow")),
    )
    .add_header("cookie", &cookie, true)
    .send(&changed_epoch)
    .await;
    assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
    let other = public_entry(
        &db,
        &user,
        PublicIngressMode::Named,
        "https://other.example.test",
    );
    let (other_client, _) = bound_client(&db, &user, &other);
    let other_service = service(db, Some(other), true, sessions);
    let response = post_form(
        "https://other.example.test/oauth/authorize/consent",
        authorize_parameters(&other_client, Some("allow")),
    )
    .add_header("cookie", &cookie, true)
    .send(&other_service)
    .await;
    assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
}

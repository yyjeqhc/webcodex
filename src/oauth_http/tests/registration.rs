use super::*;
use serde_json::{json, Value};

const CALLBACK: &str = "https://oauth-redirect.googleusercontent.com/r/test-app";
const ISSUER: &str = "https://webcodex.example";
const REGISTER: &str = "http://localhost/oauth/register";
const TOKEN: &str = "http://localhost/oauth/token";
const REVOKE: &str = "http://localhost/oauth/revoke";

fn dcr_config(owner: &UserRecord) -> Arc<crate::Config> {
    test_config(OAuth2Config {
        dcr_owner_user_id: Some(owner.id.clone()),
        ..oauth2_enabled_with_issuer(ISSUER)
    })
}

async fn register(service: &Service, metadata: Value) -> Value {
    let mut response = TestClient::post(REGISTER)
        .json(&metadata)
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::CREATED));
    assert_eq!(response.headers()["cache-control"], "no-store");
    response.take_json().await.unwrap()
}

#[tokio::test]
async fn registration_discovery_and_validation_are_scoped_to_managed_https_issuer() {
    let (_tmp, db) = test_db();
    let owner = seed_user(&db, "owner");
    let enabled = dcr_config(&owner);
    for (oauth, available) in [
        (enabled.oauth2.clone(), true),
        (oauth2_enabled_with_issuer(ISSUER), false),
        (
            OAuth2Config {
                enabled: false,
                ..enabled.oauth2.clone()
            },
            false,
        ),
        (
            OAuth2Config {
                issuer: Some("http://localhost".into()),
                ..enabled.oauth2.clone()
            },
            false,
        ),
        (
            OAuth2Config {
                project_share_grant_id: Some(TEST_PROJECT_GRANT_ID.into()),
                ..enabled.oauth2.clone()
            },
            false,
        ),
    ] {
        let service = Service::new(build_router(test_config(oauth.clone()), db.clone()));
        let mut resp = TestClient::get("http://localhost/.well-known/oauth-authorization-server")
            .send(&service)
            .await;
        let metadata: Value = resp.take_json().await.unwrap();
        if available {
            assert_eq!(
                metadata["registration_endpoint"],
                format!("{ISSUER}/oauth/register")
            );
            for method in ["none", "client_secret_basic", "client_secret_post"] {
                assert!(metadata["token_endpoint_auth_methods_supported"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == method));
            }
        } else {
            assert!(metadata.get("registration_endpoint").is_none());
            let resp = TestClient::post(REGISTER)
                .json(&json!({"redirect_uris":[CALLBACK]}))
                .send(&service)
                .await;
            assert_eq!(resp.status_code, Some(StatusCode::NOT_FOUND));
        }
    }
    let service = Service::new(build_router(enabled, db.clone()));
    for (fields, error) in [
        (json!({"redirect_uris":[]}), "invalid_redirect_uri"),
        (
            json!({"redirect_uris":["http://remote.example/cb"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris":["https://example.com/cb#bad"]}),
            "invalid_redirect_uri",
        ),
        (json!({"scope":"admin"}), "invalid_client_metadata"),
        (json!({"scope":"account:manage"}), "invalid_client_metadata"),
        (
            json!({"grant_types":["client_credentials"]}),
            "invalid_client_metadata",
        ),
        (
            json!({"response_types":["token"]}),
            "invalid_client_metadata",
        ),
        (
            json!({"token_endpoint_auth_method":"unknown"}),
            "invalid_client_metadata",
        ),
    ] {
        let mut body = json!({"redirect_uris":[CALLBACK]});
        body.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let mut resp = TestClient::post(REGISTER).json(&body).send(&service).await;
        assert_eq!(resp.status_code, Some(StatusCode::BAD_REQUEST));
        assert_eq!(resp.take_json::<Value>().await.unwrap()["error"], error);
    }
    let resp = TestClient::post(REGISTER)
        .json(&json!({"redirect_uris":[CALLBACK]}))
        .add_header("origin", "https://another.example", true)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
    let resp = TestClient::post(REGISTER)
        .json(&json!({"client_name":"x".repeat(17*1024)}))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
    db.set_user_disabled(&owner.id, true, chrono::Utc::now().timestamp())
        .unwrap();
    let resp = TestClient::post(REGISTER)
        .json(&json!({"redirect_uris":[CALLBACK]}))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
    assert!(db.list_oauth_clients().unwrap().is_empty());
}

#[tokio::test]
async fn registered_confidential_client_browser_consent_basic_exchange_refresh_and_revoke() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    let (tmp, db) = test_db();
    let owner = seed_user(&db, "owner");
    let config = dcr_config(&owner);
    let service = Service::new(build_router(config.clone(), db.clone()));
    let reg = register(
        &service,
        json!({"redirect_uris":[CALLBACK], "client_name":"Gemini fixture",
        "grant_types":["authorization_code","refresh_token"]}),
    )
    .await;
    assert_eq!(reg["token_endpoint_auth_method"], "client_secret_basic");
    assert_eq!(reg["client_secret_expires_at"], 0);
    let id = reg["client_id"].as_str().unwrap();
    let secret = reg["client_secret"].as_str().unwrap();
    assert!(!reg.as_object().unwrap().contains_key("access_token"));
    let stored = db.get_oauth_client_by_client_id(id).unwrap().unwrap();
    assert_eq!(stored.client_secret_hash, hash_token(secret));
    assert_eq!(stored.owner_user_id, Some(owner.id.clone()));
    let verifier = "p".repeat(43);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let requested_scopes = oauth_discovery_scopes_supported().join(" ");
    let params = [
        ("client_id", id),
        ("response_type", "code"),
        ("redirect_uri", CALLBACK),
        ("scope", &requested_scopes),
        ("resource", "https://webcodex.example/mcp"),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("state", "opaque-state"),
    ];
    let query = form_body(&params);
    let return_to = format!("/oauth/authorize?{query}");
    let other = seed_user(&db, "other");
    let foreign = seed_user_token_with_scopes(&db, &other, "admin");
    let resp = TestClient::get(&authorize_url(&params))
        .bearer_auth(&foreign)
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::FORBIDDEN));
    let pat = seed_user_token_with_scopes(&db, &owner, "admin");
    let login = post_form(
        "http://localhost/oauth/authorize/login",
        form_body(&[("token", &pat), ("return_to", &return_to)]),
    )
    .send(&service)
    .await;
    assert_eq!(login.status_code, Some(StatusCode::FOUND));
    let cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let mut consent = TestClient::get(&authorize_url(&params))
        .add_header("cookie", cookie, true)
        .send(&service)
        .await;
    let page = consent.take_string().await.unwrap();
    assert!(
        page.contains("Self-registered client")
            && page.contains(CALLBACK)
            && page.contains("runtime:read")
    );
    assert_eq!(auth_code_count(&db), 0);
    let approve = post_form(
        "http://localhost/oauth/authorize/consent",
        format!("{query}&decision=allow"),
    )
    .add_header("cookie", cookie, true)
    .send(&service)
    .await;
    assert_eq!(approve.status_code, Some(StatusCode::FOUND));
    let redirect = url::Url::parse(&location_header(&approve).unwrap()).unwrap();
    let code = redirect
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    let issued = auth_code_by_plaintext(&db, &code);
    assert!(!issued.admin_authority);
    let expected_scope = format!(
        "{} offline_access",
        webcodex_core::authority::profiles::LOCAL_USER.join(" ")
    );
    assert_eq!(issued.scopes, expected_scope);
    // All durable state needed for exchange survives a fresh database connection.
    drop(service);
    drop(db);
    let db = Arc::new(crate::Database::open(&tmp.path().join("oauth.db")).unwrap());
    let service = Service::new(
        build_router(config.clone(), db.clone())
            .hoop(affix_state::inject(Arc::new(
                crate::tool_runtime::ToolRuntime::new_for_tests(),
            )))
            .push(
                Router::with_path("mcp")
                    .hoop(crate::AuthMiddleware)
                    .post(crate::mcp::mcp_post),
            ),
    );
    let exchange = form_body(&[
        ("code", &code),
        ("redirect_uri", CALLBACK),
        ("code_verifier", &verifier),
        ("grant_type", "authorization_code"),
    ]);
    let wrong_secret = post_form(TOKEN, exchange.clone())
        .basic_auth(id, Some("wrong"))
        .send(&service)
        .await;
    assert_eq!(wrong_secret.status_code, Some(StatusCode::UNAUTHORIZED));
    assert!(wrong_secret.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .starts_with("Basic "));
    let wrong_method = post_form(
        TOKEN,
        format!(
            "{exchange}&{}",
            form_body(&[("client_id", id), ("client_secret", secret)])
        ),
    )
    .send(&service)
    .await;
    assert_eq!(wrong_method.status_code, Some(StatusCode::UNAUTHORIZED));
    let mut resp = post_form(TOKEN, exchange.clone())
        .basic_auth(id, Some(secret))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    let tokens: Value = resp.take_json().await.unwrap();
    assert_eq!(tokens["scope"], expected_scope);
    let replay = post_form(TOKEN, exchange)
        .basic_auth(id, Some(secret))
        .send(&service)
        .await;
    assert_eq!(replay.status_code, Some(StatusCode::BAD_REQUEST));
    let refresh = tokens["refresh_token"].as_str().unwrap();
    let mut rotated = post_form(
        TOKEN,
        form_body(&[("refresh_token", refresh), ("grant_type", "refresh_token")]),
    )
    .basic_auth(id, Some(secret))
    .send(&service)
    .await;
    assert_eq!(rotated.status_code, Some(StatusCode::OK));
    let new_tokens: Value = rotated.take_json().await.unwrap();
    assert!(new_tokens["refresh_token"]
        .as_str()
        .is_some_and(|r| r != refresh));
    let access = new_tokens["access_token"].as_str().unwrap();
    let auth = OAuth2Verifier
        .verify(&config, Some(&db), access)
        .await
        .unwrap()
        .unwrap();
    assert!(auth.has_scope("runtime:read"));
    assert!(!auth.is_admin());
    assert!(!auth.has_scope("account:manage"));
    assert!(!auth.has_scope("computer:control"));
    let mut discovery = TestClient::post("http://localhost/mcp")
        .bearer_auth(access)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}))
        .send(&service)
        .await;
    assert_eq!(discovery.status_code, Some(StatusCode::OK));
    let listing: Value = discovery.take_json().await.unwrap();
    assert!(listing["result"]["tools"]
        .as_array()
        .is_some_and(|tools| !tools.is_empty()));
    let mut changed = (*config).clone();
    changed.oauth2.issuer = Some("https://other.example".into());
    assert!(OAuth2Verifier
        .verify(&changed, Some(&db), access)
        .await
        .is_err());
    let resp = post_form(REVOKE, form_body(&[("token", access)]))
        .basic_auth(id, Some(secret))
        .send(&service)
        .await;
    assert_eq!(resp.status_code, Some(StatusCode::OK));
    assert!(OAuth2Verifier
        .verify(&config, Some(&db), access)
        .await
        .is_err());
}

#[tokio::test]
async fn code_only_registration_enforces_its_auth_method_pkce_and_exact_redirect() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    let (_tmp, db) = test_db();
    let owner = seed_user(&db, "owner");
    let pat = seed_user_token_with_scopes(&db, &owner, "runtime:read");
    let service = Service::new(build_router(dcr_config(&owner), db.clone()));
    let verifier = "v".repeat(43);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    for method in ["none", "client_secret_post"] {
        let reg = register(&service,json!({"redirect_uris":[CALLBACK],"scope":"runtime:read","token_endpoint_auth_method":method})).await;
        let id = reg["client_id"].as_str().unwrap();
        assert_eq!(reg["grant_types"], json!(["authorization_code"]));
        assert_eq!(reg.get("client_secret").is_none(), method == "none");
        let url = |uri, challenge| {
            authorize_url(&[
                ("client_id", id),
                ("response_type", "code"),
                ("redirect_uri", uri),
                ("code_challenge", challenge),
                ("code_challenge_method", "S256"),
            ])
        };
        let wrong_redirect =
            authorized_get(&url("https://example.com/elsewhere", &challenge), &pat)
                .send(&service)
                .await;
        assert_eq!(wrong_redirect.status_code, Some(StatusCode::BAD_REQUEST));
        let bad_challenge = authorized_get(&url(CALLBACK, "short"), &pat)
            .send(&service)
            .await;
        assert!(location_header(&bad_challenge)
            .unwrap()
            .contains("error=invalid_request"));
        for valid_verifier in [false, true] {
            let (_, _, _, code) =
                authorize_success(&service, &db, &url(CALLBACK, &challenge), &pat).await;
            let mut params = vec![
                ("client_id", id),
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", CALLBACK),
                (
                    "code_verifier",
                    if valid_verifier { &verifier } else { "wrong" },
                ),
            ];
            if let Some(secret) = reg["client_secret"].as_str() {
                let denied = post_form(TOKEN, form_body(&params)).send(&service).await;
                assert_eq!(denied.status_code, Some(StatusCode::UNAUTHORIZED));
                params.push(("client_secret", secret));
            }
            let mut resp = post_form(TOKEN, form_body(&params)).send(&service).await;
            assert_eq!(
                resp.status_code,
                Some(if valid_verifier {
                    StatusCode::OK
                } else {
                    StatusCode::BAD_REQUEST
                })
            );
            let body: Value = resp.take_json().await.unwrap();
            assert!(body.get("refresh_token").is_none());
            assert_eq!(body.get("access_token").is_some(), valid_verifier);
        }
    }
}

#[test]
fn dynamic_registration_capacity_is_atomic_and_reclaims_only_idle_clients() {
    use crate::models::{DynamicClientAuth, DynamicClientRegistration};
    use webcodex_store::DynamicRegistrationOutcome::{AtCapacity, Created};
    let (tmp, db) = test_db();
    let owner = seed_user(&db, "owner");
    let (manual, _) = seed_client(&db, &owner, "manual");
    let now = chrono::Utc::now().timestamp();
    let make = |created_at| OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        dynamic_registration: Some(DynamicClientRegistration {
            auth_method: DynamicClientAuth::Post,
            refresh: true,
            issuer: ISSUER.into(),
        }),
        created_at,
        ..manual.clone()
    };
    let protected = make(now - 86402);
    assert_eq!(
        db.register_dynamic_oauth_client(&protected, None).unwrap(),
        Created
    );
    seed_refresh_token(&db, &protected, &owner, "runtime:read");
    let idle = make(now - 86401);
    assert_eq!(
        db.register_dynamic_oauth_client(&idle, None).unwrap(),
        Created
    );
    for _ in 0..125 {
        assert_eq!(
            db.register_dynamic_oauth_client(&make(now), None).unwrap(),
            Created
        );
    }
    // Two independent writers compete for the last free and last reclaimable slot.
    let other = crate::Database::open(&tmp.path().join("oauth.db")).unwrap();
    std::thread::scope(|s| {
        let request = make(now);
        let t = s.spawn(move || other.register_dynamic_oauth_client(&request, None).unwrap());
        assert_eq!(
            db.register_dynamic_oauth_client(&make(now), None).unwrap(),
            Created
        );
        assert_eq!(t.join().unwrap(), Created);
    });
    assert_eq!(
        db.register_dynamic_oauth_client(&make(now), None).unwrap(),
        AtCapacity
    );
    assert!(db
        .get_oauth_client_by_client_id(&idle.client_id)
        .unwrap()
        .is_none());
    for client in [manual, protected] {
        assert!(db
            .get_oauth_client_by_client_id(&client.client_id)
            .unwrap()
            .is_some());
    }
}
